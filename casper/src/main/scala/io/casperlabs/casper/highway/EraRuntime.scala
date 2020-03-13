package io.casperlabs.casper.highway

import cats._
import cats.data.{EitherT, WriterT}
import cats.implicits._
import cats.effect.{Clock, Concurrent, Sync}
import cats.effect.concurrent.{Ref, Semaphore}
import com.google.protobuf.ByteString
import java.time.Instant

import io.casperlabs.casper.consensus.{Block, BlockSummary, Era}
import io.casperlabs.casper.dag.DagOperations
import io.casperlabs.casper.validation.Errors.ErrorMessageWrapper
import io.casperlabs.catscontrib.{MakeSemaphore, MonadThrowable}
import io.casperlabs.crypto.Keys.{PublicKey, PublicKeyBS}
import io.casperlabs.models.Message
import io.casperlabs.metrics.implicits._
import io.casperlabs.metrics.Metrics
import io.casperlabs.shared.Log
import io.casperlabs.storage.BlockHash
import io.casperlabs.storage.dag.{DagRepresentation, DagStorage, FinalityStorageReader}
import io.casperlabs.storage.era.EraStorage
import io.casperlabs.shared.SemaphoreMap

import scala.util.Random
import scala.util.control.NoStackTrace

/** Class to encapsulate the message handling logic of messages in an era.
  *
  * It can create blocks/ballots and persist them, even new eras,
  * but it should not have externally visible side effects, i.e.
  * not communicate over the network.
  *
  * Persisting messages is fine: if we want to handle messages concurrently
  * we have to keep track of the validator sequence number and make sure
  * we cite our own previous messages.
  *
  * The handler methods will return a list of domain events,
  * and a list of future actions in the form of an ADT;
  * it's up to the supervisor protocol to broadcast them
  * and schedule followup actions.
  *
  * This should make testing easier: the return values are not opaque.
  */
class EraRuntime[F[_]: Sync: Clock: Metrics: Log: EraStorage: FinalityStorageReader: ForkChoice](
    conf: HighwayConf,
    val era: Era,
    leaderFunction: LeaderFunction,
    roundExponentRef: Ref[F, Int],
    maybeMessageProducer: Option[MessageProducer[F]],
    // Make sure only one message is created by us at any time to avoid an equivocation.
    // That includes collecting justifications and making a block or ballot: if we say
    // our previous message was X, and make a message X <- Y, we must not at the same time
    // make a message X <- Z. Since we let the fork choice tell us what justifications to use,
    // we must protect the fork choice _and_ the message production with a common semaphore.
    // Also make sure that we only add one message from another validator at a time, so as
    // not to miss any equivocations that may happen if they arrive at the same time.
    validatorSemaphoreMap: SemaphoreMap[F, PublicKeyBS],
    // Indicate whether the initial syncing mechanism that runs when the node is started
    // is still ongoing, or it has concluded and the node is caught up with its peers.
    // Before that, responding to messages risks creating an equivocation, in case some
    // state was somehow lost after a restart. It could also force others to deal with
    // messages build on blocks long gone and check a huge swathe of the DAG for merge
    // conflicts.
    isSynced: => F[Boolean],
    dag: DagRepresentation[F],
    // Random number generator used to pick the omega delay.
    rng: Random = new Random()
) {
  import EraRuntime._, Agenda._
  import HighwayConf.VotingDuration

  implicit val metricsSource = HighwayMetricsSource / "EraRuntime"

  type HWL[A] = HighwayLog[F, A]
  private val noop = HighwayLog.unit[F]

  val startTick = Ticks(era.startTick)
  val endTick   = Ticks(era.endTick)
  val start     = conf.toInstant(startTick)
  val end       = conf.toInstant(endTick)

  val bookingBoundaries =
    conf.criticalBoundaries(start, end, delayDuration = conf.bookingDuration)

  val keyBoundaries =
    conf.criticalBoundaries(start, end, delayDuration = conf.keyDuration)

  /** When we handle an incoming block or create a new one we may have to do additional work:
    * - if the block is a booking block, we have to execute the auction to pick the validators for the upcoming era
    * - when see a switch block, we have to look for a main ancestor which was a key block, to decide which era it belongs to.
    * These blocks can be identified by their round ID being after a boundary (a deadline),
    * while their main parent was still before the deadline.
    */
  private def isBoundary(boundaries: List[Instant])(
      mainParentBlockRound: Instant,
      blockRound: Instant
  ) = boundaries.exists(isCrossing(_)(mainParentBlockRound, blockRound))

  val isBookingBoundary = isBoundary(bookingBoundaries)(_, _)
  val isKeyBoundary     = isBoundary(keyBoundaries)(_, _)

  /** Switch blocks are the first blocks which are created _after_ the era ends.
    * They are still created by the validators of this era, and they signal the
    * end of the era. Switch blocks are what the child era is going to build on,
    * however, the validators of _this_ era are the ones that can finalize it by
    * building ballots on top of it. They cannot build more blocks on them though.
    */
  val isSwitchBoundary = isCrossing(end)(_, _)

  private implicit class MessageOps(msg: Message) {

    /** Convert the round of the message to a time equivalent for comparisons with critical block boundaries. */
    def roundInstant: Instant =
      conf.toInstant(Ticks(msg.roundId))

    /** Check if this block is the first in its main-chain that crosses the end of the era. */
    def isSwitchBlock: F[Boolean] = isBoundaryBlock(isSwitchBoundary)

    /** Check if this block is one that crosses an era booking boundary, therefor has to execute the auction. */
    def isBookingBlock: F[Boolean] = isBoundaryBlock(isBookingBoundary)

    /** Check whether the parent to child transition crosses a boundary. */
    private def isBoundaryBlock(isBoundary: (Instant, Instant) => Boolean): F[Boolean] =
      if (msg.parentBlock.isEmpty || !msg.isInstanceOf[Message.Block])
        false.pure[F]
      else
        dag
          .lookupUnsafe(msg.parentBlock)
          .map(parent => isBoundary(parent.roundInstant, msg.roundInstant))

    /** Check whether a block or ballot is a lambda message. */
    def isLambdaMessage: F[Boolean] =
      if (leaderFunction(Ticks(msg.roundId)) == msg.validatorId)
        msg match {
          case _: Message.Block  => true.pure[F]
          case b: Message.Ballot => isLambdaLikeBallot(dag, b, endTick)
        }
      else false.pure[F]
  }

  private implicit class MessageProducerOps(mp: MessageProducer[F]) {
    def withPermit[T](block: F[T]): F[T] =
      validatorSemaphoreMap.withPermit(mp.validatorId)(block)
  }

  /** Current tick based on wall clock time. */
  private def currentTick =
    Clock[F].realTime(conf.tickUnit).map(Ticks(_))

  /** Check if the era is finished at a given tick, including the post-era voting period. */
  private val isEraOverAt: Ticks => F[Boolean] =
    conf.postEraVotingDuration match {
      case VotingDuration.FixedLength(duration) =>
        val votingEndTick = conf.toTicks(end plus duration)
        tick => (tick >= votingEndTick).pure[F]

      case VotingDuration.SummitLevel(1) =>
        // We have to keep voting until the switch block given by the fork choice
        // in *this* era is finalized.
        tick =>
          if (tick < endTick) false.pure[F]
          else {
            ForkChoice[F]
              .fromKeyBlock(era.keyBlockHash)
              .timerGauge("is_era_over_forkchoice")
              .flatMap { choice =>
                choice.block.isSwitchBlock.ifM(
                  FinalityStorageReader[F].isFinalized(choice.block.messageHash),
                  false.pure[F]
                )
              }
          }

      case VotingDuration.SummitLevel(_) =>
        // Don't know how to do this for higher levels of k, we'll have to figure it out,
        // run multiple instances of higher level finalizers or something.
        ???
    }

  /** Calculate the beginning and the end of this round,
    * based on the current tick and the round length. */
  private def roundBoundariesAt(tick: Ticks): F[(Ticks, Ticks)] =
    // TODO (round_parameter_storage): We should be able to tell about each past tick what the exponent at the time was.
    roundExponentRef.get map { exp =>
      // The era start may not be divisible by the round length.
      // We should be okay to use `Ticks.roundBoundaries` here if we follow the
      // rules of when we can update the round exponent, that is, when the higher
      // and lower speeds align themselves.
      Ticks.roundBoundaries(startTick, exp)(tick)
    }

  /** Only produce messages once we have finished the initial syncing when the node starts. */
  private def ifSynced(thunk: HWL[Unit]): HWL[Unit] =
    HighwayLog
      .liftF(isSynced)
      .ifM(thunk, noop)

  /** Execute some action unless we have already moved on from the round it was in. */
  private def ifCurrentRound(roundId: Ticks)(thunk: HWL[Unit]): HWL[Unit] =
    // It's okay not to send a response to a message where we *did* participate
    // in the round it belongs to, but we moved on to a newer round.
    HighwayLog.liftF(isCurrentRound(roundId)).ifM(thunk, noop)

  private def isCurrentRound(roundId: Ticks): F[Boolean] =
    currentTick.flatMap(roundBoundariesAt).map {
      case (from, _) => from == roundId
    }

  /** Build a block or ballot unless the fork choice is at the moment not something
    * that we can legally use, for example because it was reverted to some pre-midnight
    * block in the parent era. The fork choice will not pick something in a sibling era,
    * because it always starts from the key block of *this* era, and the sibling eras
    * have different key blocks, so it should be enough to check that the round ID of
    * the fork choice is earlier than the start of this era.
    */
  private def ifCanBuildOn[T](choice: ForkChoice.Result)(build: => F[T]): F[Option[T]] =
    // Doesn't apply on Genesis.
    if (!choice.block.parentBlock.isEmpty && choice.block.roundId < startTick) none.pure[F]
    else build.map(_.some)

  private def createLambdaResponse(
      messageProducer: MessageProducer[F],
      lambdaMessage: Message
  ): HWL[Unit] = ifSynced {
    for {
      m <- HighwayLog.liftF {
            messageProducer
              .withPermit {
                for {
                  // We need to cite the lambda message, and our own latest message.
                  // NOTE: The latter will be empty until the validator produces something in this era.
                  tips              <- dag.latestInEra(era.keyBlockHash)
                  ownLatestMessages <- tips.latestMessageHash(messageProducer.validatorId)
                  justifications = List(
                    PublicKey(lambdaMessage.validatorId) -> Set(lambdaMessage.messageHash),
                    messageProducer.validatorId          -> ownLatestMessages
                  ).filterNot(_._2.isEmpty).toMap
                  // See what the fork choice is, given the lambda and our own latest message, that is our target.
                  // In the voting period the lambda message is a ballot, so can't be a target,
                  // but in any case our own latest message might point at something with more weight already.
                  choice <- ForkChoice[F]
                             .fromJustifications(
                               lambdaMessage.eraId,
                               justifications.values.flatten.toSet
                             )
                             .timerGauge("response_forkchoice")
                  maybeMessage <- ifCanBuildOn(choice) {
                                   messageProducer
                                     .ballot(
                                       keyBlockHash = era.keyBlockHash,
                                       roundId = Ticks(lambdaMessage.roundId),
                                       target = choice.block.messageHash,
                                       justifications = choice.justificationsMap
                                     )
                                     .timerGauge("response_ballot")
                                 }
                } yield maybeMessage
              }
              .timerGauge("create_response")
          }
      _ <- m.fold(noop) { msg =>
            HighwayLog.tell[F](HighwayEvent.CreatedLambdaResponse(msg)) >>
              recordJustificationsDistance(msg)
          }
    } yield ()
  }

  private def createLambdaMessage(
      messageProducer: MessageProducer[F],
      roundId: Ticks
  ): HWL[Unit] = ifSynced {
    for {
      m <- HighwayLog.liftF {
            messageProducer
              .withPermit {
                ForkChoice[F]
                  .fromKeyBlock(era.keyBlockHash)
                  .timerGauge("lambda_forkchoice")
                  .flatMap { choice =>
                    ifCanBuildOn(choice) {
                      // In the active period we create a block, but during voting
                      // it can only be a ballot, after the switch block has been created.
                      val block = messageProducer
                        .block(
                          keyBlockHash = era.keyBlockHash,
                          roundId = roundId,
                          mainParent = choice.block.messageHash,
                          justifications = choice.justificationsMap,
                          isBookingBlock = isBookingBoundary(
                            choice.block.roundInstant,
                            conf.toInstant(roundId)
                          )
                        )
                        .timerGauge("lambda_block")
                        .widen[Message]

                      val ballot = messageProducer
                        .ballot(
                          keyBlockHash = era.keyBlockHash,
                          roundId = roundId,
                          target = choice.block.messageHash,
                          justifications = choice.justificationsMap
                        )
                        .timerGauge("lambda_ballot")
                        .widen[Message]

                      if (roundId < endTick) {
                        block
                      } else {
                        choice.block.isSwitchBlock.ifM(ballot, block)
                      }
                    }
                  }
              }
              .timerGauge("create_lambda")
          }
      _ <- m.fold(noop) { msg =>
            HighwayLog.tell[F](HighwayEvent.CreatedLambdaMessage(msg)) >>
              recordJustificationsDistance(msg) >>
              handleCriticalMessages(msg)
          }
    } yield ()
  }

  private def createOmegaMessage(
      messageProducer: MessageProducer[F],
      roundId: Ticks
  ): HWL[Unit] = ifSynced {
    for {
      m <- HighwayLog.liftF {
            messageProducer
              .withPermit {
                ForkChoice[F]
                  .fromKeyBlock(era.keyBlockHash)
                  .timerGauge("omega_forkchoice")
                  .flatMap { choice =>
                    ifCanBuildOn(choice) {
                      messageProducer
                        .ballot(
                          keyBlockHash = era.keyBlockHash,
                          roundId = roundId,
                          target = choice.block.messageHash,
                          justifications = choice.justificationsMap
                        )
                        .timerGauge("omega_ballot")
                    }
                  }
              }
              .timerGauge("create_omega")
          }
      _ <- m.fold(noop)(msg => {
            recordJustificationsDistance(msg) >>
              HighwayLog.tell[F](HighwayEvent.CreatedOmegaMessage(msg))
          })
    } yield ()
  }

  // Returns a map that represents number of msg justifications at their distance from the message.
  // Distance is a difference of rounds between the message and cited message.
  private def justificationsRoundsDistance(msg: Message): F[Map[Long, Int]] =
    msg.justifications.toList
      .traverse(j => dag.lookupUnsafe(j.latestBlockHash))
      .map { justificationMessages =>
        justificationMessages
          .filter(_.eraId == msg.eraId)
          .map(j => msg.jRank - j.jRank)
          .groupBy(identity)
          .mapValues(_.size)
      }

  private def recordJustificationsDistance(msg: Message): HWL[Unit] =
    HighwayLog.liftF(justificationsRoundsDistance(msg).flatMap { distances =>
      distances.toList.traverse {
        case (rankDistance, count) =>
          Metrics[F].record("justificationsJRankDistances", rankDistance, count.toLong)
      }.void
    })

  /** Trace the lineage of the switch block back to find a key block,
    * then the corresponding booking block.
    */
  private def createEra(
      switchBlock: Message.Block
  ): HWL[Unit] = {
    val keyBlockBoundary     = end minus conf.keyDuration
    val bookingBlockBoundary = end minus conf.bookingDuration

    // Trace back to the corresponding key block.
    val keyBlockF = findBlockCrossingBoundary(switchBlock, isCrossing(keyBlockBoundary))

    val maybeChildEra = keyBlockF flatMap { keyBlock =>
      // Checking if the era exist; if it does, we don't need to look up the booking block.
      EraStorage[F]
        .containsEra(keyBlock.messageHash)
        .ifM(
          none[Era].pure[F],
          for {
            bookingBlock <- findBlockCrossingBoundary(
                             keyBlock,
                             isCrossing(bookingBlockBoundary)
                           )
            magicBits <- collectMagicBits(dag, bookingBlock, keyBlock)
            seed      = LeaderSequencer.seed(bookingBlock.messageHash.toByteArray, magicBits)
            childEra = Era(
              parentKeyBlockHash = era.keyBlockHash,
              keyBlockHash = keyBlock.messageHash,
              bookingBlockHash = bookingBlock.messageHash,
              startTick = conf.toTicks(end),
              endTick = conf.toTicks(conf.eraEnd(end)),
              bonds = bookingBlock.blockSummary.getHeader.getState.bonds,
              leaderSeed = ByteString.copyFrom(seed)
            )
            isNew <- EraStorage[F].addEra(childEra)
          } yield childEra.some.filter(_ => isNew)
        )
    }

    HighwayLog.liftF(maybeChildEra) flatMap {
      case Some(era) => HighwayLog.tell[F](HighwayEvent.CreatedEra(era))
      case None      => noop
    }
  }

  /** Find a block in the ancestry where the parent is before a time,
    * but the block is after the time. */
  private def findBlockCrossingBoundary(
      descendant: Message,
      isBoundary: (Instant, Instant) => Boolean
  ): F[Message] = {
    def loop(child: Message, childTime: Instant): F[Message] =
      // If we reached Genesis, for any reason, use it, there's nothing else further back.
      if (child.parentBlock.isEmpty) child.pure[F]
      else {
        dag.lookupUnsafe(child.parentBlock).flatMap { parent =>
          val parentTime = parent.roundInstant
          if (isBoundary(parentTime, childTime))
            child.pure[F]
          else
            loop(parent, parentTime)
        }
      }
    loop(descendant, descendant.roundInstant)
  }

  /** Pick a time during the round to send the omega message. */
  private def chooseOmegaTick(roundStart: Ticks, roundEnd: Ticks): Ticks = {
    val r = rng.nextDouble()
    val o = conf.omegaMessageTimeStart + r * (conf.omegaMessageTimeEnd - conf.omegaMessageTimeStart)
    val t = roundStart + o * (roundEnd - roundStart)
    Ticks(t.toLong)
  }

  /** Preliminary check before the block is executed. Invalid blocks can be dropped. */
  def validate(message: Message): EitherT[F, String, Unit] = {
    val ok = EitherT.rightT[F, String](())

    def checkF(errorMessage: String, errorCondition: F[Boolean]) = EitherT {
      errorCondition.ifM(errorMessage.asLeft[Unit].pure[F], ok.value)
    }
    def check(errorMessage: String, errorCondition: Boolean) =
      checkF(errorMessage, errorCondition.pure[F])

    checkF(
      "The block is coming from a doppelganger.",
      isSynced.map(_ && maybeMessageProducer.map(_.validatorId).contains(message.validatorId))
    ) >>
      check(
        "The round ID is before the start of the era.",
        message.roundId < startTick
      ) >>
      check(
        "The round ID is after the end of the voting period.",
        conf.postEraVotingDuration match {
          case VotingDuration.FixedLength(d) => conf.toTicks(end plus d) < message.roundId
          case _                             => false
        }
      ) >>
      check(
        "The validator is not bonded in the era.",
        era.bonds.find(_.validatorPublicKey == message.validatorId).isEmpty
      ) >> {
      message match {
        case b: Message.Block =>
          check(
            "The block is not coming from the leader of the round.",
            leaderFunction(Ticks(message.roundId)) != message.validatorId
          ) >>
            checkF(
              "The leader has already sent a lambda message in this round.",
              // Not going to check this for ballots: two lambda-like ballots
              // can only be an equivocation, otherwise the 2nd one cites the first
              // and that means it's not lambda-like.
              hasOtherLambdaMessageInSameRound[F](dag, b, endTick)
            ) >>
            checkF(
              "Only ballots should be build on top of a switch block in the current era.",
              dag.lookupUnsafe(message.parentBlock).flatMap(_.isSwitchBlock)
            )

        case b: Message.Ballot if b.roundId >= endTick =>
          checkF(
            "A ballot during the voting-only period can only be built on pre-era-end blocks and switch blocks.",
            for {
              parent <- dag.lookupUnsafe(b.parentBlock)
              switch <- parent.isSwitchBlock
            } yield !(parent.roundId < endTick || switch)
          )

        case _ =>
          ok
      }
    }
  }

  /** Produce a starting agenda, depending on whether the validator is bonded or not. */
  def initAgenda: F[Agenda] =
    maybeMessageProducer.fold(Agenda.empty.pure[F]) { _ =>
      currentTick flatMap { tick =>
        isEraOverAt(tick).ifM(
          Agenda.empty.pure[F],
          roundBoundariesAt(tick) flatMap {
            case (from, to) =>
              val roundId = if (from >= tick) from else to
              Agenda(roundId -> StartRound(roundId)).pure[F]
          }
        )
      }
    }

  /** Handle a block or ballot coming from another validator.
    * The block is already validated and stored, here we just want to get the protocol reactions.
    * For example:
    * - if it's a lambda message, create a lambda response
    * - if it's a switch block, create a new era, unless it exists already.
    * Returns a list of events that happened during the persisting of changes.
    * This method is always called as a reaction to an incoming message,
    * so it doesn't return a future agenda of its own.
    */
  def handleMessage(message: ValidatedMessage): HWL[Unit] = {
    def check(ok: Boolean, error: String) =
      if (ok) noop
      else MonadThrowable[HWL].raiseError[Unit](new IllegalStateException(error) with NoStackTrace)

    for {
      _ <- check(
            message.eraId == era.keyBlockHash,
            "Shouldn't receive messages from other eras!"
          )
      _ <- maybeMessageProducer.fold(noop) { mp =>
            for {
              synced <- HighwayLog.liftF(isSynced)
              _ <- check(
                    !synced || message.validatorId != mp.validatorId,
                    "Shouldn't receive our own messages!"
                  )
              _ <- ifCurrentRound(Ticks(message.roundId)) {
                    HighwayLog
                      .liftF(message.isLambdaMessage)
                      .ifM(createLambdaResponse(mp, message), noop)
                  }
              _ <- HighwayLog
                    .liftF(isCurrentRound(Ticks(message.roundId)))
                    .ifM(
                      HighwayLog.liftF(Metrics[F].incrementCounter("incoming_same_round_message")),
                      HighwayLog
                        .liftF(Metrics[F].incrementCounter("incoming_different_round_message"))
                    )
                    .whenA(synced)
            } yield ()
          }
      _ <- handleCriticalMessages(message)
    } yield ()
  }

  /** Handle something that happens during a round:
    * - in rounds when we are leading, create a lambda message
    * - if it's a booking block, execute the auction
    * - if the main parent is a switch block, create a ballot instead
    * - sometime through the round, create an omega message
    * - if we're beyond the voting period after the end of the era, stop.
    *
    * Returns a list of events that took place, as well as its future agenda,
    * telling the caller (a supervisor) when it has to be scheduled again.
    */
  def handleAgenda(action: Agenda.Action): HWL[Agenda] =
    action match {
      case Agenda.StartRound(roundId) =>
        // Only create the agenda at the end, so if it takes too long to produce
        // a block we schedule the *next* round, possibly skipping a bunch.
        // Alternatively `StartRound` could just return a `CreateLambdaMessage`,
        // a `CreateOmegaMessage` and another `StartRound` to make them all
        // independently scheduleable.
        def agenda =
          for {
            now                           <- currentTick
            (currentRoundId, nextRoundId) <- roundBoundariesAt(now)
            // NOTE: These will potentially ask for the fork choice in this era;
            // it would be good if it was cached and updated only when new messages are added.
            isOverAtCurrent <- isEraOverAt(currentRoundId)
            isOverAtNext    <- isEraOverAt(nextRoundId)
            _ <- Log[F]
                  .info(s"No more rounds in ${era.keyBlockHash.show -> "era"}")
                  .whenA(isOverAtNext)
          } yield {
            val omega = if (!isOverAtCurrent) {
              // Schedule the omega for whatever the current round is, don't bother
              // with the old one if the block production was so slow that it pushed
              // us into the next round already. We can still participate in this one.
              val omegaTick = chooseOmegaTick(currentRoundId, nextRoundId)
              Agenda(omegaTick -> Agenda.CreateOmegaMessage(currentRoundId))
            } else Agenda.empty

            val next = if (!isOverAtNext) {
              Agenda(nextRoundId -> Agenda.StartRound(nextRoundId))
            } else Agenda.empty

            omega ++ next
          }

        maybeMessageProducer
          .filter(_.validatorId == leaderFunction(roundId))
          .fold(noop) {
            createLambdaMessage(_, roundId)
          } >> HighwayLog.liftF(agenda)

      case Agenda.CreateOmegaMessage(roundId) =>
        maybeMessageProducer.fold(noop) {
          createOmegaMessage(_, roundId)
        } >> HighwayLog.liftF(Agenda.empty.pure[F])
    }

  /** Do any kind of special logic when we encounter a "critical" block in the protocol:
    * - booking blocks should execute the auction, but that should be in their
    *   post state hash as well, so we pass the flag to the message producer
    * - key blocks don't need special handling when they are made
    * - switch blocks might be the ones that grant the rewards,
    *   according to how many blocks were finalized on time during the era
    */
  private def handleCriticalMessages(message: Message): HWL[Unit] =
    message match {
      case _: Message.Ballot =>
        noop
      case block: Message.Block =>
        HighwayLog.liftF(block.isSwitchBlock).ifM(createEra(block), noop)
    }

  /** Perform the validation and persistence of an incoming block.
    * Doesn't include reacting to it. It's here so we can encapsulate
    * the logic to make sure no two blocks from the same validator are
    * added concurrently, and that we validate the message according to the
    * rules of this era. */
  def validateAndAddBlock(messageExecutor: MessageExecutor[F], block: Block): F[ValidatedMessage] =
    for {
      message    <- MonadThrowable[F].fromTry(Message.fromBlock(block))
      maybeError <- validate(message).value
      _ <- maybeError.fold(
            error =>
              // TODO (CON-623): Some of these errors can be attributable. Those should be slashed
              // and not stop processing, so we'll need to return more detailed statuses from
              // validate to decide what to do, whether to react or not.
              MonadThrowable[F].raiseError[Unit](
                new ErrorMessageWrapper(
                  s"Could not validate block ${block.blockHash.show} against era ${era.keyBlockHash.show}: $error"
                )
              ),
            _ => ().pure[F]
          )
      semaphore      <- validatorSemaphoreMap.getOrAdd(PublicKey(block.getHeader.validatorPublicKey))
      isBookingBlock <- message.isBookingBlock
      _              <- messageExecutor.validateAndAdd(semaphore, block, isBookingBlock)
    } yield Validated(message)
}

object EraRuntime {

  def genesisEra(
      conf: HighwayConf,
      genesis: BlockSummary
  ): Era = Era(
    keyBlockHash = genesis.blockHash,
    bookingBlockHash = genesis.blockHash,
    startTick = conf.toTicks(conf.genesisEraStart),
    endTick = conf.toTicks(conf.genesisEraEnd),
    bonds = genesis.getHeader.getState.bonds
  )

  def fromGenesis[F[_]: Sync: MakeSemaphore: Clock: Metrics: Log: DagStorage: EraStorage: FinalityStorageReader: ForkChoice](
      conf: HighwayConf,
      genesis: BlockSummary,
      maybeMessageProducer: Option[MessageProducer[F]],
      initRoundExponent: Int,
      isSynced: => F[Boolean],
      leaderSequencer: LeaderSequencer = LeaderSequencer
  ): F[EraRuntime[F]] = {
    val era = genesisEra(conf, genesis)
    fromEra[F](conf, era, maybeMessageProducer, initRoundExponent, isSynced, leaderSequencer)
  }

  def fromEra[F[_]: Sync: MakeSemaphore: Clock: Metrics: Log: DagStorage: EraStorage: FinalityStorageReader: ForkChoice](
      conf: HighwayConf,
      era: Era,
      maybeMessageProducer: Option[MessageProducer[F]],
      initRoundExponent: Int,
      isSynced: => F[Boolean],
      leaderSequencer: LeaderSequencer = LeaderSequencer
  ): F[EraRuntime[F]] =
    for {
      leaderFunction   <- leaderSequencer[F](era)
      roundExponentRef <- Ref.of[F, Int](initRoundExponent)
      dag              <- DagStorage[F].getRepresentation
      semaphoreMap     <- SemaphoreMap[F, PublicKeyBS](1)
    } yield {
      new EraRuntime[F](
        conf,
        era,
        leaderFunction,
        roundExponentRef,
        // Whether the validator is bonded depends on the booking block. Only bonded validators
        // have to produce blocks and ballots in the era.
        maybeMessageProducer.filter { mp =>
          era.bonds.exists(b => b.validatorPublicKey == mp.validatorId)
        },
        semaphoreMap,
        isSynced,
        dag
      )
    }

  /** List of future actions to take. */
  type Agenda = Vector[Agenda.DelayedAction]

  object Agenda {
    sealed trait Action

    /** What action to take and when. */
    case class DelayedAction(tick: Ticks, action: Action)

    /** Handle one round:
      * - in rounds when we are leading, create a lambda message
      * - if it's a booking block, execute the auction
      * - if the main parent is a switch block, create a ballot instead
      * - if we're beyond the voting period after the end of the era, stop.
      */
    case class StartRound(roundId: Ticks) extends Action

    /** Create an Omega message, some time during the round */
    case class CreateOmegaMessage(roundId: Ticks) extends Action

    def apply(
        actions: (Ticks, Action)*
    ): Agenda =
      actions.map(DelayedAction.tupled).toVector

    val empty = apply()
  }

  /** Check that a parent timestamp is before, while the child is at or after a given boundary. */
  def isCrossing(boundary: Instant)(parent: Instant, child: Instant): Boolean =
    parent.isBefore(boundary) && !child.isBefore(boundary)

  /** Collect the magic bits between the booking block and the key block, */
  def collectMagicBits[F[_]: MonadThrowable](
      dag: DagRepresentation[F],
      bookingBlock: Message,
      keyBlock: Message
  ): F[Seq[Boolean]] =
    DagOperations
      .bfTraverseF(List(keyBlock)) { msg =>
        List(msg.parentBlock).filterNot(_.isEmpty).traverse(dag.lookupUnsafe)
      }
      .takeUntil(_.messageHash == bookingBlock.messageHash)
      .map(_.blockSummary.getHeader.magicBit)
      .toList
      .map(_.reverse)

  /** Check that two messages are in the same round. Different eras (e.g. the parent and the child)
    * can share a round ID, but they are still different rounds.
    */
  def isSameRoundAs(a: Message)(b: Message): Boolean =
    a.eraId == b.eraId && a.roundId == b.roundId

  /** Check if a message from a validator cites their own message in the same round as the message itself.
    * If it does, that would mean that this message was not the first in that round. In the voting period,
    * that is enough to distinguish between two potential ballots from the leader to pick the lambda.
    *
    * A lambda block may cite an omega block from the same validator if they were made concurrently,
    * or an omega from a different validator, if that validator works a lot faster and pushed out a ballot
    * even faster than the lambda block's justifications were established, although this is unlikely.
    */
  def citesOwnMessageInSameRound[F[_]: MonadThrowable](
      dag: DagRepresentation[F],
      message: Message
  ): F[Boolean] =
    message.justifications
      .filter(_.validatorPublicKey == message.validatorId)
      .toList
      .findM[F] { j =>
        dag.lookupUnsafe(j.latestBlockHash).map(isSameRoundAs(message))
      }
      .map(_.isDefined)

  /** Given a ballot which we already established is coming from the leader of its round,
    * check whether it looks like a lambda message (rather than an omega): a lambda message
    * normally only cites messages from different rounds than its own, because it starts at
    * the beginning of the block, and it shouldn't have time to receive stray omega messages
    * from the same round even from validators running with very low round exponents.
    */
  def isLambdaLikeBallot[F[_]: MonadThrowable](
      dag: DagRepresentation[F],
      ballot: Message.Ballot,
      eraEndTick: Ticks
  ): F[Boolean] =
    if (ballot.roundId < eraEndTick) false.pure[F]
    else citesOwnMessageInSameRound(dag, ballot).map(!_)

  /** Given a lambda message from the leader of a round, check if the validator has sent
    * another lambda message already in the same round. Ignores equivocations, just the
    * checks the legal j-DAG.
    */
  def hasOtherLambdaMessageInSameRound[F[_]: MonadThrowable](
      dag: DagRepresentation[F],
      message: Message,
      eraEndTick: Ticks
  ): F[Boolean] =
    DagOperations
      .swimlaneV[F](
        message.validatorId,
        message,
        dag
      )
      // We know this one is a lambda message.
      .filter(_.messageHash != message.messageHash)
      // Only look at messages in this round, in this era.
      .takeWhile(isSameRoundAs(message))
      // Try to find a lambda message.
      .findF {
        case _: Message.Block =>
          // We established that this validator is the lead, so a block from them is a lambda.
          true.pure[F]
        case b: Message.Ballot =>
          // The era is over, so this is a voting only period message.
          // In the active period, it's easy to know that a ballot is either a lambda response or an omega.
          // In the voting only period, however, lambda messages are ballots too.
          // The only way to tell if a ballot might be a lambda message is that it cites nothing from this round.
          isLambdaLikeBallot(dag, b, eraEndTick)
        case _ =>
          false.pure[F]
      }
      .map(_.isDefined)
}

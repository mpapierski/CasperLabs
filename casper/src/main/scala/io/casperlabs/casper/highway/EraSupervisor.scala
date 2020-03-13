package io.casperlabs.casper.highway

import cats._
import cats.implicits._
import cats.syntax.show
import cats.effect.{Concurrent, Fiber, Resource, Sync, Timer}
import cats.effect.concurrent.{Ref, Semaphore}
import io.casperlabs.casper.consensus.{Block, BlockSummary, Era}
import io.casperlabs.casper.dag.DagOperations
import io.casperlabs.casper.dag.DagOperations.Key
import io.casperlabs.casper.highway.EraRuntime.Agenda
import io.casperlabs.comm.gossiping.Relaying
import io.casperlabs.metrics.Metrics
import io.casperlabs.metrics.implicits._
import io.casperlabs.models.Message
import io.casperlabs.shared.Log
import io.casperlabs.storage.BlockHash
import io.casperlabs.storage.block.BlockStorage
import io.casperlabs.storage.dag.{DagRepresentation, DagStorage, FinalityStorageReader}
import io.casperlabs.storage.era.EraStorage

import scala.concurrent.duration.FiniteDuration
import scala.util.control.NonFatal

/** The supervisor loads all the active eras when it starts and does the following things:
  * - acts as a gateway for the rest of the system to execute blocks by passing them to the right era
  * - manages the scheduling of the agendas of the eras by acting as a trampoline for them
  * - propagates messages received or created by parent eras to the descendants to keep the latest messsages up to date.
  */
class EraSupervisor[F[_]: Concurrent: Timer: Log: Metrics: EraStorage: Relaying: ForkChoiceManager](
    conf: HighwayConf,
    // Once the supervisor is shut down, reject incoming messages.
    isShutdownRef: Ref[F, Boolean],
    eraStartSemaphore: Semaphore[F],
    erasRef: Ref[F, Map[BlockHash, EraSupervisor.Entry[F]]],
    scheduleRef: Ref[F, Map[(BlockHash, Agenda.DelayedAction), Fiber[F, Unit]]],
    makeRuntime: Era => F[EraRuntime[F]],
    messageExecutor: MessageExecutor[F]
) {
  import EraSupervisor.Entry

  implicit val metricsSource = HighwayMetricsSource / "EraSupervisor"

  private def shutdown(): F[Unit] =
    for {
      _        <- isShutdownRef.set(true)
      schedule <- scheduleRef.get
      _        <- schedule.values.toList.traverse(_.cancel.attempt)
    } yield ()

  /** Handle a block coming from the DownloadManager. No need to broadcast the block itself,
    * it will be gossiped if it's successfully validated. Only need to broadcast responses. */
  def validateAndAddBlock(block: Block): F[Unit] =
    for {
      _      <- ensureNotShutdown
      header = block.getHeader
      hash   = block.blockHash.show
      _ <- Log[F].info(
            s"Handling incoming ${hash -> "message"} from ${header.validatorPublicKey.show -> "validator"} in ${header.roundId -> "round"} ${header.keyBlockHash.show -> "era"}"
          )
      entry <- load(header.keyBlockHash)

      message <- entry.runtime
                  .validateAndAddBlock(messageExecutor, block)
                  .timerGauge("incoming_validateAndAddBlock")

      _ <- messageExecutor
            .effectsAfterAdded(message)
            .timerGauge("incoming_effectsAfterAdded")

      // Tell descendant eras for the next time they create a block that this era received a message.
      // NOTE: If the events create an era it will only be loaded later, so the first time
      // the fork choice will be asked about that era it will not have been notified about
      // this message. The fork choice manager should load the latest messages from the
      // parent era the first time it's asked, and only rely on incremental updates later.
      _ <- propagateLatestMessageToDescendantEras(message)
            .timerGauge("incoming_propagateLatestMessage")

      // See what reactions the protocol dictates.
      (events, ()) <- entry.runtime
                       .handleMessage(message)
                       .run
                       .timerGauge("incoming_handleMessage")

      _ <- handleEvents(events)
            .timerGauge("incoming_handleEvents")

      _ <- Log[F].info(s"Finished handling ${hash -> "message"}")
    } yield ()

  private def ensureNotShutdown: F[Unit] =
    isShutdownRef.get.ifM(
      Sync[F]
        .raiseError(new IllegalStateException("EraSupervisor is already shut down.")),
      Sync[F].unit
    )

  def eras: F[Set[Entry[F]]] =
    erasRef.get.map(_.values.toSet)

  private def load(keyBlockHash: BlockHash): F[Entry[F]] =
    erasRef.get >>= {
      _.get(keyBlockHash).fold {
        eraStartSemaphore.withPermit {
          erasRef.get >>= {
            _.get(keyBlockHash).fold(start(keyBlockHash))(_.pure[F])
          }
        }
      }(_.pure[F])
    }

  /** Create a runtime and schedule its initial agenda. */
  private def start(keyBlockHash: BlockHash): F[Entry[F]] =
    for {
      // We should already have seen the switch block that created this block's era,
      // or it should be in the genesis era. If we can't find it, the Download Manager
      // would not have given the block to us, or the key block is invalid.
      era     <- EraStorage[F].getEraUnsafe(keyBlockHash)
      runtime <- makeRuntime(era)
      agenda  <- runtime.initAgenda
      entry   <- start(runtime, agenda)
    } yield entry

  private def start(runtime: EraRuntime[F], agenda: Agenda): F[Entry[F]] = {
    val key = runtime.era.keyBlockHash
    for {
      childEras <- EraStorage[F].getChildEras(key)
      entry     = Entry[F](runtime, childEras.map(_.keyBlockHash))
      _ <- erasRef.update { eras =>
            // Sanity check that the semaphore is applied.
            assert(!eras.contains(key), "Shouldn't start eras more than once!")
            eras.updated(key, entry)
          }
      _ <- schedule(runtime, agenda)
    } yield entry
  }

  private def schedule(runtime: EraRuntime[F], agenda: Agenda): F[Unit] =
    agenda.traverse {
      case delayed @ Agenda.DelayedAction(tick, action) =>
        val key = (runtime.era.keyBlockHash, delayed)
        for {
          fiber <- scheduleAt(tick) {
                    val era = runtime.era.keyBlockHash.show
                    val exec = for {
                      _ <- Log[F].debug(s"Executing $action in $era")
                      (events, agenda) <- runtime
                                           .handleAgenda(action)
                                           .run
                                           .timerGauge("schedule_handleAgenda")

                      isScheduleEmpty <- scheduleRef.modify { sch =>
                                          val rem = sch - key
                                          rem -> rem.isEmpty
                                        }
                      _ <- Log[F]
                            .warn(s"There are no more actions scheduled for any of the active eras")
                            .whenA(isScheduleEmpty && agenda.isEmpty)

                      _ <- schedule(runtime, agenda)
                      _ <- handleEvents(events).timerGauge("schedule_handleEvents")
                    } yield ()

                    exec.recoverWith {
                      case NonFatal(ex) =>
                        Metrics[F].incrementCounter("schedule_errors") *>
                          Log[F].error(s"Error executing $action in $era: $ex")
                    }
                  }
          _ <- scheduleRef.update { s =>
                // Sanity check that we're not leaking fibers.
                assert(!s.contains(key), "Shouldn't schedule the same thing twice!")
                s.updated(key, fiber)
              }
        } yield ()
    } void

  /** Execute an effect later, asynchronously. */
  private def scheduleAt[A](ticks: Ticks)(effect: F[A]): F[Fiber[F, A]] =
    for {
      now   <- Timer[F].clock.realTime(conf.tickUnit)
      delay = math.max(ticks - now, 0L)
      delayedF = Timer[F].sleep(FiniteDuration(delay, conf.tickUnit)) >> effect.onError {
        case NonFatal(ex) => Log[F].error(s"Error executing fiber: $ex")
      }
      fiber <- Concurrent[F].start {
                Metrics[F].gauge("scheduled_items")(delayedF)
              }
    } yield fiber

  /** Handle the domain events coming out of the execution of the protocol. */
  private def handleEvents(events: Vector[HighwayEvent]): F[Unit] = {
    def handleCreatedMessage(message: Message, kind: String) =
      for {
        _ <- Log[F].info(
              s"Created $kind ${message.messageHash.show -> "message"} in ${message.roundId -> "round"} ${message.eraId.show -> "era"} child of ${message.parentBlock.show -> "parent"}"
            )
        _ <- messageExecutor
              .effectsAfterAdded(Validated(message))
              .timerGauge(s"created_${kind}_effectsAfterAdded")
        _ <- Relaying[F]
              .relay(List(message.messageHash))
              .timerGauge(s"created_${kind}_relay")
        _ <- propagateLatestMessageToDescendantEras(message)
              .timerGauge(s"created_${kind}_propagateLatestMessage")
        _ <- Metrics[F].incrementCounter(s"created_$kind")
      } yield ()

    events.traverse {
      case HighwayEvent.CreatedEra(era) =>
        val eraHash    = era.keyBlockHash.show
        val parentHash = era.parentKeyBlockHash.show
        val tick       = era.startTick
        val start      = conf.toInstant(Ticks(tick))
        for {
          _ <- Log[F].info(
                s"Created ${eraHash -> "era"} starting at ${tick -> "tick"} ${start -> "instant"} child of ${parentHash -> "parent"}"
              )
          // Schedule the child era.
          child <- load(era.keyBlockHash)
          // Remember the offspring.
          _ <- addToParent(child)
        } yield ()

      case HighwayEvent.CreatedLambdaMessage(m)  => handleCreatedMessage(m, "lambda-message")
      case HighwayEvent.CreatedLambdaResponse(m) => handleCreatedMessage(m, "lambda-response")
      case HighwayEvent.CreatedOmegaMessage(m)   => handleCreatedMessage(m, "omega-message")
    } void
  }

  /** Update this and descendant eras' latest messages. */
  private def propagateLatestMessageToDescendantEras(message: Message): F[Unit] =
    for {
      // Let this era know as well. You'd expect that the message production will
      // keep this updated, when the fork choice is implemented, but for now in
      // tests if produce a block in a leaf era this makes sure that the next round
      // will build on it, and it seems to make sense anyway.
      _  <- ForkChoiceManager[F].updateLatestMessage(message.eraId, message)
      ds <- loadDescendants(message.eraId)
      _  <- ds.traverse(d => ForkChoiceManager[F].updateLatestMessage(d.keyBlockHash, message))
    } yield ()

  private def loadDescendants(keyBlockHash: BlockHash): F[List[Entry[F]]] = {
    def loadChildren(keyBlockHash: BlockHash): F[List[Entry[F]]] =
      for {
        childEras <- erasRef.get.map(_(keyBlockHash).children.toList)
        // Make sure the child tree is loaded into memory, so we traverse
        // even the ones that were inactive when the supervisor started,
        // which could happen for example if the validator is bonded in the
        // grandparent and the child era but in the parent, and suddenly
        // there's a stray message in the grandparent: if we don't have
        // the parent loaded, the child won't be notified.
        childEntries <- childEras.traverse(load)
      } yield childEntries
    for {
      children <- loadChildren(keyBlockHash)
      descendants <- DagOperations
                      .bfTraverseF[F, Entry[F]](children) { entry =>
                        loadChildren(entry.keyBlockHash)
                      }
                      .toList
    } yield descendants
  }

  /** Add a parent-child era association to the tree of eras, so that we can update
    * the descendant eras when new messages come in to the ancestors, which is required
    * for things like rewarding voting-only ballots that are created in the parent,
    * with the rewards being added to some blocks in the child era.
    */
  private def addToParent(child: Entry[F]): F[Unit] =
    erasRef.update { eras =>
      val parentKeyBlockHash = child.runtime.era.parentKeyBlockHash
      eras.updated(parentKeyBlockHash, eras(parentKeyBlockHash).withChild(child))
    }
}

object EraSupervisor {

  def apply[F[_]: Concurrent: Timer: Log: Metrics: EraStorage: DagStorage: FinalityStorageReader: Relaying: ForkChoiceManager](
      conf: HighwayConf,
      genesis: BlockSummary,
      maybeMessageProducer: Option[MessageProducer[F]],
      messageExecutor: MessageExecutor[F],
      initRoundExponent: Int,
      isSynced: F[Boolean]
  ): Resource[F, EraSupervisor[F]] =
    Resource.make {
      for {
        isShutdownRef <- Ref.of(false)
        erasSemaphore <- Semaphore[F](1)
        erasRef       <- Ref.of(Map.empty[BlockHash, Entry[F]])
        scheduleRef   <- Ref.of(Map.empty[(BlockHash, Agenda.DelayedAction), Fiber[F, Unit]])

        makeRuntime = (era: Era) =>
          EraRuntime.fromEra[F](
            conf,
            era,
            maybeMessageProducer,
            initRoundExponent,
            isSynced
          )

        supervisor = new EraSupervisor[F](
          conf,
          isShutdownRef,
          erasSemaphore,
          erasRef,
          scheduleRef,
          makeRuntime,
          messageExecutor
        )

        // Make sure we have the genesis era in storage. We'll resume if it's the first one.
        genesisEra = EraRuntime.genesisEra(conf, genesis)
        _          <- EraStorage[F].addEra(genesisEra)

        // Resume currently active eras.
        activeEras <- collectActiveEras[F](makeRuntime)
        _ <- activeEras.traverse {
              case (runtime, agenda) => supervisor.start(runtime, agenda)
            }
        _ <- Log[F].warn("There are no active eras!").whenA(activeEras.isEmpty)
      } yield supervisor
    }(_.shutdown())

  /** The supervisor keeps track of the eras it initiated, and maintains the set of
    * child eras that era originally had, plus what it obtained along the way.
    */
  case class Entry[F[_]](
      runtime: EraRuntime[F],
      children: Set[BlockHash]
  ) {
    def keyBlockHash =
      runtime.era.keyBlockHash
    def withChild(child: Entry[F]) =
      copy(children = children + child.keyBlockHash)
  }
  object Entry {
    implicit def `Key[Entry]`[F[_]]: Key[Entry[F]] =
      Key.instance[Entry[F], BlockHash](_.keyBlockHash)
  }

  // Return type for the initialization of active eras along the era tree.
  type EraWithAgenda[F[_]] = (EraRuntime[F], EraRuntime.Agenda)

  // Helper for traversing the era tree.
  implicit def `Key[EraWithAgenda]`[F[_]] = Key.instance[EraWithAgenda[F], BlockHash] {
    case (runtime, _) => runtime.era.keyBlockHash
  }

  /** Walk up the era tree, starting from the tips, and collect any era that's
    * not finished yet. We know they are not finished because they have something
    * they want to do. If there's nothing, we'll have to rely on incoming messages
    * to establish the missing links for us.
    */
  def collectActiveEras[F[_]: Sync: EraStorage](
      makeRuntime: Era => F[EraRuntime[F]]
  ): F[List[EraWithAgenda[F]]] = {
    // Eras without agendas are finished.
    def selectActives(eras: List[EraWithAgenda[F]]) =
      eras.filterNot(_._2.isEmpty)

    for {
      tips <- EraStorage[F].getChildlessEras

      activeTips <- tips.toList.traverse { era =>
                     for {
                       runtime <- makeRuntime(era)
                       agenda  <- runtime.initAgenda
                     } yield runtime -> agenda
                   } map { selectActives }

      active <- DagOperations
                 .bfTraverseF[F, EraWithAgenda[F]](activeTips) {
                   case (runtime, _) if !runtime.era.parentKeyBlockHash.isEmpty =>
                     for {
                       parentEra     <- EraStorage[F].getEraUnsafe(runtime.era.parentKeyBlockHash)
                       parentRuntime <- makeRuntime(parentEra)
                       parentAgenda  <- parentRuntime.initAgenda
                     } yield selectActives(List(parentRuntime -> parentAgenda))

                   case _ =>
                     List.empty.pure[F]
                 }
                 .toList
    } yield active
  }
}

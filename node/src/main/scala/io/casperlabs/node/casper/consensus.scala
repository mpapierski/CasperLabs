package io.casperlabs.node.casper.consensus

import com.google.protobuf.ByteString
import cats.implicits._
import cats.effect._
import cats.effect.concurrent._
import cats.mtl.FunctorRaise
import io.casperlabs.casper.consensus._
import io.casperlabs.casper.{
  BlockStatus,
  CasperState,
  EventEmitter,
  MultiParentCasper,
  MultiParentCasperImpl,
  MultiParentCasperRef,
  PrettyPrinter,
  ValidatorIdentity
}
import io.casperlabs.casper.{EquivocatedBlock, InvalidBlock, Processed, SelfEquivocatedBlock, Valid}
import io.casperlabs.casper.DeploySelection.DeploySelection
import io.casperlabs.casper.MultiParentCasperRef.MultiParentCasperRef
import io.casperlabs.casper.finality.MultiParentFinalizer
import io.casperlabs.casper.finality.votingmatrix.FinalityDetectorVotingMatrix
import io.casperlabs.casper.validation.{
  raiseValidateErrorThroughApplicativeError,
  HighwayValidationImpl,
  NCBValidationImpl,
  Validation,
  ValidationImpl
}
import io.casperlabs.casper.util.{CasperLabsProtocol, ProtoUtil}
import io.casperlabs.casper.highway.{
  EraSupervisor,
  ForkChoiceManager,
  HighwayConf,
  MessageExecutor,
  MessageProducer
}
import io.casperlabs.catscontrib.MonadThrowable
import io.casperlabs.comm.ServiceError.{NotFound, Unavailable}
import io.casperlabs.comm.gossiping.Relaying
import io.casperlabs.crypto.Keys.PublicKey
import io.casperlabs.ipc.ChainSpec
import io.casperlabs.models.Message
import io.casperlabs.metrics.Metrics
import io.casperlabs.mempool.DeployBuffer
import io.casperlabs.node.api.EventStream
import io.casperlabs.node.configuration.Configuration
import io.casperlabs.shared.{Cell, FatalError, Log, Time}
import io.casperlabs.storage.BlockHash
import io.casperlabs.storage.deploy.DeployStorage
import io.casperlabs.storage.block.BlockStorage
import io.casperlabs.storage.dag.DagStorage
import io.casperlabs.storage.dag.FinalityStorage
import io.casperlabs.storage.era.EraStorage
import io.casperlabs.smartcontracts.ExecutionEngineService
import java.util.concurrent.TimeUnit
import java.time.Instant
import io.casperlabs.shared.Sorting.jRankOrder

import scala.util.control.NoStackTrace
import scala.concurrent.duration._
import simulacrum.typeclass
import io.casperlabs.storage.dag.AncestorsStorage

// Stuff we need to pass to gossiping.
@typeclass
trait Consensus[F[_]] {

  def validateSummary(
      summary: BlockSummary
  ): F[Unit]

  /** Validate and persist a block. Raise an error if there's something wrong. Don't gossip. */
  def validateAndAddBlock(
      block: Block
  ): F[Unit]

  /** Let consensus know that the Genesis block has been approved and stored,
    * so it can transition to block processing now.
    */
  def onGenesisApproved(genesisBlockHash: ByteString): F[Unit]

  /** Let consensus know that a new block has been scheduled for downloading.
    */
  def onScheduled(summary: BlockSummary): F[Unit]

  /** Provide a rank to start the initial synchronization from. */
  def lastSynchronizedRank: F[Long]

  /** Latest messages for pull based gossiping. */
  def latestMessages: F[Set[Block.Justification]]
}

object NCB {
  def apply[F[_]: Concurrent: Time: Log: BlockStorage: DagStorage: ExecutionEngineService: MultiParentCasperRef: Metrics: DeployStorage: DeployBuffer: DeploySelection: FinalityStorage: CasperLabsProtocol: EventStream](
      conf: Configuration,
      chainSpec: ChainSpec,
      maybeValidatorId: Option[ValidatorIdentity]
  ): Consensus[F] = {
    val chainName = chainSpec.getGenesis.name

    implicit val raise: FunctorRaise[F, InvalidBlock] =
      raiseValidateErrorThroughApplicativeError[F]

    implicit val validationEff: Validation[F] = ValidationImpl.metered[F](new NCBValidationImpl[F])

    new Consensus[F] {

      override def validateSummary(summary: BlockSummary): F[Unit] =
        Validation[F].blockSummary(summary, chainName)

      override def validateAndAddBlock(
          block: Block
      ): F[Unit] =
        MultiParentCasperRef[F].get
          .flatMap {
            case Some(casper) =>
              casper.addBlock(block)

            case None if block.getHeader.parentHashes.isEmpty =>
              for {
                _     <- Log[F].info(s"Validating genesis-like ${show(block.blockHash) -> "block"}")
                state <- Cell.mvarCell[F, CasperState](CasperState())
                executor <- MultiParentCasperImpl.StatelessExecutor
                             .create[F](
                               maybeValidatorId.map(_.publicKey),
                               chainName = chainSpec.getGenesis.name,
                               chainSpec.upgrades
                             )
                status <- executor.validateAndAddBlock(None, block)(state)
              } yield status

            case None =>
              MonadThrowable[F].raiseError[BlockStatus](Unavailable("Casper is not yet available."))
          }
          .flatMap {
            case Valid =>
              Log[F].debug(s"Validated and stored ${show(block.blockHash) -> "block"}")

            case EquivocatedBlock =>
              Log[F].debug(
                s"Detected ${show(block.blockHash) -> "block"} equivocated"
              )

            case Processed =>
              Log[F].warn(
                s"${show(block.blockHash) -> "block"} seems to have been processed before."
              )

            case SelfEquivocatedBlock =>
              FatalError.selfEquivocationError(block.blockHash)

            case other =>
              Log[F].debug(s"Received invalid ${show(block.blockHash) -> "block"}: $other") *>
                MonadThrowable[F].raiseError[Unit](
                  // Raise an exception to stop the DownloadManager from progressing with this block.
                  new RuntimeException(s"Non-valid status: $other") with NoStackTrace
                )
          }

      override def onGenesisApproved(genesisBlockHash: ByteString): F[Unit] =
        for {
          maybeGenesis <- BlockStorage[F].get(genesisBlockHash)
          genesisStore <- MonadThrowable[F].fromOption(
                           maybeGenesis,
                           NotFound(
                             s"Cannot retrieve ${show(genesisBlockHash) -> "genesis"}"
                           )
                         )
          genesis    = genesisStore.getBlockMessage
          prestate   = ProtoUtil.preStateHash(genesis)
          transforms = genesisStore.blockEffects.flatMap(_.effects)
          casper <- MultiParentCasper.fromGossipServices(
                     maybeValidatorId,
                     genesis,
                     prestate,
                     transforms,
                     genesis.getHeader.chainName,
                     conf.casper.minTtl,
                     chainSpec.upgrades,
                     rFTT = chainSpec.getGenesis.getHighwayConfig.ftt
                   )
          _ <- MultiParentCasperRef[F].set(casper)
          _ <- Log[F].info(s"Making the transition to block processing.")
        } yield ()

      override def onScheduled(summary: BlockSummary): F[Unit] =
        // The EquivocationDetector treats equivocations with children differently,
        // so let Casper know about the DAG dependencies up front.
        MultiParentCasperRef[F].get.flatMap {
          case Some(casper: MultiParentCasperImpl[F]) =>
            val partialBlock = Block()
              .withBlockHash(summary.blockHash)
              .withHeader(summary.getHeader)

            Log[F].debug(
              s"Feeding a pending block to Casper: ${show(summary.blockHash) -> "block"}"
            ) *>
              casper.addMissingDependencies(partialBlock)

          case _ => ().pure[F]
        }

      /** Start from the rank of the oldest message of validators bonded at the latest finalized block. */
      override def lastSynchronizedRank: F[Long] =
        for {
          dag            <- DagStorage[F].getRepresentation
          latestMessages <- dag.latestMessages
          lfb            <- FinalityStorage[F].getLastFinalizedBlock
          lfbMessage     <- dag.lookupUnsafe(lfb)
          // Take only validators who were bonded in the LFB to make sure unbonded
          // validators don't cause syncing to always start from the rank they left at.
          bonded = lfbMessage.blockSummary.getHeader.getState.bonds.map(_.validatorPublicKey).toSet
          // The minimum rank of all latest messages is not exactly
          minRank = latestMessages
            .filterKeys(bonded)
            .values
            .flatMap(_.map(_.jRank))
            .toList
            .minimumOption
            .getOrElse(0L)
        } yield minRank

      override def latestMessages: F[Set[Block.Justification]] =
        for {
          dag <- DagStorage[F].getRepresentation
          lm  <- dag.latestMessages
        } yield lm.values.flatten
          .map(m => Block.Justification(m.validatorId, m.messageHash))
          .toSet

      private def show(hash: ByteString) =
        PrettyPrinter.buildString(hash)
    }
  }
}

object Highway {
  def apply[F[_]: Concurrent: Time: Timer: Clock: Log: Metrics: DagStorage: BlockStorage: DeployBuffer: DeployStorage: EraStorage: FinalityStorage: AncestorsStorage: CasperLabsProtocol: ExecutionEngineService: DeploySelection: EventEmitter: Relaying](
      conf: Configuration,
      chainSpec: ChainSpec,
      maybeValidatorId: Option[ValidatorIdentity],
      genesis: Block,
      isSyncedRef: Ref[F, Boolean]
  ): Resource[F, Consensus[F]] = {
    val chainName               = chainSpec.getGenesis.name
    val hc                      = chainSpec.getGenesis.getHighwayConfig
    val faultToleranceThreshold = chainSpec.getGenesis.getHighwayConfig.ftt

    for {
      implicit0(finalizer: MultiParentFinalizer[F]) <- {
        Resource.liftF[F, MultiParentFinalizer[F]] {
          for {
            lfb <- FinalityStorage[F].getLastFinalizedBlock
            dag <- DagStorage[F].getRepresentation
            finalityDetector <- FinalityDetectorVotingMatrix
                                 .of[F](
                                   dag,
                                   lfb,
                                   faultToleranceThreshold,
                                   isHighway = true
                                 )
            finalizer <- MultiParentFinalizer.create[F](
                          dag,
                          lfb,
                          finalityDetector
                        )
          } yield finalizer
        }
      }

      implicit0(forkchoice: ForkChoiceManager[F]) <- Resource.pure[F, ForkChoiceManager[F]] {
                                                      ForkChoiceManager.create[F]
                                                    }

      implicit0(raise: FunctorRaise[F, InvalidBlock]) = raiseValidateErrorThroughApplicativeError[F]

      implicit0(validationEff: Validation[F]) = ValidationImpl.metered[F](
        new HighwayValidationImpl[F]
      )

      hwConf = HighwayConf(
        tickUnit = TimeUnit.MILLISECONDS,
        genesisEraStart = Instant.ofEpochMilli(hc.genesisEraStartTimestamp),
        eraDuration = HighwayConf.EraDuration.FixedLength(hc.eraDurationMillis.millis),
        bookingDuration = hc.bookingDurationMillis.millis,
        entropyDuration = hc.entropyDurationMillis.millis,
        postEraVotingDuration = if (hc.votingPeriodSummitLevel > 0) {
          HighwayConf.VotingDuration.SummitLevel(hc.votingPeriodSummitLevel)
        } else {
          HighwayConf.VotingDuration
            .FixedLength(hc.votingPeriodDurationMillis.millis)
        },
        omegaMessageTimeStart = conf.highway.omegaMessageTimeStart.value,
        omegaMessageTimeEnd = conf.highway.omegaMessageTimeEnd.value
      )

      _ <- Resource.liftF {
            Log[F].info(
              s"Genesis era lasts from ${hwConf.genesisEraStart} to ${hwConf.genesisEraEnd}"
            ) >>
              Log[F].info(s"Era duration is ${hwConf.eraDuration}") >>
              Log[F].info(s"Booking duration is ${hwConf.bookingDuration}") >>
              Log[F].info(s"Entropy duration is ${hwConf.entropyDuration}") >>
              Log[F].info(s"Voting duration is ${hwConf.postEraVotingDuration}") >>
              Log[F].info(
                s"Initial round exponent is ${conf.highway.initRoundExponent.value -> "exponent"}"
              )
          }

      supervisor <- EraSupervisor(
                     conf = hwConf,
                     genesis = BlockSummary(genesis.blockHash, genesis.header, genesis.signature),
                     maybeMessageProducer = maybeValidatorId.map { validatorId =>
                       MessageProducer[F](
                         validatorId,
                         chainName = chainSpec.getGenesis.name,
                         upgrades = chainSpec.upgrades
                       )
                     },
                     messageExecutor = new MessageExecutor(
                       chainName = chainSpec.getGenesis.name,
                       genesis = genesis,
                       upgrades = chainSpec.upgrades,
                       maybeValidatorId =
                         maybeValidatorId.map(v => PublicKey(ByteString.copyFrom(v.publicKey)))
                     ),
                     initRoundExponent = conf.highway.initRoundExponent.value,
                     isSynced = isSyncedRef.get
                   )

      consensusEff = new Consensus[F] {
        override def validateSummary(summary: BlockSummary): F[Unit] =
          Validation[F].blockSummary(summary, chainName)

        override def validateAndAddBlock(block: Block): F[Unit] =
          supervisor.validateAndAddBlock(block).whenA(block != genesis)

        override def onGenesisApproved(genesisBlockHash: ByteString): F[Unit] =
          // This is for the integration tests, they are looking for this.
          Log[F].info(s"Making the transition to block processing.")

        override def onScheduled(summary: BlockSummary): F[Unit] =
          ().pure[F]

        /** Start from the key block of the latest era. Eventually we should use the base block. */
        override def lastSynchronizedRank: F[Long] =
          for {
            dag  <- DagStorage[F].getRepresentation
            eras <- EraStorage[F].getChildlessEras
            // Key block hashes are going to be in the grandparent era.
            keyBlocksHashes = eras.map(_.keyBlockHash).toList
            keyBlocks       <- keyBlocksHashes.traverse(dag.lookupUnsafe)
            // Take the latest, to allow eras to be killed without causing
            // initial sync to start from their keyblock forever.
            maxRank = keyBlocks.map(_.jRank).max
          } yield maxRank

        /** Serve the latest messages from childless eras and their parents,
          * to keep the data used in pull based gossiping to a reasonable limit.
          */
        override def latestMessages: F[Set[Block.Justification]] =
          for {
            eras <- supervisor.eras
            childless = eras.collect {
              case entry if entry.children.isEmpty =>
                entry.runtime.era
            }
            active = childless.flatMap { era =>
              Set(era.parentKeyBlockHash, era.keyBlockHash)
            }
            dag <- DagStorage[F].getRepresentation
            lms <- active.toList.traverse { keyBlockHash =>
                    dag.latestInEra(keyBlockHash).flatMap(_.latestMessages)
                  }
          } yield {
            lms.flatMap { lm =>
              lm.values.flatten
                .map(m => Block.Justification(m.validatorId, m.messageHash))
            }.toSet
          }
      }
    } yield consensusEff
  }
}

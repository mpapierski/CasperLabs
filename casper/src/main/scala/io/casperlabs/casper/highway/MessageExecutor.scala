package io.casperlabs.casper.highway

import cats.implicits._
import cats.effect.{Concurrent, Sync}
import cats.mtl.FunctorRaise
import cats.effect.concurrent.Semaphore
import io.casperlabs.casper.api.BlockAPI
import io.casperlabs.casper.consensus.Block
import io.casperlabs.casper.consensus.info.BlockInfo
import io.casperlabs.casper.finality.MultiParentFinalizer
import io.casperlabs.casper.validation.Validation
import io.casperlabs.casper.validation.Validation.BlockEffects
import io.casperlabs.casper.validation.Errors.{DropErrorWrapper, ValidateErrorWrapper}
import io.casperlabs.casper.util.CasperLabsProtocol
import io.casperlabs.casper._
import io.casperlabs.casper.util.execengine.ExecEngineUtil
import io.casperlabs.catscontrib.Fs2Compiler
import io.casperlabs.catscontrib.{Fs2Compiler, MonadThrowable}
import io.casperlabs.catscontrib.effect.implicits.fiberSyntax
import io.casperlabs.crypto.codec.Base16
import io.casperlabs.crypto.Keys.PublicKeyBS
import io.casperlabs.ipc
import io.casperlabs.mempool.DeployBuffer
import io.casperlabs.models.Message
import io.casperlabs.models.BlockImplicits._
import io.casperlabs.metrics.Metrics
import io.casperlabs.metrics.implicits._ // for .timer syntax
import io.casperlabs.shared.{FatalError, Log, Time}
import io.casperlabs.storage.block.BlockStorage
import io.casperlabs.storage.deploy.{DeployStorage, DeployStorageWriter}
import io.casperlabs.storage.dag.{DagStorage, FinalityStorage}
import io.casperlabs.smartcontracts.ExecutionEngineService
import scala.util.control.NonFatal
import scala.util.control.NoStackTrace

/** A stateless class to encapsulate the steps to validate, execute and store a block. */
class MessageExecutor[F[_]: Concurrent: Log: Time: Metrics: BlockStorage: DagStorage: DeployStorage: BlockEventEmitter: Validation: CasperLabsProtocol: ExecutionEngineService: Fs2Compiler: MultiParentFinalizer: FinalityStorage: DeployBuffer](
    chainName: String,
    genesis: Block,
    upgrades: Seq[ipc.ChainSpec.UpgradePoint],
    maybeValidatorId: Option[PublicKeyBS]
) {

  private implicit val functorRaiseInvalidBlock =
    validation.raiseValidateErrorThroughApplicativeError[F]

  /** Validate, execute and persist an incoming block.
    * The blocks made by the MessageProducer don't have to be passed here.
    */
  def validateAndAdd(semaphore: Semaphore[F], block: Block, isBookingBlock: Boolean): F[Unit] =
    // If the block timestamp is in the future, wait some time before adding it,
    // so we won't include it as a justification from the future.
    Validation.preTimestamp[F](block).attempt.flatMap {
      case Right(Some(delay)) =>
        Log[F].info(
          s"${block.blockHash.show -> "block"} is ahead for $delay from now, will retry adding later"
        ) >>
          Time[F].sleep(delay) >>
          validateAndAdd(semaphore, block, isBookingBlock)

      case Right(None) =>
        semaphore.withPermit {
          for {
            (status, effects) <- computeEffects(block, isBookingBlock)
            _                 <- addEffects(status, block, effects)
          } yield ()
        }

      case _ =>
        semaphore.withPermit {
          Log[F]
            .warn(
              s"${block.blockHash.show -> "block"} timestamp exceeded threshold"
            ) >>
            addEffects(InvalidUnslashableBlock, block, BlockEffects.empty)
        }
    }

  /** Carry out maintenance after a message has been added either by this validator or another one.
    * This used to happen together with validation, however that meant that messages created by this
    * validator was also validated, so executed twice. Now messages are created by the `MessageProducer`,
    * so this method needs to be accessible on its own. However it should not be called by the `MessageProducer`
    * itself, because that's not supposed to have side effects beyond persistence, and this here can emit events
    * which end up visible to the outside world.
    *
    * Return a wait handle.
    */
  def effectsAfterAdded(message: ValidatedMessage): F[F[Unit]] =
    for {
      _ <- markDeploysAsProcessed(message)
      // Forking event emissions so as not to hold up block processing.
      w1 <- BlockEventEmitter[F].blockAdded(message.messageHash).forkAndLog
      w2 <- updateLastFinalizedBlock(message)
    } yield w1 *> w2

  private def updateLastFinalizedBlock(message: Message): F[F[Unit]] =
    for {
      result <- MultiParentFinalizer[F].onNewMessageAdded(message)
      w <- result.traverse {
            case MultiParentFinalizer.FinalizedBlocks(mainParent, _, secondary) => {
              val mainParentFinalizedStr = mainParent.show
              val secondaryParentsFinalizedStr =
                secondary.map(_.show).mkString("{", ", ", "}")
              for {
                _ <- Log[F].info(
                      s"New last finalized block hashes are ${mainParentFinalizedStr -> null}, ${secondaryParentsFinalizedStr -> null}."
                    )
                _  <- FinalityStorage[F].markAsFinalized(mainParent, secondary)
                w1 <- DeployBuffer[F].removeFinalizedDeploys(secondary + mainParent).forkAndLog
                w2 <- BlockEventEmitter[F].newLastFinalizedBlock(mainParent, secondary).forkAndLog
              } yield w1 *> w2
            }
          }
    } yield w getOrElse ().pure[F]

  private def markDeploysAsProcessed(message: Message): F[Unit] =
    for {
      block            <- BlockStorage[F].getBlockUnsafe(message.messageHash)
      processedDeploys = block.getBody.deploys.map(_.getDeploy).toList
      _                <- DeployStorageWriter[F].markAsProcessed(processedDeploys)
    } yield ()

  /** Carry out the effects according to the status:
    * - store valid blocks
    * - store invalid but attributable blocks
    * - raise and error for unattributable errors to stop further processing
    */
  private def addEffects(
      status: BlockStatus,
      block: Block,
      blockEffects: BlockEffects
  ): F[Unit] =
    status match {
      case MissingBlocks =>
        Sync[F].raiseError(
          new RuntimeException(
            "The DownloadManager should not give us a block with missing dependencies."
          )
        )

      case Valid =>
        save(block, blockEffects) *>
          Log[F].info(s"Added ${block.blockHash.show -> "block"}")

      case EquivocatedBlock | SelfEquivocatedBlock =>
        save(block, blockEffects) *>
          Log[F].info(s"Added equivocated ${block.blockHash.show -> "block"}") *>
          FatalError.selfEquivocationError(block.blockHash).whenA(status == SelfEquivocatedBlock)

      case status: StoredInvalid =>
        save(block, blockEffects) *>
          Log[F].warn(s"Added slashable ${block.blockHash.show -> "block"}: $status")

      case status: InvalidBlock =>
        Log[F].warn(s"Ignoring unslashable ${block.blockHash.show -> "block"}: $status") *>
          functorRaiseInvalidBlock.raise(status)

      case Processing | Processed =>
        Sync[F].raiseError(
          new IllegalStateException("A block should not be processing at this stage.")
            with NoStackTrace
        )

      case UnexpectedBlockException(ex) =>
        Log[F].error(
          s"Encountered exception in while processing ${block.blockHash.show -> "block"}: $ex"
        ) >>
          ex.raiseError[F, Unit]
    }

  /** Save the block to the block and DAG storage. */
  private def save(block: Block, blockEffects: BlockEffects): F[Unit] =
    BlockStorage[F].put(block, blockEffects.effects)

  // NOTE: Don't call this on genesis, genesis is presumed to be already computed and saved.
  def computeEffects(
      block: Block,
      isBookingBlock: Boolean
  ): F[(BlockStatus, BlockEffects)] = {
    import io.casperlabs.casper.validation.ValidationImpl.metricsSource
    Metrics[F].timer("computeEffects") {
      val hashPrefix = block.blockHash.show
      val effectsF: F[BlockEffects] = for {
        _   <- Log[F].info(s"Attempting to add $isBookingBlock ${hashPrefix -> "block"} to the DAG.")
        dag <- DagStorage[F].getRepresentation
        _   <- Validation[F].blockFull(block, dag, chainName, genesis.some)
        // Confirm the parents are correct (including checking they commute) and capture
        // the effect needed to compute the correct pre-state as well.
        _      <- Log[F].debug(s"Validating the parents of ${hashPrefix -> "block"}")
        merged <- Validation[F].parents(block, dag)
        // TODO (CON-626): Pass the isBookingBlock information to the effects calculation. Or should it be computePrestate?
        _ <- Log[F].debug(
              s"Computing the pre-state hash of $isBookingBlock ${hashPrefix -> "block"}"
            )
        preStateHash <- ExecEngineUtil
                         .computePrestate[F](merged, block.mainRank, upgrades) //TODO: This should probably use p-rank
                         .timer("computePrestate")
        preStateBonds = merged.parents.headOption.getOrElse(block).getHeader.getState.bonds
        _             <- Log[F].debug(s"Computing the effects for ${hashPrefix -> "block"}")
        blockEffects <- ExecEngineUtil
                         .effectsForBlock[F](block, preStateHash)
                         .recoverWith {
                           case NonFatal(ex) =>
                             Log[F].error(
                               s"Could not calculate effects for ${hashPrefix -> "block"}: $ex"
                             ) *>
                               FunctorRaise[F, InvalidBlock].raise(InvalidTransaction)
                         }
                         .timer("effectsForBlock")
        gasSpent = block.getBody.deploys.foldLeft(0L) { case (acc, next) => acc + next.cost }
        _ <- Metrics[F]
              .incrementCounter("gas_spent", gasSpent)
        _ <- Log[F].debug(s"Validating the transactions in ${hashPrefix -> "block"}")
        _ <- Validation[F].transactions(
              block,
              preStateHash,
              preStateBonds,
              blockEffects
            )
        // TODO: The invalid block tracker used to be a transient thing, it didn't survive a restart.
        // It's not clear why we need to do this, the DM will not download a block if it depends on
        // an invalid one that could not be validated. Is it equivocations? Wouldn't the hash change,
        // because of hashing affecting the post state hash?
        // _ <- Log[F].debug(s"Validating neglection for ${hashPrefix -> "block"}")
        // _ <- Validation[F]
        //       .neglectedInvalidBlock(
        //         block,
        //         invalidBlockTracker = Set.empty
        //       )
        _ <- Log[F].debug(s"Checking equivocation for ${hashPrefix -> "block"}")
        _ <- Validation[F].checkEquivocation(dag, block).timer("checkEquivocationsWithUpdate")
        _ <- Log[F].debug(s"Block effects calculated for ${hashPrefix -> "block"}")
      } yield blockEffects

      effectsToStatus(block, effectsF)
    }
  }

  private def effectsToStatus(
      block: Block,
      effects: F[Validation.BlockEffects]
  ): F[(BlockStatus, BlockEffects)] = {
    def validBlock(effects: BlockEffects)  = ((Valid: BlockStatus)  -> effects).pure[F]
    def invalidBlock(status: InvalidBlock) = ((status: BlockStatus) -> BlockEffects.empty).pure[F]

    effects.attempt.flatMap {
      case Right(effects) =>
        validBlock(effects)

      case Left(DropErrorWrapper(invalid)) =>
        // These exceptions are coming from the validation checks that used to happen outside attemptAdd,
        // the ones that returned boolean values.
        invalidBlock(invalid)

      case Left(ValidateErrorWrapper(EquivocatedBlock))
          if maybeValidatorId.contains(block.getHeader.validatorPublicKey) =>
        // NOTE: This will probably not be detected any more like this,
        // since the blocks made by the MessageProducer are not normally
        // validated, to avoid double execution.
        invalidBlock(SelfEquivocatedBlock)

      case Left(ValidateErrorWrapper(invalid)) =>
        invalidBlock(invalid)

      case Left(ex) =>
        Log[F].error(
          s"Unexpected exception during validation of ${block.blockHash.show -> "block"}: $ex"
        ) *>
          ex.raiseError[F, (BlockStatus, BlockEffects)]
    }
  }
}

package io.casperlabs.casper.highway

import cats._
import cats.data.NonEmptyList
import cats.implicits._
import cats.effect.{Clock, Concurrent}
import cats.effect.concurrent.Semaphore
import com.google.protobuf.ByteString
import io.casperlabs.casper.{DeployFilters, Estimator, ValidatorIdentity}
import io.casperlabs.casper.DeploySelection.DeploySelection
import io.casperlabs.casper.consensus.{Block, Era}
import io.casperlabs.casper.consensus.Block.Justification
import io.casperlabs.casper.consensus.state.ProtocolVersion
import io.casperlabs.catscontrib.MonadThrowable
import io.casperlabs.catscontrib.effect.implicits.fiberSyntax
import io.casperlabs.crypto.Keys.{PublicKey, PublicKeyBS}
import io.casperlabs.models.Message
import io.casperlabs.metrics.Metrics
import io.casperlabs.storage.BlockHash
import io.casperlabs.storage.era.EraStorage
import io.casperlabs.storage.dag.{DagRepresentation, DagStorage}
import io.casperlabs.storage.block.BlockStorage
import io.casperlabs.storage.deploy.{DeployStorage, DeployStorageReader}
import io.casperlabs.casper.util.execengine.ExecEngineUtil
import io.casperlabs.casper.util.execengine.ExecEngineUtil.{MergeResult, TransformMap}
import io.casperlabs.casper.util.{CasperLabsProtocol, ProtoUtil}
import io.casperlabs.casper.util.ProtocolVersions.Config
import io.casperlabs.shared.Log
import io.casperlabs.shared.Sorting.byteStringOrdering
import io.casperlabs.mempool.DeployBuffer
import io.casperlabs.smartcontracts.ExecutionEngineService
import io.casperlabs.ipc
import io.casperlabs.casper.PrettyPrinter
import io.casperlabs.casper.dag.DagOperations
import io.casperlabs.models.Message.{asMainRank, JRank, MainRank}
import io.casperlabs.shared.Sorting._
import scala.concurrent.duration._

/** Produce a signed message, persisted message.
  * The producer should the thread safe, so that when it's
  * called from multiple threads to produce ballots in response
  * to different messages it doesn't create an equivocation.
  */
trait MessageProducer[F[_]] {
  def validatorId: PublicKeyBS

  def ballot(
      keyBlockHash: BlockHash,
      roundId: Ticks,
      target: BlockHash,
      // For lambda responses we want to limit the justifications to just direct ones.
      justifications: Map[PublicKeyBS, Set[BlockHash]]
  ): F[Message.Ballot]

  /** Pick whatever secondary parents are compatible with the chosen main parent
    * and the justifications selected when the caller started their operation,
    * select deploys from the buffer, and create a (possibly empty) block,
    * persisting it to the block store.
    */
  def block(
      keyBlockHash: BlockHash,
      roundId: Ticks,
      mainParent: BlockHash,
      justifications: Map[PublicKeyBS, Set[BlockHash]],
      isBookingBlock: Boolean
  ): F[Message.Block]
}

object MessageProducer {
  def apply[F[_]: Concurrent: Clock: Log: Metrics: DagStorage: BlockStorage: DeployBuffer: DeployStorage: EraStorage: CasperLabsProtocol: ExecutionEngineService: DeploySelection](
      validatorIdentity: ValidatorIdentity,
      chainName: String,
      upgrades: Seq[ipc.ChainSpec.UpgradePoint],
      onlyTakeOwnLatestFromJustifications: Boolean = false,
      asyncRequeueOrphans: Boolean = true
  ): MessageProducer[F] =
    new MessageProducer[F] {
      override val validatorId =
        PublicKey(ByteString.copyFrom(validatorIdentity.publicKey))

      override def ballot(
          keyBlockHash: BlockHash,
          roundId: Ticks,
          target: BlockHash,
          justifications: Map[PublicKeyBS, Set[BlockHash]]
      ): F[Message.Ballot] =
        for {
          parent        <- BlockStorage[F].getBlockSummaryUnsafe(target)
          parentMessage <- MonadThrowable[F].fromTry(Message.fromBlockSummary(parent))
          props         <- messageProps(keyBlockHash, List(parentMessage), justifications)
          timestamp     <- Clock[F].currentTimeMillis

          signed = ProtoUtil.ballot(
            props.justifications,
            parent.getHeader.getState.postStateHash,
            parent.getHeader.getState.bonds,
            props.protocolVersion,
            parent.blockHash,
            props.validatorSeqNum,
            props.validatorPrevBlockHash,
            chainName,
            timestamp,
            props.jRank,
            props.mainRank,
            validatorIdentity.publicKey,
            validatorIdentity.privateKey,
            validatorIdentity.signatureAlgorithm,
            keyBlockHash,
            roundId
          )

          message <- MonadThrowable[F].fromTry(Message.fromBlock(signed))

          _ <- BlockStorage[F].put(signed, transforms = Map.empty)

        } yield message.asInstanceOf[Message.Ballot]

      override def block(
          keyBlockHash: BlockHash,
          roundId: Ticks,
          mainParent: BlockHash,
          justifications: Map[PublicKeyBS, Set[BlockHash]],
          isBookingBlock: Boolean
      ): F[Message.Block] =
        for {
          dag          <- DagStorage[F].getRepresentation
          merged       <- selectParents(dag, keyBlockHash, mainParent, justifications)
          parentHashes = merged.parents.map(_.blockHash).toSet
          _            <- startRequeueingOrphanedDeploys(parentHashes)
          parentMessages <- MonadThrowable[F].fromTry {
                             merged.parents.toList.traverse(Message.fromBlock(_))
                           }
          props     <- messageProps(keyBlockHash, parentMessages, justifications)
          timestamp <- Clock[F].currentTimeMillis

          remainingHashes <- DeployBuffer[F].remainingDeploys(
                              dag,
                              parentHashes,
                              timestamp,
                              props.configuration.deployConfig
                            )

          deployStream = DeployStorageReader[F]
            .getByHashes(remainingHashes)
            .through(
              DeployFilters.Pipes.dependenciesMet[F](dag, parentHashes)
            )

          checkpoint <- ExecEngineUtil
                         .computeDeploysCheckpoint[F](
                           merged,
                           deployStream,
                           timestamp,
                           props.protocolVersion,
                           props.mainRank,
                           upgrades
                         )

          magicBit = scala.util.Random.nextBoolean()

          signed = ProtoUtil.block(
            props.justifications,
            checkpoint.preStateHash,
            checkpoint.postStateHash,
            checkpoint.bondedValidators,
            checkpoint.deploysForBlock,
            props.protocolVersion,
            merged.parents.map(_.blockHash),
            props.validatorSeqNum,
            props.validatorPrevBlockHash,
            chainName,
            timestamp,
            props.jRank,
            props.mainRank,
            validatorIdentity.publicKey,
            validatorIdentity.privateKey,
            validatorIdentity.signatureAlgorithm,
            keyBlockHash,
            roundId,
            magicBit
          )

          message <- MonadThrowable[F].fromTry(Message.fromBlock(signed))

          _ <- BlockStorage[F].put(signed, transforms = checkpoint.stageEffects)

        } yield message.asInstanceOf[Message.Block]

      // NOTE: Currently this will requeue deploys in the background, some will make it, some won't.
      // This made sense with the AutoProposer, since a new block could be proposed any time;
      // in Highway that's going to the next round, whenever that is.
      private def startRequeueingOrphanedDeploys(parentHashes: Set[BlockHash]): F[Unit] = {
        val requeue =
          DeployBuffer[F].requeueOrphanedDeploys(parentHashes) >>= { requeued =>
            Log[F].info(s"Re-queued ${requeued.size} orphaned deploys.").whenA(requeued.nonEmpty)
          }
        if (asyncRequeueOrphans) requeue.forkAndLog.void else requeue
      }

      private def messageProps(
          keyBlockHash: BlockHash,
          parents: Seq[Message],
          justifications: Map[PublicKeyBS, Set[BlockHash]]
      ): F[MessageProps] =
        // NCB used to filter justifications to be only the bonded ones.
        // In Highway, we must include the justifications of the parent era _when_ there's a
        // new message that that we haven't included before. Transitive elimination should
        // take care of it, eventually, when it's implemented.
        for {
          // NOTE: The validator sequence number restarts in each era, and `justifications`
          // can contain entries for the parent era as well as the child.
          justificationMessages <- justifications.values.flatten.toSet.toList
                                    .traverse { h =>
                                      BlockStorage[F]
                                        .getBlockSummaryUnsafe(h)
                                        .flatMap { s =>
                                          MonadThrowable[F].fromTry(Message.fromBlockSummary(s))
                                        }
                                    }

          // Find the latest justification of the validator. They might be eliminated with transitives.
          validatorLatests = justificationMessages.filter { j =>
            j.validatorId == validatorId && j.eraId == keyBlockHash
          }

          // If they were eliminated we can look them up, as long as we make sure they don't change
          // due to concurrency since the time the justifications were collected, otherwise we could
          // be equivocating. This, currently, is catered for by a Semaphore in the EraRuntime around
          // the use of MessageProducer, which spans the fork choice as well as the block production.
          ownLatests <- if (validatorLatests.nonEmpty || onlyTakeOwnLatestFromJustifications)
                         validatorLatests.pure[F]
                       else {
                         for {
                           dag  <- DagStorage[F].getRepresentation
                           tips <- dag.latestInEra(keyBlockHash)
                           // Looking up hashes should be faster than full messages, and the following lookup is cached.
                           // Alternatively we could traverse the DAG from the justifications (currently transitions are eliminated).
                           ownLatestsHashes <- tips.latestMessageHash(validatorId)
                           ownLatests       <- ownLatestsHashes.toList.traverse(dag.lookupUnsafe)
                         } yield ownLatests
                       }

          maybeOwnLatest = Option(ownLatests)
            .filterNot(_.isEmpty)
            .map(_.maxBy(_.validatorMsgSeqNum))

          validatorSeqNum        = maybeOwnLatest.fold(1)(_.validatorMsgSeqNum + 1)
          validatorPrevBlockHash = maybeOwnLatest.fold(ByteString.EMPTY)(_.messageHash)

          // Genesis is for example not part of the justifications, so to be safe include parents too.
          jRank           = ProtoUtil.nextJRank(parents ++ justificationMessages)
          mainRank        = ProtoUtil.nextMainRank(parents.toList)
          config          <- CasperLabsProtocol[F].configAt(mainRank)
          protocolVersion <- CasperLabsProtocol[F].versionAt(mainRank)
        } yield MessageProps(
          validatorSeqNum,
          validatorPrevBlockHash,
          jRank,
          mainRank,
          config,
          protocolVersion,
          ProtoUtil.toJustification(justificationMessages)
        )
    }

  case class MessageProps(
      validatorSeqNum: Int,
      validatorPrevBlockHash: BlockHash,
      jRank: JRank,
      mainRank: MainRank,
      configuration: Config,
      protocolVersion: ProtocolVersion,
      justifications: Seq[Justification]
  )

  /** Pick secondary parents that don't conflict with the already chosen fork choice. */
  def selectParents[F[_]: MonadThrowable: Metrics: BlockStorage: DagStorage: EraStorage](
      dag: DagRepresentation[F],
      keyBlockHash: BlockHash,
      mainParent: BlockHash,
      justifications: Map[PublicKeyBS, Set[BlockHash]]
  ): F[MergeResult[TransformMap, Block]] = {
    // TODO (CON-627): The merge doesn't deal with ballots that are cast in response to protocol
    // messages such as the switch block or lambda message. It wouldn't properly return all possible
    // secondary parents. Better would be if the fork choice result already contained them.
    // Until that's fixed, and other stuff noted in CON-628, don't use secondary parents at all.
    val secondaryParentsEnabled = false

    val secondaryCandidates: F[List[BlockHash]] =
      for {
        latestMessages <- (mainParent +: justifications.values.flatten.toList)
                           .traverse(dag.lookupUnsafe(_))
                           .map(NonEmptyList.fromListUnsafe(_))
        tips         <- Estimator.tipsOfLatestMessages[F](dag, latestMessages, stopHash = keyBlockHash)
        equivocators <- collectEquivocators[F](keyBlockHash)
        // TODO: There are no scores here for ordering secondary parents. Another reason for the fork choice to give these.
        secondaries = tips
          .filterNot(m => equivocators(m.validatorId) || m.messageHash == mainParent)
          .map(_.messageHash)
          .sorted
      } yield secondaries

    for {
      secondaries <- if (secondaryParentsEnabled) secondaryCandidates else Nil.pure[F]
      candidates  = NonEmptyList(mainParent, secondaries)
      blocks      <- candidates.traverse(BlockStorage[F].getBlockUnsafe)
      merged      <- ExecEngineUtil.merge[F](blocks, dag)
    } yield merged
  }

  /** Gather all the unforgiven equivocators that are in eras between the era of the key block and the era
    * defined _by_ the keyblock itself (typically that means grandparent, parent and child era).
    */
  def collectEquivocators[F[_]: MonadThrowable: EraStorage: DagStorage](
      keyBlockHash: BlockHash
  ): F[Set[ByteString]] =
    for {
      dag               <- DagStorage[F].getRepresentation
      keyBlocks         <- collectKeyBlocks[F](keyBlockHash)
      eraLatestMessages <- DagOperations.latestMessagesInEras[F](dag, keyBlocks)
      equivocators      = eraLatestMessages.map(_._2.filter(_._2.size > 1)).map(_.keySet).flatten.toSet
    } yield equivocators

  /** Collects key blocks between an era identified by [[keyBlockHash]] (current era)
    * and an era in which that key block was created (most probably a grandparent era).
    */
  def collectKeyBlocks[F[_]: MonadThrowable: DagStorage: EraStorage](
      keyBlockHash: BlockHash
  ): F[List[Message.Block]] =
    for {
      eras           <- collectEras[F](keyBlockHash)
      keyBlockHashes = eras.map(_.keyBlockHash)
      dag            <- DagStorage[F].getRepresentation
      keyBlocks      <- keyBlockHashes.traverse(dag.lookupBlockUnsafe(_))
    } yield keyBlocks

  /** Collects ancestor eras between an era identified by [[keyBlockHash]] (current era)
    * and an era in which that key block was created (most probably a grandparent era).
    */
  def collectEras[F[_]: MonadThrowable: EraStorage: DagStorage](
      keyBlockHash: BlockHash
  ): F[List[Era]] =
    for {
      dag      <- DagStorage[F].getRepresentation
      keyBlock <- dag.lookupUnsafe(keyBlockHash)
      startEra <- EraStorage[F].getEraUnsafe(keyBlockHash)
      eras <- DagOperations
               .bfTraverseF(List(startEra)) { era =>
                 List(era.parentKeyBlockHash)
                   .filterNot(_.isEmpty) // Don't follow further than Genesis era.
                   .traverse(EraStorage[F].getEraUnsafe(_))
               }
               .takeUntil(_.keyBlockHash == keyBlock.eraId)
               .toList
    } yield eras.reverse
}

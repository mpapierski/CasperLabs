package io.casperlabs.storage

import java.util.concurrent.TimeUnit

import cats.effect.Sync
import cats.implicits._
import doobie.util.transactor.Transactor
import io.casperlabs.casper.consensus.info.{BlockInfo, DeployInfo}
import io.casperlabs.casper.consensus.{Block, BlockSummary, Era}
import io.casperlabs.catscontrib.MonadThrowable
import io.casperlabs.metrics.Metrics
import io.casperlabs.models.Message
import io.casperlabs.shared.Time
import io.casperlabs.storage.block.{BlockStorage, SQLiteBlockStorage}
import io.casperlabs.storage.dag.DagRepresentation.Validator
import io.casperlabs.storage.dag._
import io.casperlabs.storage.deploy.{
  DeployStorage,
  DeployStorageReader,
  DeployStorageWriter,
  SQLiteDeployStorage
}
import io.casperlabs.storage.era.{EraStorage, SQLiteEraStorage}
import fs2._

object SQLiteStorage {
  type CombinedStorage[F[_]] =
    BlockStorage[F]
      with DagStorage[F]
      with DeployStorage[F]
      with DagRepresentation[F]
      with FinalityStorage[F]
      with AncestorsStorage[F]
      with EraStorage[F]

  def create[F[_]: Sync: Metrics: Time](
      deployStorageChunkSize: Int = 100,
      tickUnit: TimeUnit = TimeUnit.MILLISECONDS,
      readXa: Transactor[F],
      writeXa: Transactor[F]
  ): F[CombinedStorage[F]] =
    create[F](
      deployStorageChunkSize = deployStorageChunkSize,
      tickUnit = tickUnit,
      readXa = readXa,
      writeXa = writeXa,
      wrapBlockStorage = (_: BlockStorage[F]).pure[F],
      wrapDagStorage =
        (_: DagStorage[F] with DagRepresentation[F] with FinalityStorage[F] with AncestorsStorage[
          F
        ]).pure[F]
    )

  def create[F[_]: Sync: Metrics: Time](
      deployStorageChunkSize: Int,
      tickUnit: TimeUnit,
      readXa: Transactor[F],
      writeXa: Transactor[F],
      wrapBlockStorage: BlockStorage[F] => F[BlockStorage[F]],
      wrapDagStorage: DagStorage[F] with DagRepresentation[F] with FinalityStorage[F] with AncestorsStorage[
        F
      ] => F[
        DagStorage[F] with DagRepresentation[F] with FinalityStorage[F] with AncestorsStorage[
          F
        ]
      ]
  ): F[CombinedStorage[F]] =
    for {
      blockStorage  <- SQLiteBlockStorage.create[F](readXa, writeXa) >>= wrapBlockStorage
      dagStorage    <- SQLiteDagStorage.create[F](readXa, writeXa) >>= wrapDagStorage
      deployStorage <- SQLiteDeployStorage.create[F](deployStorageChunkSize, readXa, writeXa)
      eraStorage    <- SQLiteEraStorage.create[F](tickUnit, readXa, writeXa)
    } yield new BlockStorage[F]
      with DagStorage[F]
      with DeployStorage[F]
      with DagRepresentation[F]
      with AncestorsStorage[F]
      with FinalityStorage[F]
      with EraStorage[F] {

      override def writer: DeployStorageWriter[F] =
        deployStorage.writer
      override def reader(implicit dv: DeployInfo.View): DeployStorageReader[F] =
        deployStorage.reader

      override def getRepresentation: F[DagRepresentation[F]] = dagStorage.getRepresentation

      override def insert(block: Block): F[DagRepresentation[F]] =
        dagStorage.insert(block)

      override def checkpoint(): F[Unit] = dagStorage.checkpoint()

      override def clear(): F[Unit] =
        for {
          _ <- deployStorage.writer.clear()
          _ <- dagStorage.clear()
          _ <- blockStorage.clear()
        } yield ()

      override def close(): F[Unit] =
        for {
          _ <- deployStorage.writer.close()
          _ <- dagStorage.close()
          _ <- blockStorage.close()
        } yield ()

      override def get(
          blockHash: BlockHash
      )(implicit dv: DeployInfo.View = DeployInfo.View.FULL): F[Option[BlockMsgWithTransform]] =
        blockStorage.get(blockHash)

      override def getByPrefix(
          blockHashPrefix: String
      )(implicit dv: DeployInfo.View = DeployInfo.View.FULL): F[Option[BlockMsgWithTransform]] =
        blockStorage.getByPrefix(blockHashPrefix)

      override def isEmpty: F[Boolean] = blockStorage.isEmpty

      override def getBlockInfoByPrefix(blockHashPrefix: String): F[Option[BlockInfo]] =
        blockStorage.getBlockInfoByPrefix(blockHashPrefix)

      override def getBlockInfo(blockHash: BlockHash): F[Option[BlockInfo]] =
        blockStorage.getBlockInfo(blockHash)

      override def put(
          blockHash: BlockHash,
          blockMsgWithTransform: BlockMsgWithTransform
      ): F[Unit] =
        for {
          _ <- blockMsgWithTransform.blockMessage.fold(().pure[F])(
                b => deployStorage.writer.addAsExecuted(b) >> dagStorage.insert(b).void
              )
          _ <- blockStorage.put(blockHash, blockMsgWithTransform)
        } yield ()

      override def getBlockSummary(blockHash: BlockHash): F[Option[BlockSummary]] =
        dagStorage.lookup(blockHash).map(_.map(_.blockSummary))

      override def findBlockHashesWithDeployHashes(
          deployHashes: List[DeployHash]
      ): F[Map[DeployHash, Set[BlockHash]]] =
        blockStorage.findBlockHashesWithDeployHashes(deployHashes)

      override def children(blockHash: BlockHash): F[Set[BlockHash]] =
        dagStorage.children(blockHash)

      override def justificationToBlocks(blockHash: BlockHash): F[Set[BlockHash]] =
        dagStorage.justificationToBlocks(blockHash)

      override def lookup(blockHash: BlockHash): F[Option[Message]] =
        dagStorage.lookup(blockHash)

      override def contains(blockHash: BlockHash): F[Boolean] =
        dagStorage.contains(blockHash)

      override def topoSort(
          startBlockNumber: Long,
          endBlockNumber: Long
      ): Stream[F, Vector[BlockInfo]] =
        dagStorage.topoSort(startBlockNumber, endBlockNumber)

      override def topoSort(startBlockNumber: Long): Stream[F, Vector[BlockInfo]] =
        dagStorage.topoSort(startBlockNumber)

      override def topoSortTail(tailLength: Int): Stream[F, Vector[BlockInfo]] =
        dagStorage.topoSortTail(tailLength)

      override def getBlockInfosByValidator(
          validator: Validator,
          limit: Int,
          lastTimeStamp: Long,
          lastBlockHash: BlockHash
      ) =
        dagStorage.getBlockInfosByValidator(validator, limit, lastTimeStamp, lastBlockHash)

      override def latestGlobal =
        dagStorage.getRepresentation.flatMap(_.latestGlobal)

      override def latestInEra(keyBlockHash: BlockHash) =
        dagStorage.getRepresentation.flatMap(_.latestInEra(keyBlockHash))

      override def addEra(era: Era)               = eraStorage.addEra(era)
      override def getEra(eraId: BlockHash)       = eraStorage.getEra(eraId)
      override def getChildEras(eraId: BlockHash) = eraStorage.getChildEras(eraId)
      override def getChildlessEras               = eraStorage.getChildlessEras

      override def markAsFinalized(
          mainParent: BlockHash,
          secondary: Set[BlockHash]
      ): F[Unit] =
        dagStorage.markAsFinalized(mainParent, secondary)

      override def getLastFinalizedBlock: F[BlockHash] = dagStorage.getLastFinalizedBlock

      override def isFinalized(block: BlockHash): F[Boolean] =
        dagStorage.isFinalized(block)

      override implicit val MT: MonadThrowable[F] = Sync[F]

      override def findAncestor(block: BlockHash, distance: Long) =
        dagStorage.findAncestor(block, distance)

    }
}

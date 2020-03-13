package io.casperlabs.node.casper

import java.util.concurrent.{TimeUnit, TimeoutException}

import cats._
import cats.data.NonEmptyList
import cats.effect._
import cats.effect.implicits._
import cats.implicits._
import com.google.protobuf.ByteString
import eu.timepit.refined.auto._
import fs2.interop.reactivestreams._
import io.casperlabs.casper._
import io.casperlabs.casper.consensus._
import io.casperlabs.casper.consensus.info.DeployInfo
import io.casperlabs.casper.util.CasperLabsProtocol
import io.casperlabs.casper.validation.Validation
import io.casperlabs.comm.ServiceError.{InvalidArgument, Unavailable}
import io.casperlabs.comm.discovery.NodeUtils._
import io.casperlabs.comm.discovery.{Node, NodeDiscovery}
import io.casperlabs.comm.gossiping._
import io.casperlabs.comm.gossiping.synchronization._
import io.casperlabs.comm.grpc._
import io.casperlabs.comm.{CachedConnections, NodeAsk}
import io.casperlabs.crypto.Keys.PublicKey
import io.casperlabs.crypto.codec.Base16
import io.casperlabs.metrics.Metrics
import io.casperlabs.node.casper.consensus.Consensus
import io.casperlabs.node.configuration.Configuration
import io.casperlabs.shared.{Log, Time}
import io.casperlabs.storage.block._
import io.casperlabs.storage.dag._
import io.casperlabs.storage.deploy.DeployStorage
import io.grpc.netty.{NegotiationType, NettyChannelBuilder}
import io.grpc.{ManagedChannel, Server}
import io.netty.handler.ssl.{ClientAuth, SslContext}
import monix.eval.TaskLike
import monix.execution.Scheduler
import monix.tail.Iterant

import scala.concurrent.duration._
import scala.util.Random
import scala.util.control.NonFatal

/** Create the Casper stack using the GossipService. */
package object gossiping {

  private implicit val metricsSource: Metrics.Source =
    Metrics.Source(Metrics.Source(Metrics.BaseSource, "node"), "gossiping")

  def apply[F[_]: Parallel: ConcurrentEffect: Log: Metrics: Time: Timer: BlockStorage: DagStorage: DeployStorage: NodeDiscovery: NodeAsk: CasperLabsProtocol: Consensus](
      port: Int,
      conf: Configuration,
      maybeValidatorId: Option[ValidatorIdentity],
      genesis: Block,
      ingressScheduler: Scheduler,
      egressScheduler: Scheduler,
      onInitialSyncCompleted: F[Unit]
  )(
      implicit logId: Log[Id],
      metricsId: Metrics[Id]
  ): Resource[F, Relaying[F]] = {

    val (cert, key) = conf.tls.readIntraNodeCertAndKey

    // SSL context to use when connecting to another node.
    val clientSslContext = SslContexts.forClient(cert, key)
    // SSL context to use when another node connects to us.
    val serverSslContext = SslContexts.forServer(cert, key, ClientAuth.REQUIRE)

    // For client stub to GossipService conversions.
    implicit val oi = ObservableIterant.default(implicitly[Effect[F]], egressScheduler)

    for {
      cachedConnections <- makeConnectionsCache(
                            conf,
                            clientSslContext,
                            egressScheduler
                          )

      connectToGossip: GossipService.Connector[F] = (node: Node) => {
        cachedConnections.connection(node, enforce = true) map { chan =>
          new GossipingGrpcMonix.GossipServiceStub(chan)
        } map { stub =>
          implicit val s = egressScheduler
          GrpcGossipService.toGossipService(
            stub,
            onError = {
              case Unavailable(_) | _: TimeoutException =>
                disconnect(cachedConnections, node) *> NodeDiscovery[F].banTemp(node)
            },
            timeout = conf.server.defaultTimeout
          )
        }
      }

      relaying <- makeRelaying(conf, connectToGossip)

      synchronizer <- makeSynchronizer(
                       conf,
                       connectToGossip
                     )

      downloadManager <- makeDownloadManager(
                          conf,
                          connectToGossip,
                          relaying,
                          synchronizer,
                          maybeValidatorId
                        )

      genesisApprover <- makeGenesisApprover(
                          conf,
                          maybeValidatorId,
                          connectToGossip,
                          downloadManager,
                          genesis
                        )

      // Make sure consensus is initialised before the synchronizer is resumed.
      awaitApproval <- makeFiberResource {
                        genesisApprover.awaitApproval.flatMap(Consensus[F].onGenesisApproved(_))
                      }

      // Start syncing with the bootstrap and/or some others in the background.
      awaitSynchronization <- Resource
                               .pure[F, Boolean](
                                 conf.server.bootstrap.nonEmpty && !conf.casper.standalone
                               )
                               .ifM(
                                 makeInitialSynchronizer(
                                   conf,
                                   downloadManager,
                                   synchronizer,
                                   connectToGossip,
                                   awaitApproval
                                 ),
                                 Resource.pure[F, F[Unit]](().pure[F])
                               )

      // Let the outside world know when we're done.
      _ <- makeFiberResource {
            awaitSynchronization >> onInitialSyncCompleted
          }

      // The stashing synchronizer waits for Genesis approval and the initial synchronization
      // to complete before actually syncing anything. We had to create the underlying
      // synchronizer earlier because the Download Manager needs a reference to it,
      // and the Download Manager is needed by the Initial Synchronizer.
      stashingSynchronizer <- Resource.liftF {
                               StashingSynchronizer.wrap(
                                 synchronizer,
                                 awaitApproval >> awaitSynchronization
                               )
                             }

      rateLimiter <- makeRateLimiter[F](conf)

      gossipServiceServer <- makeGossipServiceServer(
                              conf,
                              stashingSynchronizer,
                              downloadManager,
                              genesisApprover
                            )

      _ <- startGrpcServer(
            gossipServiceServer,
            rateLimiter,
            genesis.blockHash,
            serverSslContext,
            conf,
            port,
            ingressScheduler
          )

      _ <- makePeriodicSynchronizer(
            conf,
            gossipServiceServer,
            connectToGossip,
            awaitApproval >> awaitSynchronization
          )

      // Start a loop to periodically print peer count, new and disconnected peers, based on NodeDiscovery.
      _ <- makePeerCountPrinter

    } yield relaying
  }

  /** Cached connection resources, closed at the end. */
  private def makeConnectionsCache[F[_]: Concurrent: Log: Metrics](
      conf: Configuration,
      clientSslContext: SslContext,
      egressScheduler: Scheduler
  ): Resource[F, CachedConnections[F, Unit]] = Resource {
    for {
      makeCache <- CachedConnections[F, Unit]
      cache = makeCache {
        makeGossipChannel(
          _,
          conf,
          clientSslContext,
          egressScheduler
        )
      }
      shutdown = cache.read.flatMap { s =>
        s.connections.toList.traverse {
          case (peer, chan) =>
            Log[F].debug(s"Closing connection to ${peer.show -> "peer"}") *>
              Sync[F].delay(chan.shutdown()).attempt.void
        }.void
      }
    } yield (cache, shutdown)
  }

  /** Open a gRPC channel for gossiping. */
  private def makeGossipChannel[F[_]: Sync: Log](
      peer: Node,
      conf: Configuration,
      clientSslContext: SslContext,
      egressScheduler: Scheduler
  ): F[ManagedChannel] =
    for {
      _ <- Log[F].debug(s"Creating new channel to peer ${peer.show -> "peer"}")
      chan <- Sync[F].delay {
               NettyChannelBuilder
                 .forAddress(peer.host, peer.protocolPort)
                 .executor(egressScheduler)
                 .maxInboundMessageSize(conf.server.maxMessageSize)
                 .negotiationType(NegotiationType.TLS)
                 .sslContext(clientSslContext)
                 .overrideAuthority(Base16.encode(peer.id.toByteArray))
                 // https://github.com/grpc/grpc-java/issues/3470 indicates that the timeout will raise Unavailable when a connection is attempted.
                 .keepAliveTimeout(conf.server.defaultTimeout.toMillis, TimeUnit.MILLISECONDS)
                 .idleTimeout(60, TimeUnit.SECONDS) // This just puts the channel in a low resource state.
                 .build()
             }
    } yield chan

  /** Close and remove a cached connection. */
  private def disconnect[F[_]: Sync: Log](cache: CachedConnections[F, Unit], peer: Node): F[Unit] =
    cache.modify { s =>
      for {
        _ <- s.connections.get(peer).fold(().pure[F]) { c =>
              Log[F].debug(s"Disconnecting from peer ${peer.show -> "peer"}") *>
                Sync[F].delay(c.shutdown()).attempt.void
            }
      } yield s.copy(connections = s.connections - peer)
    }

  def makeRelaying[F[_]: Concurrent: Parallel: Log: Metrics: NodeDiscovery: NodeAsk](
      conf: Configuration,
      connectToGossip: GossipService.Connector[F]
  ): Resource[F, Relaying[F]] =
    Resource
      .liftF(RelayingImpl.establishMetrics[F])
      .as(
        RelayingImpl(
          NodeDiscovery[F],
          connectToGossip = connectToGossip,
          relayFactor = conf.server.relayFactor,
          relaySaturation = conf.server.relaySaturation
        )
      )

  private def makeDownloadManager[F[_]: Concurrent: Log: Time: Timer: Metrics: DagStorage: Consensus](
      conf: Configuration,
      connectToGossip: GossipService.Connector[F],
      relaying: Relaying[F],
      synchronizer: Synchronizer[F],
      maybeValidatorId: Option[ValidatorIdentity]
  ): Resource[F, DownloadManager[F]] =
    for {
      _ <- Resource.liftF(DownloadManagerImpl.establishMetrics[F])
      maybeValidatorPublicKey = maybeValidatorId
        .map(x => ByteString.copyFrom(x.publicKey))
        .filterNot(_.isEmpty)
      downloadManager <- DownloadManagerImpl[F](
                          maxParallelDownloads = conf.server.downloadMaxParallelBlocks,
                          connectToGossip = connectToGossip,
                          backend = new DownloadManagerImpl.Backend[F] {
                            override def hasBlock(blockHash: ByteString): F[Boolean] =
                              isInDag(blockHash)

                            override def validateBlock(block: Block): F[Unit] =
                              maybeValidatorPublicKey
                                .filter(_ == block.getHeader.validatorPublicKey)
                                .fold(().pure[F]) { _ =>
                                  Log[F]
                                    .warn(
                                      s"${PrettyPrinter.buildString(block) -> "block" -> null} seems to be created by a doppelganger using the same validator key!"
                                    )
                                } *>
                                Consensus[F].validateAndAddBlock(block)

                            override def storeBlock(block: Block): F[Unit] =
                              // Validation has already stored it.
                              ().pure[F]

                            override def storeBlockSummary(
                                summary: BlockSummary
                            ): F[Unit] =
                              // Storing the block automatically stores the summary as well.
                              ().pure[F]

                            override def onScheduled(summary: BlockSummary): F[Unit] =
                              Consensus[F].onScheduled(summary)

                            override def onDownloaded(blockHash: ByteString): F[Unit] =
                              // Calling `addBlock` during validation has already stored the block,
                              // so we have nothing more to do here with the consensus.
                              synchronizer.downloaded(blockHash)
                          },
                          relaying = relaying,
                          retriesConf = DownloadManagerImpl.RetriesConf(
                            maxRetries = conf.server.downloadMaxRetries,
                            initialBackoffPeriod = conf.server.downloadRetryInitialBackoffPeriod,
                            backoffFactor = conf.server.downloadRetryBackoffFactor
                          )
                        )
    } yield downloadManager

  // Even though we create the Genesis from the chainspec, the approver gives the green light to use it,
  // which could be based on the presence of other known validators, signaled by their approvals.
  // That just gives us the assurance that we are using the right chain spec because other are as well.
  private def makeGenesisApprover[F[_]: Concurrent: Log: Time: Timer: NodeDiscovery: BlockStorage: Consensus](
      conf: Configuration,
      maybeValidatorId: Option[ValidatorIdentity],
      connectToGossip: GossipService.Connector[F],
      downloadManager: DownloadManager[F],
      genesis: Block
  ): Resource[F, GenesisApprover[F]] =
    for {
      _ <- Resource.liftF {
            maybeValidatorId match {
              case Some(ValidatorIdentity(publicKey, _, _)) =>
                Log[F].info(
                  s"Starting with validator identity ${Base16.encode(publicKey) -> "validator"}"
                )
              case None =>
                Log[F].info("Starting without a validator identity.")
            }
          }

      _ <- Resource.liftF {
            for {
              // Validate it to make sure that code path is exercised.
              _ <- Log[F].info(
                    s"Trying to validate and run the Genesis ${show(genesis.blockHash) -> "candidate"}"
                  )
              _ <- Consensus[F].validateAndAddBlock(genesis)
            } yield ()
          }

      knownValidators <- Resource.liftF {
                          // Based on `CasperPacketHandler.of` in default mode.
                          CasperConf.parseValidatorsFile[F](conf.casper.knownValidatorsFile)
                        }

      // Produce an approval for a valid candiate if this node is a Genesis validator.
      maybeApproveBlock = (block: Block) =>
        maybeValidatorId
          .filter { id =>
            val publicKey = ByteString.copyFrom(id.publicKey)
            block.getHeader.getState.bonds.exists { bond =>
              bond.validatorPublicKey == publicKey
            }
          }
          .map { id =>
            val sig = id.signature(block.blockHash.toByteArray)
            Approval()
              .withApproverPublicKey(ByteString.copyFrom(id.publicKey))
              .withSignature(sig)
          }

      backend = new GenesisApproverImpl.Backend[F] {
        // Everyone has to have the same chain spec, so approval is just a matter of checking the candidate hash.
        override def validateCandidate(
            block: Block
        ): F[Either[Throwable, Option[Approval]]] =
          if (block.blockHash == genesis.blockHash) {
            maybeApproveBlock(block).asRight.pure[F]
          } else {
            InvalidArgument(
              s"${show(block.blockHash) -> "candidate"} did not equal the expected Genesis ${show(genesis.blockHash) -> "genesis"}"
            ).asLeft.pure[F].widen
          }

        override def canTransition(
            block: Block,
            signatories: Set[ByteString]
        ): Boolean =
          signatories.size >= conf.casper.requiredSigs &&
            knownValidators.forall(signatories(_))

        override def validateSignature(
            blockHash: ByteString,
            publicKey: ByteString,
            signature: Signature
        ): Boolean =
          Validation.signature(
            blockHash.toByteArray,
            signature,
            PublicKey(publicKey.toByteArray)
          )

        override def getBlock(blockHash: ByteString): F[Option[Block]] =
          BlockStorage[F]
            .get(blockHash)
            .map(_.map(_.getBlockMessage))
      }

      approver <- GenesisApproverImpl[F](
                   backend,
                   NodeDiscovery[F],
                   connectToGossip,
                   relayFactor = conf.server.approvalRelayFactor,
                   maybeBootstrapParams = NonEmptyList.fromList(
                     conf.server.bootstrap.map(_.withChainId(genesis.blockHash))
                   ) map { bootstraps =>
                     GenesisApproverImpl.BootstrapParams(
                       bootstraps,
                       conf.server.approvalPollInterval,
                       downloadManager
                     )
                   },
                   maybeGenesis = Some(genesis),
                   maybeApproval = maybeApproveBlock(genesis)
                 )
    } yield approver

  private def show(hash: ByteString) =
    PrettyPrinter.buildString(hash)

  def makeSynchronizer[F[_]: Concurrent: Parallel: Log: Metrics: DagStorage: Consensus: CasperLabsProtocol](
      conf: Configuration,
      connectToGossip: GossipService.Connector[F]
  ): Resource[F, Synchronizer[F]] = Resource.liftF {
    for {
      _ <- SynchronizerImpl.establishMetrics[F]
      synchronizer <- SynchronizerImpl[F](
                       connectToGossip,
                       new SynchronizerImpl.Backend[F] {

                         override def justifications: F[List[ByteString]] =
                           for {
                             dag    <- DagStorage[F].getRepresentation
                             latest <- dag.latestMessageHashes
                           } yield latest.values.flatten.toList

                         override def validate(blockSummary: BlockSummary): F[Unit] =
                           Consensus[F].validateSummary(blockSummary)

                         override def notInDag(blockHash: ByteString): F[Boolean] =
                           isInDag(blockHash).map(!_)
                       },
                       maxPossibleDepth = conf.server.syncMaxPossibleDepth,
                       minBlockCountToCheckWidth = conf.server.syncMinBlockCountToCheckWidth,
                       maxBondingRate = conf.server.syncMaxBondingRate,
                       maxDepthAncestorsRequest = conf.server.syncMaxDepthAncestorsRequest,
                       disableValidations = conf.server.syncDisableValidations
                     )
    } yield synchronizer
  }

  /** Check if we have a block yet. */
  private def isInDag[F[_]: Sync: DagStorage](blockHash: ByteString): F[Boolean] =
    for {
      dag  <- DagStorage[F].getRepresentation
      cont <- dag.contains(blockHash)
    } yield cont

  /** Create gossip service. */
  def makeGossipServiceServer[F[_]: ConcurrentEffect: Parallel: Log: Metrics: BlockStorage: DagStorage: DeployStorage: Consensus](
      conf: Configuration,
      synchronizer: Synchronizer[F],
      downloadManager: DownloadManager[F],
      genesisApprover: GenesisApprover[F]
  ): Resource[F, GossipServiceServer[F]] =
    for {
      backend <- Resource.pure[F, GossipServiceServer.Backend[F]] {
                  new GossipServiceServer.Backend[F] {
                    override def hasBlock(blockHash: ByteString): F[Boolean] =
                      isInDag(blockHash)

                    override def getBlockSummary(blockHash: ByteString): F[Option[BlockSummary]] =
                      BlockStorage[F]
                        .getBlockSummary(blockHash)

                    override def getBlock(
                        blockHash: ByteString,
                        deploysBodiesExcluded: Boolean
                    ): F[Option[Block]] =
                      BlockStorage[F]
                        .get(blockHash)(
                          if (deploysBodiesExcluded) DeployInfo.View.BASIC
                          else DeployInfo.View.FULL
                        )
                        .map(_.map(_.getBlockMessage))

                    override def getDeploys(deployHashes: Set[ByteString]): Iterant[F, Deploy] =
                      Iterant.fromReactivePublisher {
                        DeployStorage[F]
                          .reader(DeployInfo.View.FULL)
                          .getByHashes(deployHashes)
                          .toUnicastPublisher
                      }

                    /** Returns latest messages as seen currently by the node.
                      * NOTE: In the future we will remove redundant messages. */
                    override def latestMessages: F[Set[Block.Justification]] =
                      Consensus[F].latestMessages

                    override def dagTopoSort(
                        startRank: Long,
                        endRank: Long
                    ): Iterant[F, BlockSummary] = {
                      import fs2.interop.reactivestreams._
                      Iterant
                        .liftF(for {
                          dag <- DagStorage[F].getRepresentation
                          stream = dag
                            .topoSort(startRank, endRank)
                            .flatMap(blockInfo => fs2.Stream.emits(blockInfo.flatMap(_.summary)))
                          publisher = stream.toUnicastPublisher()
                          iterant   = Iterant.fromReactivePublisher(publisher)
                        } yield iterant)
                        .flatten
                    }
                  }
                }

      server <- Resource.liftF {
                 GossipServiceServer[F](
                   backend,
                   synchronizer,
                   downloadManager,
                   genesisApprover,
                   maxChunkSize = conf.server.chunkSize,
                   maxParallelBlockDownloads = conf.server.relayMaxParallelBlocks
                 )
               }

    } yield server

  /** Initially sync with the bootstrap node and/or some others.
    * Returns handle which will be resolved when initial synchronization is finished. */
  private def makeInitialSynchronizer[F[_]: Concurrent: Parallel: Log: Timer: NodeDiscovery: DagStorage: Consensus](
      conf: Configuration,
      downloadManager: DownloadManager[F],
      synchronizer: Synchronizer[F],
      connectToGossip: GossipService.Connector[F],
      awaitApproved: F[Unit]
  ): Resource[F, F[Unit]] =
    for {
      initialSync <- Resource.liftF {
                      Consensus[F].lastSynchronizedRank >>= { startRank =>
                        Sync[F].delay(
                          new InitialSynchronizationForwardImpl[F](
                            NodeDiscovery[F],
                            selectNodes =
                              ns => Random.shuffle(ns).take(conf.server.initSyncMaxNodes),
                            minSuccessful = conf.server.initSyncMinSuccessful,
                            memoizeNodes = conf.server.initSyncMemoizeNodes,
                            skipFailedNodesInNextRounds = conf.server.initSyncSkipFailedNodes,
                            connector = connectToGossip,
                            downloadManager = downloadManager,
                            synchronizer = synchronizer,
                            step = conf.server.initSyncStep,
                            rankStartFrom = startRank,
                            roundPeriod = conf.server.initSyncRoundPeriod
                          )
                        )
                      }
                    }
      handle <- makeFiberResource {
                 for {
                   _         <- awaitApproved
                   awaitSync <- initialSync.sync()
                   _         <- awaitSync
                   _         <- Log[F].info(s"Initial synchronization complete.")
                 } yield ()
               }
    } yield handle

  /** Periodically sync with a random node. */
  private def makePeriodicSynchronizer[F[_]: Concurrent: Parallel: Log: Timer: NodeDiscovery](
      conf: Configuration,
      gossipServiceServer: GossipServiceServer[F],
      connectToGossip: GossipService.Connector[F],
      awaitApproved: F[Unit]
  ): Resource[F, Unit] = {
    def loop(synchronizer: InitialSynchronization[F]): F[Unit] = {
      val syncOne: F[Unit] = for {
        _        <- Time[F].sleep(conf.server.periodicSyncRoundPeriod)
        hasPeers <- NodeDiscovery[F].recentlyAlivePeersAscendingDistance.map(_.nonEmpty)
        _        <- synchronizer.sync().flatMap(identity).whenA(hasPeers)
      } yield ()

      syncOne.attempt >> loop(synchronizer)
    }

    for {
      periodicSync <- Resource.pure[F, InitialSynchronization[F]] {
                       new InitialSynchronizationBackwardImpl[F](
                         NodeDiscovery[F],
                         gossipServiceServer,
                         selectNodes = ns => List(ns(Random.nextInt(ns.length))),
                         minSuccessful = 1,
                         memoizeNodes = false,
                         skipFailedNodesInNextRounds = false,
                         roundPeriod = conf.server.periodicSyncRoundPeriod,
                         connector = connectToGossip
                       )
                     }
      _ <- makeFiberResource {
            awaitApproved >> loop(periodicSync)
          }
    } yield ()
  }

  /** The TransportLayer setup prints the number of peers now and then which integration tests
    * may depend upon. We aren't using the `Connect` functionality so have to do it another way. */
  private def makePeerCountPrinter[F[_]: Concurrent: Time: Log: NodeDiscovery]
      : Resource[F, Unit] = {
    def loop(prevPeers: Set[Node]): F[Unit] = {
      // Based on Connecttions.removeConn
      val newPeers = for {
        peers <- NodeDiscovery[F].recentlyAlivePeersAscendingDistance.map(_.toSet)
        _     <- Log[F].info(s"Peers: ${peers.size}").whenA(peers.size != prevPeers.size)
        _ <- (prevPeers diff peers).toList.traverse { peer =>
              Log[F].info(s"Disconnected from ${peer.show -> "peer"}")
            }
        _ <- (peers diff prevPeers).toList.traverse { peer =>
              Log[F].info(s"Connected to ${peer.show -> "peer"}")
            }
        _ <- Time[F].sleep(15.seconds)
      } yield peers

      Time[F].sleep(5.seconds) *> newPeers >>= (loop(_))
    }

    makeFiberResource(loop(Set.empty)).void
  }

  /** Start something in a fiber. Make sure it stops if the resource is released. */
  private def makeFiberResource[F[_]: Concurrent: Log, A](f: F[A]): Resource[F, F[A]] =
    f.onError {
      case NonFatal(ex) => Log[F].error(s"Fiber resource died: $ex") *> Concurrent[F].raiseError(ex)
      case fatal        => Sync[F].delay(throw fatal)
    }.background

  /** Limit the rate of block downloads per peer. */
  private def makeRateLimiter[F[_]: Concurrent: Timer: Log](
      conf: Configuration
  ): Resource[F, RateLimiter[F, ByteString]] = {
    val elementsPerPeriod: Int = conf.server.blockUploadRateMaxRequests
    val period                 = conf.server.blockUploadRatePeriod
    val queueSize: Int         = conf.server.blockUploadRateMaxThrottled

    if (elementsPerPeriod == 0 || period == Duration.Zero) {
      Resource.liftF {
        for {
          _ <- Log[F].info("Disable rate limiting")
        } yield RateLimiter.noOp[F, ByteString]
      }
    } else {
      Resource.liftF[F, Unit](
        Log[F].info(s"Rate limiting enabled: $elementsPerPeriod requests every $period")
      ) >>
        RateLimiter.create[F, ByteString](
          elementsPerPeriod = elementsPerPeriod,
          period = period,
          maxQueueSize =
            if (queueSize == 0) Int.MaxValue
            else queueSize
        )
    }
  }

  def startGrpcServer[F[_]: Sync: TaskLike: ObservableIterant](
      server: GossipServiceServer[F],
      rateLimiter: RateLimiter[F, ByteString],
      chainId: ByteString,
      serverSslContext: SslContext,
      conf: Configuration,
      port: Int,
      ingressScheduler: Scheduler
  )(implicit m: Metrics[Id], l: Log[Id]): Resource[F, Server] = {
    // Start the gRPC server.
    implicit val s = ingressScheduler
    GrpcServer(
      port,
      services = List(
        (scheduler: Scheduler) =>
          Sync[F].delay {
            val svc = GrpcGossipService.fromGossipService(
              server,
              rateLimiter,
              chainId,
              blockChunkConsumerTimeout = conf.server.relayBlockChunkConsumerTimeout
            )
            GossipingGrpcMonix.bindService(svc, scheduler)
          }
      ),
      interceptors = List(
        new AuthInterceptor(),
        new MetricsInterceptor(),
        ErrorInterceptor.default
      ),
      sslContext = serverSslContext.some,
      maxMessageSize = conf.server.maxMessageSize.some
    )
  }
}

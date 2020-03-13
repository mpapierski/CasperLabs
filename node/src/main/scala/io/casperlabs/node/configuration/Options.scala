package io.casperlabs.node.configuration

/////// WARNING! Do not clean this imports, they needed for @scallop macro
import java.nio.file.Path

import cats._
import cats.implicits._
import com.github.ghik.silencer.silent
import io.casperlabs.comm.discovery.NodeUtils.NodeWithoutChainId
import io.casperlabs.configuration.cli.scallop
import io.casperlabs.node.BuildInfo
import io.casperlabs.node.configuration.Utils._
import izumi.logstage.api.{Log => IzLog}
import org.rogach.scallop._
import scala.collection.mutable
import scala.concurrent.duration.FiniteDuration
import scala.io.Source
import scala.language.implicitConversions

private[configuration] object Converter extends ParserImplicits {
  import Options._

  implicit val bootstrapAddressConverter: ValueConverter[List[NodeWithoutChainId]] =
    new ValueConverter[List[NodeWithoutChainId]] {
      def parse(
          s: List[(String, List[String])]
      ): Either[String, Option[List[NodeWithoutChainId]]] = {
        val all = s.unzip._2.flatten.mkString(" ")
        Parser[List[NodeWithoutChainId]].parse(all).map(Option(_).filterNot(_.isEmpty))
      }

      val argType: ArgType.V = ArgType.LIST
    }

  implicit val optionsFlagConverter: ValueConverter[Flag] = new ValueConverter[Flag] {
    def parse(s: List[(String, List[String])]): Either[String, Option[Flag]] =
      flagConverter.parse(s).map(_.map(flag))

    val argType: ArgType.V = ArgType.FLAG
  }

  implicit val finiteDurationConverter: ValueConverter[FiniteDuration] =
    new ValueConverter[FiniteDuration] {

      override def parse(s: List[(String, List[String])]): Either[String, Option[FiniteDuration]] =
        s match {
          case (_, duration :: Nil) :: Nil =>
            Parser[FiniteDuration].parse(duration).map(_.some)
          case Nil => Right(None)
          case _   => Left("Provide a duration.")
        }

      override val argType: ArgType.V = ArgType.SINGLE
    }

  implicit val logLevelConverter: ValueConverter[IzLog.Level] =
    singleArgConverter(lvl => IzLog.Level.parse(lvl))
}

private[configuration] object Options {
  import shapeless.tag.@@

  sealed trait FlagTag
  type Flag = Boolean @@ FlagTag

  def flag(b: Boolean): Flag = b.asInstanceOf[Flag]

  def safeCreate(args: Seq[String], defaults: Map[CamelCase, String]): Either[String, Options] =
    Either.catchNonFatal(Options(args, defaults)).leftMap(_.getMessage)
}

//noinspection TypeAnnotation
private[configuration] final case class Options private (
    arguments: Seq[String],
    defaults: Map[CamelCase, String]
) extends ScallopConf(arguments) {
  helpWidth(120)
  //Do not clean this imports, they needed for @scallop macro
  import Converter._
  import cats.syntax.show._
  import io.casperlabs.comm.discovery.NodeUtils._
  import io.casperlabs.comm.discovery.NodeUtils
  import Options.Flag

  //Used in @scallop macro when it puts things into `field`
  private implicit def showList[T: Show]: Show[List[T]] = xs => xs.map(_.show).mkString(" ")
  @silent("is never used")
  private implicit val showNodeList = showList[NodeWithoutChainId]
  //Used in @scallop macro
  @silent("is never used")
  private implicit def show[T: NotNode]: Show[T] = Show.show(_.toString)

  //Needed only for eliminating red code from IntelliJ IDEA, see @scallop definition
  @silent("is never used")
  private def gen[A](descr: String, short: Char = '\u0000'): ScallopOption[A] =
    sys.error("Add @scallop macro annotation")

  /**
    * Converts between string representation of field name and its actual value
    * Filled by [[io.casperlabs.configuration.cli.scallop]] macro
    */
  private val fields =
    mutable.Map.empty[(ScallopConfBase, CamelCase), () => ScallopOption[String]]

  def fieldByName(fieldName: CamelCase): Option[String] =
    subcommand
      .flatMap(
        command => fields.get((command, fieldName)).flatMap { _.apply().toOption }
      )

  def parseCommand: Either[String, Configuration.Command] =
    subcommand.fold(s"Command was not provided".asLeft[Configuration.Command]) {
      case this.run         => Configuration.Command.Run.asRight[String]
      case this.diagnostics => Configuration.Command.Run.asRight[String]
      case _                => "Failed to parse command".asLeft[Configuration.Command]
    }

  def readConfigFile: Either[String, Option[String]] =
    configFile.toOption
      .map(
        p => Either.catchNonFatal(Source.fromFile(p.toFile).mkString.some).leftMap(_.getMessage)
      )
      .fold(none[String].asRight[String])(identity)

  val configFile = opt[Path](descr = "Path to the TOML configuration file.")

  version(
    s"CasperLabs Node ${BuildInfo.version} (${BuildInfo.gitHeadCommit.getOrElse("commit # unknown")})"
  )
  printedName = "casperlabs"
  banner(
    """
      |Configuration file --config-file can contain tables
      |[server], [grpc], [casper], [tls], [metrics], [influx] and [blockstorage].
      |
      |CLI options match TOML keys and environment variables, example:
      |    --[prefix]-[key-name]=value e.g. --server-data-dir=/casperlabs
      |
      |    equals
      |
      |    [prefix]            [server]                  CL_[PREFIX]_[SNAKIFIED_KEY]
      |    key-name = "value"  data-dir = "/casperlabs"  CL_SERVER_DATA_DIR=/casperlabs
      |
      |Each option has a type listed in opt's description beginning that should be used in TOML file.
      |
      |CLI arguments will take precedence over environment variables.
      |environment variables will take precedence over TOML file.
    """.stripMargin
  )

  val diagnostics = new Subcommand("diagnostics") {
    helpWidth(120)
    descr("Node diagnostics")

    @scallop
    val grpcPortExternal =
      gen[Int]("Port used for external gRPC API, e.g. deployments.")

    @scallop
    val serverHost =
      gen[String]("Externally addressable hostname or IP of node on which gRPC service is running.")

    @scallop
    val serverMaxMessageSize =
      gen[Int]("Maximum size of message that can be sent via transport layer.")

    @scallop
    val serverChunkSize =
      gen[Int]("Size of chunks to split larger payloads into when streamed via transport layer.")
  }
  addSubcommand(diagnostics)

  val run = new Subcommand("run") {

    helpWidth(120)

    @scallop
    val logLevel =
      gen[IzLog.Level]("Log level, e.g. DEBUG, INFO, WARN, ERROR.")

    @scallop
    val logJsonPath =
      gen[Path]("Optionally print logs to a file in JSON format.")

    @scallop
    val grpcPortExternal =
      gen[Int]("Port used for external gRPC API, e.g. deployments.")

    @scallop
    val grpcUseTls =
      gen[Flag]("Enable TLS encryption for public facing gRPC API endpoints.")

    @scallop
    val serverMaxMessageSize =
      gen[Int]("Maximum size of message that can be sent via transport layer.")

    @scallop
    val serverEngineParallelism =
      gen[Int]("Target parallelism for execution engine requests.")

    @scallop
    val serverChunkSize =
      gen[Int]("Size of chunks to split larger payloads into when streamed via transport layer.")

    @scallop
    val serverEventStreamBufferSize =
      gen[Int]("Size of the buffer to store emitted block events")

    @scallop
    val grpcPortInternal =
      gen[Int]("Port used for internal gRPC API, e.g. diagnostics.")

    @scallop
    val grpcSocket =
      gen[Path]("Socket path used for internal gRPC API.")

    @scallop
    val serverDynamicHostAddress =
      gen[Flag]("Host IP address changes dynamically.")

    @scallop
    val serverNoUpnp =
      gen[Flag]("Use this flag to disable UpNp.")

    @scallop
    val serverDefaultTimeout =
      gen[FiniteDuration](
        "Default timeout for roundtrip connections."
      )

    @scallop
    val tlsCertificate =
      gen[Path](
        "Path to node's X.509 certificate file, that is being used for identification.",
        'c'
      )

    @scallop
    val tlsKey =
      gen[Path](
        "Path to node's unencrypted secp256r1 PKCS#8 private key file, that is being used for TLS communication.",
        'k'
      )

    @scallop
    val tlsApiCertificate =
      gen[Path](
        "Path to an optional X.509 certificate file signed by a trusted root CA, to be used in the with public API."
      )

    @scallop
    val tlsApiKey =
      gen[Path](
        "Path to the unencrypted secp256r1 PKCS#8 private key file corresponding to the API certificate, if given."
      )

    @scallop
    val serverPort =
      gen[Int]("Network port to use for intra-node gRPC communication.", 'p')

    @scallop
    val serverHttpPort =
      gen[Int]("HTTP port for utility services: /metrics, /version and /status.")

    @scallop
    val serverKademliaPort =
      gen[Int](
        "Kademlia port used for node discovery based on Kademlia algorithm."
      )

    @scallop
    val casperKnownValidatorsFile =
      gen[Path](
        "Path to plain text file listing the public keys of validators known to the user (one per line). " +
          "Signatures from these validators are required in order to accept a block which starts the local" +
          s"node's view of the DAG."
      )

    @scallop
    val casperChainSpecPath =
      gen[Path]("Path to the directory which contains the Chain Spec.")

    @scallop
    val casperAutoProposeEnabled =
      gen[Flag]("Enable auto-proposal of blocks.")

    @scallop
    val casperAutoProposeCheckInterval =
      gen[FiniteDuration]("Time between proposal checks.")
    @scallop
    val casperAutoProposeBallotInterval =
      gen[FiniteDuration]("Maximum time to allow before trying to propose a ballot or block.")

    @scallop
    val casperAutoProposeAccInterval =
      gen[FiniteDuration]("Time to accumulate deploys before proposing.")

    @scallop
    val casperAutoProposeAccCount =
      gen[Int]("Number of deploys to accumulate before proposing.")

    @scallop
    val casperMaxBlockSizeBytes =
      gen[Int]("Maximum block size [in bytes].")

    @scallop
    val serverBootstrap =
      gen[List[NodeWithoutChainId]](
        "Bootstrap casperlabs node address for initial seed. Accepts multiple instances for redundancy.",
        'b'
      )

    @scallop
    val serverCleanBlockStorage =
      gen[Flag]("Use this flag to clear the blockStorage and dagStorage")

    @scallop
    val serverRelayFactor =
      gen[Int]("Number of new nodes to which try to gossip a new block.")

    @scallop
    val serverRelaySaturation =
      gen[Int](
        "Percentage (in between 0 and 100) of nodes required to have already seen a new block before stopping to try to gossip it to new nodes."
      )

    @scallop
    val serverApprovalRelayFactor =
      gen[Int]("Number of nodes to relay genesis approvals to.")

    @scallop
    val serverApprovalPollInterval =
      gen[FiniteDuration](
        "Time to wait between asking the bootstrap node for an updated list of genesis approvals."
      )

    @scallop
    val serverAlivePeersCacheExpirationPeriod =
      gen[FiniteDuration](
        "Time to cache live peers for and to ban unresponsive ones."
      )

    @scallop
    val serverSyncMaxPossibleDepth =
      gen[Int]("Maximum DAG depth to allow when syncing after a new block notification.")

    @scallop
    val serverSyncMinBlockCountToCheckWidth =
      gen[Int](
        "Minimum DAG size before we start checking the branching factor for abnormal growth."
      )

    @scallop
    val serverSyncMaxBondingRate =
      gen[Double](
        "Maximum bonding rate per rank to allow during syncs before terminating the operation as malicious."
      )

    @scallop
    val serverSyncMaxDepthAncestorsRequest =
      gen[Int]("Maximum DAG depth to ask in iterative requests during syncing.")

    @scallop
    val serverSyncDisableValidations =
      gen[Flag]("Disable DAG shape validations during synchronization.")

    @scallop
    val serverInitSyncMaxNodes =
      gen[Int]("Maximum number of nodes to try to sync with initially in a round.")

    @scallop
    val serverInitSyncMinSuccessful =
      gen[Int]("Minimum number of successful initial syncs in a round to call it done.")

    @scallop
    val serverInitSyncMemoizeNodes =
      gen[Boolean](
        "Remember the selection of nodes to synchronize with initially, or pick a new set in each round."
      )

    @scallop
    val serverInitSyncSkipFailedNodes =
      gen[Boolean](
        "Skip nodes which failed previous synchronization attempts or allow them to be tried again."
      )

    @scallop
    val serverInitSyncMaxBlockCount =
      gen[Int]("Maximum number of blocks to allow to be synced initially.")

    @scallop
    val serverInitSyncStep =
      gen[Int](
        "Depth of DAG slices (by rank) retrieved slice-by-slice until node fully synchronized."
      )

    @scallop
    val serverInitSyncRoundPeriod =
      gen[FiniteDuration]("Time to wait between initial synchronization attempts.")

    @scallop
    val serverPeriodicSyncRoundPeriod =
      gen[FiniteDuration]("Time to wait between periodic synchronization attempts.")

    @scallop
    val serverDownloadMaxParallelBlocks =
      gen[Int]("Maximum number of parallel block downloads initiated by the download manager.")
    @scallop
    val serverDownloadMaxRetries =
      gen[Int]("Maximum number of times to retry to download a block from the same node.")

    @scallop
    val serverDownloadRetryInitialBackoffPeriod =
      gen[FiniteDuration](
        "Time to wait before trying to download a failed block again from the same node."
      )

    @scallop
    val serverDownloadRetryBackoffFactor =
      gen[Double](
        "Exponential factor to apply on subsequent wait times before trying to download again."
      )

    @scallop
    val serverRelayMaxParallelBlocks =
      gen[Int]("Maximum number of parallel block downloads allowed to peers.")

    @scallop
    val serverRelayBlockChunkConsumerTimeout =
      gen[FiniteDuration]("Maximum time to allow a peer downloading a block to consume each chunk.")

    @scallop
    val casperStandalone =
      gen[Flag](
        "Start a stand-alone node (no bootstrapping).",
        's'
      )

    @scallop
    val casperRequiredSigs =
      gen[Int](
        "Number of signatures from trusted validators required to creating an approved genesis block."
      )

    @scallop
    val serverHost =
      gen[String]("Hostname or IP of this node.")

    @scallop
    val serverDataDir =
      gen[Path]("Path to data directory. ")

    @scallop
    val serverMaxNumOfConnections =
      gen[Int]("Maximum number of peers allowed to connect to the node.")

    @scallop
    val serverBlockUploadRateMaxRequests = gen[Int](
      "Maximum number of block download requests per peer in period (see below), " +
        "if 0 then rate limiting will be disabled."
    )

    @scallop
    val serverBlockUploadRatePeriod = gen[FiniteDuration](
      "Time window to apply rate limiting (see above), " +
        "if 0 then rate limiting will be disabled."
    )

    @scallop
    val serverBlockUploadRateMaxThrottled = gen[Int](
      "Maximum number of in-flight throttled block download requests per peer, " +
        "if 0 then unlimited, if reached max size then peer will receive RESOURCE_EXHAUSTED response."
    )

    @scallop
    val casperMinTtl = gen[FiniteDuration](
      "Minimum deploy TTL value."
    )

    @scallop
    val blockstorageCacheMaxSizeBytes =
      gen[Long]("Maximum size of each of in-memory block/dag/justifications caches in bytes.")

    @scallop
    val blockstorageCacheNeighborhoodBefore =
      gen[Int]("How far to go to the past (by ranks) for caching neighborhood of looked up block")

    @scallop
    val blockstorageCacheNeighborhoodAfter = gen[Int](
      "How far to go to the future (by ranks) for caching neighborhood of looked up block"
    )

    @scallop
    val blockstorageDeployStreamChunkSize = gen[Int](
      "How many records to pull from the DB in a chunk of a stream."
    )

    @scallop
    val casperValidatorPublicKey =
      gen[String](
        "base-64 or PEM encoding of the public key to use for signing a proposed blocks. " +
          s"Can be inferred from the private key for some signature algorithms."
      )

    @scallop
    val casperValidatorPublicKeyPath =
      gen[Path](
        "base-64 or PEM encoding of the public key to use for signing a proposed blocks." +
          s"Can be inferred from the private key for some signature algorithms."
      )

    @scallop
    val casperValidatorPrivateKey =
      gen[String](
        "base-64 or PEM encoding of the private key to use for signing a proposed blocks. " +
          s"It is not recommended to use in production since private key could be revealed through the process table." +
          "Use the `validator-private-key-path` instead."
      )

    @scallop
    val casperValidatorPrivateKeyPath =
      gen[Path](
        "Path to the base-64 or PEM encoded private key to use for signing a proposed blocks."
      )

    @scallop
    val casperValidatorSigAlgorithm =
      gen[String](
        "Name of the algorithm to use for signing proposed blocks. " +
          s"Currently supported values: ed25519."
      )

    @scallop
    val highwayEnabled =
      gen[Flag](
        "Use highway, or stick to NCB."
      )

    @scallop
    val highwayOmegaMessageTimeStart =
      gen[Double](
        "Fraction of time through the round after which we can create an omega message."
      )

    @scallop
    val highwayOmegaMessageTimeEnd =
      gen[Double](
        "Fraction of time through the round before which we must have created the omega message."
      )

    @scallop
    val highwayInitRoundExponent =
      gen[Int](
        "Initial round exponent to start the node with, before auto-adjustment takes over; corresponds to the tick unit of the chain."
      )

    @scallop
    val metricsPrometheus =
      gen[Flag]("Enable the Prometheus metrics reporter.")

    @scallop
    val metricsZipkin =
      gen[Flag]("Enable the Zipkin span reporter.")

    @scallop
    val metricsSigar =
      gen[Flag]("Enable Sigar host system metrics.")

    @scallop
    val metricsInflux =
      gen[Flag]("Enable Influx system metrics.")

    @scallop
    val influxHostname =
      gen[String]("Hostname or IP of the Influx instance.")

    @scallop
    val influxDatabase =
      gen[String]("Name of the database in Influx.")

    @scallop
    val influxPort =
      gen[Int]("Port of the Influx instance.")

    @scallop
    val influxProtocol =
      gen[String]("Protocol used in Influx.")
  }
  addSubcommand(run)

  verify()
}

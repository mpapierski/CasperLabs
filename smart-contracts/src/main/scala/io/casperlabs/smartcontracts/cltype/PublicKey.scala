package io.casperlabs.smartcontracts.cltype

import io.casperlabs.smartcontracts.bytesrepr.{BytesView, FromBytes, ToBytes}

sealed trait PublicKey

object PublicKey {
  case class ED25519(bytes: ByteArray32) extends PublicKey

  object ED25519 {
    val tag: Byte = 0
  }

  implicit val toBytesPublicKey: ToBytes[PublicKey] = new ToBytes[PublicKey] {
    override def toBytes(publicKey: PublicKey): Array[Byte] =
      publicKey match {
        case ED25519(bytes) => ED25519.tag +: ToBytes.toBytes(bytes)
      }
  }

  val deserializer: FromBytes.Deserializer[PublicKey] =
    FromBytes.byte.flatMap {
      case tag if tag == ED25519.tag =>
        for {
          publicKey <- ByteArray32.deserializer
        } yield ED25519(publicKey)
      case other => FromBytes.raise(FromBytes.Error.InvalidVariantTag(other, "PublicKey"))
    }
}

package io.casperlabs.models.cltype

import io.casperlabs.models.bytesrepr.{BytesView, FromBytes, ToBytes}

sealed trait PublicKey

object PublicKey {
  case class ED25519(bytes: ByteArray32) extends PublicKey

  object ED25519 {
    val tag: Byte = 0
  }

  def lt(x: PublicKey, y: PublicKey): Boolean = (x, y) match {
    case (ED25519(x), ED25519(y)) => ByteArray32.lt(x, y)
    case (_, _)                   => false
  }

  implicit val toBytesPublicKey: ToBytes[PublicKey] = new ToBytes[PublicKey] {
    override def toBytes(publicKey: PublicKey): Array[Byte] =
      publicKey match {
        case ED25519(bytes) => ED25519.tag +: ToBytes.toBytes(bytes)
      }
  }

  val deserializer: FromBytes.Deserializer[PublicKey] =
    FromBytes.byte.flatMap {
      case tag if tag == ED25519.tag => ByteArray32.deserializer.map(ED25519.apply)
      case other                     => FromBytes.raise(FromBytes.Error.InvalidVariantTag(other, "PublicKey"))
    }
}

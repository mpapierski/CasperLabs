package io.casperlabs.smartcontracts.cltype

import io.casperlabs.smartcontracts.bytesrepr.{BytesView, FromBytes, ToBytes}

sealed trait PublicKey

object PublicKey {
  case class ED25519(bytes: ByteArray32) extends PublicKey

  implicit val toBytesPublicKey: ToBytes[PublicKey] = new ToBytes[PublicKey] {
    override def toBytes(publicKey: PublicKey): Array[Byte] =
      publicKey match {
        case ED25519(bytes) => ToBytes.toBytes(bytes)
      }
  }

  val deserializer: FromBytes.Deserializer[PublicKey] =
    for {
      publicKey <- ByteArray32.deserializer
    } yield ED25519(publicKey)
}

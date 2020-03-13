package io.casperlabs.models.cltype

import io.casperlabs.models.bytesrepr.SerializationTest.roundTrip
import org.scalacheck.{Arbitrary, Gen}
import org.scalatest.{FlatSpec, Matchers}
import org.scalatest.prop.PropertyChecks
import PublicKeySerializationTest.arbPublicKey

class PublicKeySerializationTest extends FlatSpec with Matchers with PropertyChecks {
  "PublicKeys" should "serialize properly" in forAll { (pk: PublicKey) =>
    roundTrip(pk, PublicKey.deserializer)
  }
}

object PublicKeySerializationTest {
  val genPublicKey: Gen[PublicKey] = for {
    publicKey <- ByteArray32SerializationTest.genByteArray32
  } yield PublicKey.ED25519(publicKey)

  implicit val arbPublicKey: Arbitrary[PublicKey] = Arbitrary(genPublicKey)
}

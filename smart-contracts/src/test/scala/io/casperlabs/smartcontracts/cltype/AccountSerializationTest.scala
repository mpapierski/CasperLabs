package io.casperlabs.smartcontracts.cltype

import io.casperlabs.smartcontracts.bytesrepr.SerializationTest.roundTrip
import org.scalacheck.{Arbitrary, Gen}
import org.scalatest.{FlatSpec, Matchers}
import org.scalatest.prop.PropertyChecks
import AccountSerializationTest.arbAccount

class AccountSerializationTest extends FlatSpec with Matchers with PropertyChecks {
  "Accounts" should "serialize properly" in forAll { (a: Account) =>
    roundTrip(a, Account.deserializer)
  }
}

object AccountSerializationTest {
  private val genWeight = Gen.choose[Byte](-128, 127)

  val genAccount: Gen[Account] = for {
    publicKey <- ByteArray32SerializationTest.genByteArray32
    namedKeys <- Gen.mapOf(
                  Gen.alphaStr.flatMap(s => KeySerializationTest.genKey.map(k => s -> k))
                )
    mainPurse <- URefSerializationTest.genURef
    associatedKeys <- Gen.mapOf(
                       ByteArray32SerializationTest.genByteArray32.flatMap(
                         k => genWeight.map(w => k -> w)
                       )
                     )
    actionThresholds <- genWeight.flatMap { d =>
                         genWeight.map(k => Account.ActionThresholds(d, k))
                       }
  } yield Account(
    PublicKey.ED25519(publicKey),
    namedKeys,
    mainPurse,
    associatedKeys.map({
      case (k, v) => (PublicKey.ED25519(k), v)
    }),
    actionThresholds
  )

  implicit val arbAccount: Arbitrary[Account] = Arbitrary(genAccount)
}

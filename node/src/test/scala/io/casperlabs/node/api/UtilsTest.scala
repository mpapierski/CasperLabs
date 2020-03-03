package io.casperlabs.node.api

import io.casperlabs.crypto.codec.Base16
import io.casperlabs.casper.consensus.state
import monix.eval.Task
import monix.execution.Scheduler.Implicits.global
import org.scalatest.{EitherValues, FlatSpec, Matchers}

class UtilsTest extends FlatSpec with EitherValues with Matchers {

  def attemptToKeyTest(
      nBytes: Int,
      keyType: String,
      typeTest: state.Key.Value => Boolean,
      bytesExtract: state.Key.Value => Array[Byte]
  ) = {
    val keyValue = randomBytes(nBytes)

    val maybeKey = attemptToKey(keyType, keyValue)
    maybeKey.isRight shouldBe true

    val state.Key(key) = maybeKey.right.get
    typeTest(key) shouldBe true

    val bytes = bytesExtract(key)
    Base16.encode(bytes) shouldBe keyValue
  }

  "toKey" should "convert a hash-type key successfully" in {
    attemptToKeyTest(32, "hash", _.isHash, _.hash.get.hash.toByteArray)
  }

  it should "convert a uref-type key successfully" in {
    attemptToKeyTest(32, "uref", _.isUref, _.uref.get.uref.toByteArray)
  }

  it should "convert an address-type key successfully" in {
    attemptToKeyTest(32, "address", _.isAddress, _.address.get.account.toByteArray)
  }

  it should "fail for any invalid key type" in {
    val keyValue = randomBytes(32)
    val keyType  = util.Random.alphanumeric.take(10).mkString

    val maybeKey = attemptToKey(keyType, keyValue)
    maybeKey.isLeft shouldBe true
  }

  it should "fail if the wrong number of bytes is given for the key type" in {
    val a = util.Random.nextInt(50) + 33 //number > 32
    val c = util.Random.nextInt(11) + 21 //number > 20 and < 32
    val d = 20
    val e = util.Random.nextInt(20) //number < 20

    attemptToKey("hash", randomBytes(a)) shouldBe ('left)
    attemptToKey("uref", randomBytes(a)) shouldBe ('left)
    attemptToKey("address", randomBytes(a)) shouldBe ('left)

    attemptToKey("hash", randomBytes(c)) shouldBe ('left)
    attemptToKey("uref", randomBytes(c)) shouldBe ('left)
    attemptToKey("address", randomBytes(c)) shouldBe ('left)

    attemptToKey("hash", randomBytes(d)) shouldBe ('left)
    attemptToKey("uref", randomBytes(d)) shouldBe ('left)
    attemptToKey("address", randomBytes(d)) shouldBe ('left)

    attemptToKey("hash", randomBytes(e)) shouldBe ('left)
    attemptToKey("uref", randomBytes(e)) shouldBe ('left)
    attemptToKey("address", randomBytes(e)) shouldBe ('left)
  }

  private def randomBytes(length: Int): String =
    Base16.encode(Array.fill(length)(util.Random.nextInt(256).toByte))

  private def attemptToKey(keyType: String, keyValue: String): Either[Throwable, state.Key] =
    Utils.toKey[Task](keyType, keyValue).attempt.runSyncUnsafe()
}

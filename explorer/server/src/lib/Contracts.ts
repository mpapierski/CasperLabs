import { Deploy } from "casperlabs-grpc/io/casperlabs/casper/consensus/consensus_pb";
import { Args, ByteArray } from "casperlabs-sdk";

const Ed25519Tag: number = 0;

function toBytesPublicKey(publicKey: ByteArray): ByteArray {
  const data = new Uint8Array(publicKey.length + 1);
  data.set([Ed25519Tag]);
  data.set(publicKey, 1);
  return data;
}

export class Faucet {
  public static args(accountPublicKey: ByteArray, amount: bigint): Deploy.Arg[] {
    return Args.Args(
      ["account", Args.BytesValue(toBytesPublicKey(accountPublicKey))],
      ["amount", Args.BigIntValue(amount)]
    );
  }
}

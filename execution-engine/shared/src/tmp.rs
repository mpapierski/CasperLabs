#[cfg(test)]
mod test {
    use common::bytesrepr::{deserialize, ToBytes};
    use common::value::account::PublicKey;

    #[test]
    fn serialize_public_key() {
        let pk = PublicKey::new([0u8; 32]);
        let serialized = pk.to_bytes().expect("should serialize");
        let pk_ret: PublicKey = deserialize(&serialized).expect("should deserialize");
        assert_eq!(pk, pk_ret);
    }
}

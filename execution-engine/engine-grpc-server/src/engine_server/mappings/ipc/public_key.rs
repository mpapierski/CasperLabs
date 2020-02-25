use std::convert::TryFrom;

use types::account::PublicKey;

use crate::engine_server::{ipc, mappings::MappingError};

impl From<PublicKey> for ipc::PublicKey {
    fn from(public_key: PublicKey) -> Self {
        let raw_bytes = {
            let PublicKey::Ed25519(ed25519) = public_key;
            ed25519.value().to_vec()
        };

        let mut result = ipc::PublicKey::new();
        let mut ed25519 = ipc::Ed25519::new();
        ed25519.set_public_key(raw_bytes);
        result.set_ed25519(ed25519);
        result
    }
}

impl TryFrom<ipc::PublicKey> for PublicKey {
    type Error = MappingError;
    fn try_from(public_key: ipc::PublicKey) -> Result<Self, Self::Error> {
        if !public_key.has_ed25519() {
            return Err(MappingError::MissingPayload);
        }
        let ed25519 = public_key.get_ed25519();
        let raw_bytes = ed25519.get_public_key();
        PublicKey::ed25519_try_from(&raw_bytes)
            .map_err(|_| MappingError::invalid_public_key_length(raw_bytes.len()))
    }
}

#[cfg(test)]
mod tests {
    use proptest::proptest;

    use types::gens;

    use super::*;
    use crate::engine_server::mappings::test_utils;

    #[test]
    fn public_key_from_missing_ed25519() {
        let mut result = ipc::PublicKey::new();
        result.set_ed25519(ipc::Ed25519::new());
        assert!(PublicKey::try_from(result).is_err());
    }

    #[test]
    fn public_key_from_invalid_ed25519_bytes() {
        let mut result = ipc::PublicKey::new();
        let mut ed25519 = ipc::Ed25519::new();
        ed25519.set_public_key((0u8..255).collect());
        result.set_ed25519(ed25519);
        assert!(PublicKey::try_from(result).is_err());
    }

    proptest! {
        #[test]
        fn round_trip(pk in gens::public_key_arb()) {
            test_utils::protobuf_round_trip::<PublicKey, ipc::PublicKey>(pk);
        }
    }
}

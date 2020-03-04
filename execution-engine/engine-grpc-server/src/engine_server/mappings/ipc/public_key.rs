use std::convert::TryFrom;

use types::account::PublicKey;

use crate::engine_server::{
    mappings::{MappingError, ParsingError},
    state,
};

impl From<PublicKey> for state::PublicKey {
    fn from(public_key: PublicKey) -> Self {
        let raw_bytes = {
            let PublicKey::Ed25519(ed25519) = public_key;
            ed25519.value().to_vec()
        };

        let mut result = state::PublicKey::new();
        result.mut_ed25519().set_public_key(raw_bytes);
        result
    }
}

impl TryFrom<state::PublicKey> for PublicKey {
    type Error = MappingError;

    fn try_from(pb_public_key: state::PublicKey) -> Result<Self, Self::Error> {
        let pb_public_key = pb_public_key
            .variant
            .ok_or_else(|| ParsingError("Unable to parse Protobuf PublicKey".to_string()))?;
        let public_key = match pb_public_key {
            state::PublicKey_oneof_variant::ed25519(ed25519) => {
                let raw_bytes = ed25519.get_public_key();
                PublicKey::ed25519_try_from(&raw_bytes)
                    .map_err(|_| MappingError::invalid_public_key_length(raw_bytes.len()))?
            }
        };
        Ok(public_key)
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
        let mut result = state::PublicKey::new();
        result.set_ed25519(state::Ed25519::new());
        assert!(PublicKey::try_from(result).is_err());
    }

    #[test]
    fn public_key_from_invalid_ed25519_bytes() {
        let mut result = state::PublicKey::new();
        result.mut_ed25519().set_public_key((0u8..255).collect());
        assert!(PublicKey::try_from(result).is_err());
    }

    proptest! {
        #[test]
        fn round_trip(pk in gens::public_key_arb()) {
            test_utils::protobuf_round_trip::<PublicKey, state::PublicKey>(pk);
        }
    }
}

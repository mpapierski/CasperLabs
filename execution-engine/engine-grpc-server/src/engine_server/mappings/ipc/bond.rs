use std::convert::{TryFrom, TryInto};

use types::{account::PublicKey, U512};

use crate::engine_server::{ipc, mappings::MappingError};

impl From<(PublicKey, U512)> for ipc::Bond {
    fn from((key, amount): (PublicKey, U512)) -> Self {
        let mut pb_bond = ipc::Bond::new();
        pb_bond.set_validator_public_key(key.as_bytes().to_vec());
        pb_bond.set_stake(amount.into());
        pb_bond
    }
}

impl TryFrom<ipc::Bond> for (PublicKey, U512) {
    type Error = MappingError;

    fn try_from(mut pb_bond: ipc::Bond) -> Result<Self, Self::Error> {
        let public_key_bytes = pb_bond.take_validator_public_key();
        let public_key = PublicKey::ed25519_try_from(&public_key_bytes)
            .map_err(|_| MappingError::invalid_public_key_length(public_key_bytes.len()))?;
        let stake = pb_bond.take_stake().try_into()?;
        Ok((public_key, stake))
    }
}

#[cfg(test)]
mod tests {
    use proptest::proptest;

    use types::gens;

    use super::*;
    use crate::engine_server::mappings::test_utils;

    proptest! {
        #[test]
        fn round_trip(public_key in gens::public_key_arb(), u512 in gens::u512_arb()) {
            test_utils::protobuf_round_trip::<(PublicKey, U512), ipc::Bond>((public_key, u512));
        }
    }
}

use std::convert::{TryFrom, TryInto};

use engine_core::engine_state::genesis::GenesisAccount;
use engine_shared::motes::Motes;
use types::account::PublicKey;

use crate::engine_server::{ipc::ChainSpec_GenesisAccount, mappings::MappingError};

impl From<GenesisAccount> for ChainSpec_GenesisAccount {
    fn from(genesis_account: GenesisAccount) -> Self {
        let mut pb_genesis_account = ChainSpec_GenesisAccount::new();

        pb_genesis_account.set_public_key(genesis_account.public_key().into());
        pb_genesis_account.set_balance(genesis_account.balance().value().into());
        pb_genesis_account.set_bonded_amount(genesis_account.bonded_amount().value().into());

        pb_genesis_account
    }
}

impl TryFrom<ChainSpec_GenesisAccount> for GenesisAccount {
    type Error = MappingError;

    fn try_from(mut pb_genesis_account: ChainSpec_GenesisAccount) -> Result<Self, Self::Error> {
        // TODO: our TryFromSliceForPublicKeyError should convey length info
        let public_key = PublicKey::try_from(pb_genesis_account.take_public_key())?;
        let balance = pb_genesis_account
            .take_balance()
            .try_into()
            .map(Motes::new)?;
        let bonded_amount = pb_genesis_account
            .take_bonded_amount()
            .try_into()
            .map(Motes::new)?;
        Ok(GenesisAccount::new(public_key, balance, bonded_amount))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_server::mappings::test_utils;

    #[test]
    fn round_trip() {
        let genesis_account = rand::random();
        test_utils::protobuf_round_trip::<GenesisAccount, ChainSpec_GenesisAccount>(
            genesis_account,
        );
    }
}

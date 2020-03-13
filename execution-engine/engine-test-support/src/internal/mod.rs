mod additive_map_diff;
mod deploy_item_builder;
pub mod exec_with_return;
mod execute_request_builder;
mod upgrade_request_builder;
pub mod utils;
mod wasm_test_builder;

use lazy_static::lazy_static;
use num_traits::identities::Zero;

use engine_core::engine_state::genesis::{GenesisAccount, GenesisConfig};
use engine_shared::{motes::Motes, test_utils};
use engine_wasm_prep::wasm_costs::WasmCosts;
use types::{account::PublicKey, ProtocolVersion, U512};

use super::{DEFAULT_ACCOUNT_ADDR, DEFAULT_ACCOUNT_INITIAL_BALANCE};
pub use additive_map_diff::AdditiveMapDiff;
pub use deploy_item_builder::DeployItemBuilder;
pub use execute_request_builder::ExecuteRequestBuilder;
pub use upgrade_request_builder::UpgradeRequestBuilder;
pub use wasm_test_builder::{
    InMemoryWasmTestBuilder, LmdbWasmTestBuilder, WasmTestBuilder, WasmTestResult,
};

pub const MINT_INSTALL_CONTRACT: &str = "mint_install.wasm";
pub const POS_INSTALL_CONTRACT: &str = "pos_install.wasm";
pub const STANDARD_PAYMENT_INSTALL_CONTRACT: &str = "standard_payment_install.wasm";
pub const STANDARD_PAYMENT_CONTRACT: &str = "standard_payment.wasm";

pub const DEFAULT_CHAIN_NAME: &str = "gerald";
pub const DEFAULT_GENESIS_TIMESTAMP: u64 = 0;
pub const DEFAULT_BLOCK_TIME: u64 = 0;
pub const MOCKED_ACCOUNT_ADDRESS: PublicKey = PublicKey::ed25519_from([48u8; 32]);

pub const DEFAULT_ACCOUNT_KEY: PublicKey = DEFAULT_ACCOUNT_ADDR;

lazy_static! {
    pub static ref DEFAULT_ACCOUNTS: Vec<GenesisAccount> = {
        let mut ret = Vec::new();
        let genesis_account = GenesisAccount::new(
            DEFAULT_ACCOUNT_ADDR,
            Motes::new(DEFAULT_ACCOUNT_INITIAL_BALANCE.into()),
            Motes::zero(),
        );
        ret.push(genesis_account);
        ret
    };
    pub static ref DEFAULT_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::V1_0_0;
    pub static ref DEFAULT_PAYMENT: U512 = 100_000_000.into();
    pub static ref DEFAULT_WASM_COSTS: WasmCosts = test_utils::wasm_costs_mock();
    pub static ref DEFAULT_GENESIS_CONFIG: GenesisConfig = {
        let mint_installer_bytes;
        let pos_installer_bytes;
        let standard_payment_installer_bytes;
        if cfg!(feature = "use-system-contracts") {
            mint_installer_bytes = utils::read_wasm_file_bytes(MINT_INSTALL_CONTRACT);
            pos_installer_bytes = utils::read_wasm_file_bytes(POS_INSTALL_CONTRACT);
            standard_payment_installer_bytes =
                utils::read_wasm_file_bytes(STANDARD_PAYMENT_INSTALL_CONTRACT);
        } else {
            mint_installer_bytes = Vec::new();
            pos_installer_bytes = Vec::new();
            standard_payment_installer_bytes = Vec::new();
        };

        GenesisConfig::new(
            DEFAULT_CHAIN_NAME.to_string(),
            DEFAULT_GENESIS_TIMESTAMP,
            *DEFAULT_PROTOCOL_VERSION,
            mint_installer_bytes,
            pos_installer_bytes,
            standard_payment_installer_bytes,
            DEFAULT_ACCOUNTS.clone(),
            *DEFAULT_WASM_COSTS,
        )
    };
}

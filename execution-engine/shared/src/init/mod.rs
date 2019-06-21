use std::collections::btree_map::BTreeMap;

use common::key::Key;
use common::uref::{AccessRights, URef};
use common::value::account::{
    AccountActivity, ActionThresholds, AssociatedKeys, BlockTime, PublicKey, PurseId, Weight,
};
use common::value::{Account, Value};

const DEFAULT_CURRENT_BLOCK_TIME: BlockTime = BlockTime(0);
const DEFAULT_INACTIVITY_PERIOD_TIME: BlockTime = BlockTime(100);
const GENESIS_NONCE: u64 = 0;

pub fn create_genesis_account(
    account_addr: [u8; 32],
    purse_id: PurseId,
    known_urefs: BTreeMap<String, Key>,
) -> Account {
    create_account(account_addr, GENESIS_NONCE, known_urefs, purse_id)
}

pub fn create_account(
    account_addr: [u8; 32],
    nonce: u64,
    known_urefs: BTreeMap<String, Key>,
    purse_id: PurseId,
) -> Account {
    let associated_keys = AssociatedKeys::new(PublicKey::new(account_addr), Weight::new(1));
    let action_thresholds: ActionThresholds = Default::default();
    let account_activity =
        AccountActivity::new(DEFAULT_CURRENT_BLOCK_TIME, DEFAULT_INACTIVITY_PERIOD_TIME);
    Account::new(
        account_addr,
        nonce,
        known_urefs,
        purse_id,
        associated_keys,
        action_thresholds,
        account_activity,
    )
}

pub fn mocked_account(account_addr: [u8; 32]) -> Vec<(Key, Value)> {
    let purse_id = PurseId::new(URef::new([0u8; 32], AccessRights::READ_ADD_WRITE));
    let account = create_genesis_account(account_addr, purse_id, BTreeMap::new());
    vec![(Key::Account(account_addr), Value::Account(account))]
}

use contract_ffi::{
    key::Key,
    value::{account::PurseId, U512},
};
use engine_core::engine_state::genesis::POS_REWARDS_PURSE;

use crate::{
    support::test_support::{ExecuteRequestBuilder, InMemoryWasmTestBuilder},
    test::{DEFAULT_ACCOUNT_ADDR, DEFAULT_GENESIS_CONFIG},
};

const CONTRACT_TRANSFER: &str = "transfer_purse_to_account.wasm";
const ACCOUNT_ADDR_1: [u8; 32] = [1u8; 32];

fn get_pos_purse_id_by_name(
    builder: &InMemoryWasmTestBuilder,
    purse_name: &str,
) -> Option<PurseId> {
    let pos_contract = builder.get_pos_contract();

    pos_contract
        .named_keys()
        .get(purse_name)
        .and_then(Key::as_uref)
        .map(|u| PurseId::new(*u))
}

#[test]
fn should_not_be_able_to_unbond_reward() {
    let mut builder = InMemoryWasmTestBuilder::default();

    builder.run_genesis(&DEFAULT_GENESIS_CONFIG);

    // First request to put some funds in the reward purse

    let exec_request_1 = ExecuteRequestBuilder::standard(
        DEFAULT_ACCOUNT_ADDR,
        CONTRACT_TRANSFER,
        (ACCOUNT_ADDR_1, U512::from(100)),
    )
    .build();

    builder.exec(exec_request_1).expect_success().commit();

    let rewards_purse = get_pos_purse_id_by_name(&builder, POS_REWARDS_PURSE).unwrap();

    let exec_request_2 = ExecuteRequestBuilder::standard(
        DEFAULT_ACCOUNT_ADDR,
        "ee_803_regression.wasm",
        ("bond", rewards_purse),
    )
    .build();

    builder.exec(exec_request_2).expect_success().commit();

    let exec_request_3 = ExecuteRequestBuilder::standard(
        DEFAULT_ACCOUNT_ADDR,
        "ee_803_regression.wasm",
        ("unbond",),
    )
    .build();

    builder.exec(exec_request_3).expect_success().commit();
}

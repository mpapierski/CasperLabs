use num_traits::cast::AsPrimitive;

use engine_core::engine_state::CONV_RATE;
use engine_test_support::{
    internal::{
        ExecuteRequestBuilder, InMemoryWasmTestBuilder, DEFAULT_GENESIS_CONFIG, DEFAULT_PAYMENT,
    },
    DEFAULT_ACCOUNT_ADDR,
};
use types::U512;

#[ignore]
#[test]
fn should_charge_gas_for_subcall() {
    const CONTRACT_NAME: &str = "measure_gas_subcall.wasm";
    const DO_NOTHING: &str = "do-nothing";
    const DO_SOMETHING: &str = "do-something";
    const NO_SUBCALL: &str = "no-subcall";

    let do_nothing_request =
        ExecuteRequestBuilder::standard(DEFAULT_ACCOUNT_ADDR, CONTRACT_NAME, (DO_NOTHING,)).build();

    let do_something_request =
        ExecuteRequestBuilder::standard(DEFAULT_ACCOUNT_ADDR, CONTRACT_NAME, (DO_SOMETHING,))
            .build();

    let no_subcall_request =
        ExecuteRequestBuilder::standard(DEFAULT_ACCOUNT_ADDR, CONTRACT_NAME, (NO_SUBCALL,)).build();

    let mut builder = InMemoryWasmTestBuilder::default();

    builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(do_nothing_request)
        .expect_success()
        .commit()
        .exec(do_something_request)
        .expect_success()
        .commit()
        .exec(no_subcall_request)
        .expect_success()
        .commit()
        .finish();

    let do_nothing_cost = builder.exec_costs(0)[0];

    let do_something_cost = builder.exec_costs(1)[0];

    let no_subcall_cost = builder.exec_costs(2)[0];

    assert_ne!(
        do_nothing_cost, do_something_cost,
        "should have different costs"
    );

    assert_ne!(
        no_subcall_cost, do_something_cost,
        "should have different costs"
    );

    assert!(
        do_nothing_cost < do_something_cost,
        "should cost more to do something via subcall"
    );

    assert!(
        no_subcall_cost < do_nothing_cost,
        "do nothing in a subcall should cost more than no subcall"
    );
}

#[ignore]
#[test]
fn should_add_all_gas_for_subcall() {
    const CONTRACT_NAME: &str = "add_gas_subcall.wasm";
    const ADD_GAS_FROM_SESSION: &str = "add-gas-from-session";
    const ADD_GAS_VIA_SUBCALL: &str = "add-gas-via-subcall";

    // Use 90% of the standard test contract's balance
    let gas_to_add: U512 = *DEFAULT_PAYMENT / CONV_RATE * 9 / 10;

    assert!(gas_to_add <= U512::from(i32::max_value()));
    let gas_to_add_as_arg: i32 = gas_to_add.as_();

    let add_zero_gas_from_session_request = ExecuteRequestBuilder::standard(
        DEFAULT_ACCOUNT_ADDR,
        CONTRACT_NAME,
        (0, ADD_GAS_FROM_SESSION),
    )
    .build();

    let add_some_gas_from_session_request = ExecuteRequestBuilder::standard(
        DEFAULT_ACCOUNT_ADDR,
        CONTRACT_NAME,
        (gas_to_add_as_arg, ADD_GAS_FROM_SESSION),
    )
    .build();

    let add_zero_gas_via_subcall_request = ExecuteRequestBuilder::standard(
        DEFAULT_ACCOUNT_ADDR,
        CONTRACT_NAME,
        (0, ADD_GAS_VIA_SUBCALL),
    )
    .build();

    let add_some_gas_via_subcall_request = ExecuteRequestBuilder::standard(
        DEFAULT_ACCOUNT_ADDR,
        CONTRACT_NAME,
        (gas_to_add_as_arg, ADD_GAS_VIA_SUBCALL),
    )
    .build();

    let mut builder = InMemoryWasmTestBuilder::default();

    builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(add_zero_gas_from_session_request)
        .expect_success()
        .commit()
        .exec(add_some_gas_from_session_request)
        .expect_success()
        .commit()
        .exec(add_zero_gas_via_subcall_request)
        .expect_success()
        .commit()
        .exec(add_some_gas_via_subcall_request)
        .expect_success()
        .commit()
        .finish();

    let add_zero_gas_from_session_cost = builder.exec_costs(0)[0];
    let add_some_gas_from_session_cost = builder.exec_costs(1)[0];
    let add_zero_gas_via_subcall_cost = builder.exec_costs(2)[0];
    let add_some_gas_via_subcall_cost = builder.exec_costs(3)[0];

    assert!(add_zero_gas_from_session_cost.value() < gas_to_add);
    assert!(add_some_gas_from_session_cost.value() > gas_to_add);
    assert_eq!(
        add_some_gas_from_session_cost.value(),
        gas_to_add + add_zero_gas_from_session_cost.value()
    );

    assert!(add_zero_gas_via_subcall_cost.value() < gas_to_add);
    assert!(add_some_gas_via_subcall_cost.value() > gas_to_add);
    assert_eq!(
        add_some_gas_via_subcall_cost.value(),
        gas_to_add + add_zero_gas_via_subcall_cost.value()
    );
}

#[ignore]
#[test]
fn expensive_subcall_should_cost_more() {
    const CONTRACT_NAME: &str = "measure_gas_subcall_2.wasm";
    const DO_NOTHING: &str = "do_nothing_stored.wasm";
    const EXPENSIVE_CALCULATION: &str = "expensive_calculation.wasm";
    const DO_NOTHING_KEY: &str = "do_nothing_stored";
    const EXPENSIVE_CALCULATION_KEY: &str = "expensive-calculation";

    let store_do_nothing_request =
        ExecuteRequestBuilder::standard(DEFAULT_ACCOUNT_ADDR, DO_NOTHING, ("hash",)).build();

    let store_calculation_request =
        ExecuteRequestBuilder::standard(DEFAULT_ACCOUNT_ADDR, EXPENSIVE_CALCULATION, ()).build();

    let mut builder = InMemoryWasmTestBuilder::default();

    // store the contracts first
    builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(store_do_nothing_request)
        .expect_success()
        .commit()
        .exec(store_calculation_request)
        .expect_success()
        .commit()
        .finish();

    let account = builder
        .get_account(DEFAULT_ACCOUNT_ADDR)
        .expect("should get account");

    // get the contract hashes so that we can pass them to the caller contract
    let do_nothing_contract_hash = account
        .named_keys()
        .get(DO_NOTHING_KEY)
        .expect("should get do_nothing contract hash")
        .into_hash()
        .expect("should get hash");

    let expensive_calculation_contract_hash = account
        .named_keys()
        .get(EXPENSIVE_CALCULATION_KEY)
        .expect("should get expensive_calculation contract hash")
        .into_hash()
        .expect("should get hash");

    // execute the contracts via subcalls

    let call_do_nothing_request = ExecuteRequestBuilder::standard(
        DEFAULT_ACCOUNT_ADDR,
        CONTRACT_NAME,
        (do_nothing_contract_hash,),
    )
    .build();

    let call_expensive_calculation_request = ExecuteRequestBuilder::standard(
        DEFAULT_ACCOUNT_ADDR,
        CONTRACT_NAME,
        (expensive_calculation_contract_hash,),
    )
    .build();

    builder
        .exec(call_do_nothing_request)
        .expect_success()
        .commit()
        .exec(call_expensive_calculation_request)
        .expect_success()
        .commit()
        .finish();

    let do_nothing_cost = builder.exec_costs(2)[0];

    let expensive_calculation_cost = builder.exec_costs(3)[0];

    assert!(
        do_nothing_cost < expensive_calculation_cost,
        "calculation cost should be higher than doing nothing cost"
    );
}

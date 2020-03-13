use engine_core::engine_state::{genesis::POS_REWARDS_PURSE, CONV_RATE, MAX_PAYMENT};
use engine_shared::{motes::Motes, transform::Transform};
use engine_test_support::{
    internal::{
        utils, DeployItemBuilder, ExecuteRequestBuilder, InMemoryWasmTestBuilder,
        DEFAULT_ACCOUNT_KEY, DEFAULT_GENESIS_CONFIG,
    },
    DEFAULT_ACCOUNT_ADDR, DEFAULT_ACCOUNT_INITIAL_BALANCE,
};
use types::{account::PublicKey, Key, URef, U512};

const ACCOUNT_1_ADDR: PublicKey = PublicKey::ed25519_from([42u8; 32]);
const DO_NOTHING_WASM: &str = "do_nothing.wasm";
const TRANSFER_PURSE_TO_ACCOUNT_WASM: &str = "transfer_purse_to_account.wasm";
const REVERT_WASM: &str = "revert.wasm";
const ENDLESS_LOOP_WASM: &str = "endless_loop.wasm";

#[ignore]
#[test]
fn should_raise_insufficient_payment_when_caller_lacks_minimum_balance() {
    let account_1_public_key = ACCOUNT_1_ADDR;

    let exec_request = ExecuteRequestBuilder::standard(
        DEFAULT_ACCOUNT_ADDR,
        TRANSFER_PURSE_TO_ACCOUNT_WASM,
        (account_1_public_key, U512::from(MAX_PAYMENT - 1)),
    )
    .build();

    let mut builder = InMemoryWasmTestBuilder::default();

    let _response = builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(exec_request)
        .expect_success()
        .commit()
        .get_exec_response(0)
        .expect("there should be a response")
        .to_owned();

    let account_1_request =
        ExecuteRequestBuilder::standard(ACCOUNT_1_ADDR, REVERT_WASM, ()).build();

    let account_1_response = builder
        .exec(account_1_request)
        .commit()
        .get_exec_response(1)
        .expect("there should be a response");

    let error_message = utils::get_error_message(account_1_response);

    assert!(
        error_message.contains("InsufficientPayment"),
        "expected insufficient payment, got: {}",
        error_message
    );

    let expected_transfers_count = 0;
    let transforms = builder.get_transforms();
    let transform = &transforms[1];

    assert_eq!(
        transform.len(),
        expected_transfers_count,
        "there should be no transforms if the account main purse has less than max payment"
    );
}

#[cfg(feature = "use-system-contracts")]
#[ignore]
#[test]
fn should_raise_insufficient_payment_when_payment_code_does_not_pay_enough() {
    let account_1_public_key = ACCOUNT_1_ADDR;

    let exec_request = {
        let deploy = DeployItemBuilder::new()
            .with_address(DEFAULT_ACCOUNT_ADDR)
            .with_deploy_hash([1; 32])
            .with_session_code(
                TRANSFER_PURSE_TO_ACCOUNT_WASM,
                (account_1_public_key, U512::from(1)),
            )
            .with_empty_payment_bytes((U512::from(1),))
            .with_authorization_keys(&[DEFAULT_ACCOUNT_KEY])
            .build();

        ExecuteRequestBuilder::new().push_deploy(deploy).build()
    };

    let mut builder = InMemoryWasmTestBuilder::default();

    builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(exec_request)
        .commit();

    let modified_balance = builder.get_purse_balance(
        builder
            .get_account(DEFAULT_ACCOUNT_ADDR)
            .expect("should have account")
            .main_purse(),
    );
    let reward_balance = get_pos_rewards_purse_balance(&builder);

    let initial_balance: U512 = U512::from(DEFAULT_ACCOUNT_INITIAL_BALANCE);
    let expected_reward_balance: U512 = U512::from(MAX_PAYMENT);

    assert_eq!(
        modified_balance,
        initial_balance - expected_reward_balance,
        "modified balance is incorrect"
    );

    assert_eq!(
        reward_balance, expected_reward_balance,
        "reward balance is incorrect"
    );

    assert_eq!(
        initial_balance,
        (modified_balance + reward_balance),
        "no net resources should be gained or lost post-distribution"
    );

    let response = builder
        .get_exec_response(0)
        .expect("there should be a response");

    let execution_result = utils::get_success_result(response);
    let error_message = format!("{}", execution_result.error().expect("should have error"));

    assert_eq!(
        error_message, "Insufficient payment",
        "expected insufficient payment"
    );
}

#[cfg(feature = "use-system-contracts")]
#[ignore]
#[test]
fn should_raise_insufficient_payment_error_when_out_of_gas() {
    let account_1_public_key = ACCOUNT_1_ADDR;
    let payment_purse_amount: U512 = U512::from(1);
    let transferred_amount = U512::from(1);

    let exec_request = {
        let deploy = DeployItemBuilder::new()
            .with_address(DEFAULT_ACCOUNT_ADDR)
            .with_deploy_hash([1; 32])
            .with_empty_payment_bytes((payment_purse_amount,))
            .with_session_code(
                TRANSFER_PURSE_TO_ACCOUNT_WASM,
                (account_1_public_key, transferred_amount),
            )
            .with_authorization_keys(&[DEFAULT_ACCOUNT_KEY])
            .build();

        ExecuteRequestBuilder::new().push_deploy(deploy).build()
    };

    let mut builder = InMemoryWasmTestBuilder::default();

    builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(exec_request)
        .commit()
        .finish();

    let initial_balance: U512 = U512::from(DEFAULT_ACCOUNT_INITIAL_BALANCE);
    let expected_reward_balance: U512 = U512::from(MAX_PAYMENT);

    let modified_balance = builder.get_purse_balance(
        builder
            .get_account(DEFAULT_ACCOUNT_ADDR)
            .expect("should have account")
            .main_purse(),
    );
    let reward_balance = get_pos_rewards_purse_balance(&builder);

    assert_eq!(
        modified_balance,
        initial_balance - expected_reward_balance,
        "modified balance is incorrect"
    );

    assert_eq!(
        reward_balance, expected_reward_balance,
        "reward balance is incorrect"
    );

    assert_eq!(
        initial_balance,
        (modified_balance + reward_balance),
        "no net resources should be gained or lost post-distribution"
    );

    let response = builder
        .get_exec_response(0)
        .expect("there should be a response");

    let execution_result = utils::get_success_result(response);
    let error_message = format!("{}", execution_result.error().expect("should have error"));

    assert_eq!(
        error_message, "Insufficient payment",
        "expected insufficient payment"
    );
}

#[ignore]
#[test]
fn should_forward_payment_execution_runtime_error() {
    let account_1_public_key = ACCOUNT_1_ADDR;
    let transferred_amount = U512::from(1);

    let exec_request = {
        let deploy = DeployItemBuilder::new()
            .with_address(DEFAULT_ACCOUNT_ADDR)
            .with_deploy_hash([1; 32])
            .with_payment_code(REVERT_WASM, ())
            .with_session_code(
                TRANSFER_PURSE_TO_ACCOUNT_WASM,
                (account_1_public_key, transferred_amount),
            )
            .with_authorization_keys(&[DEFAULT_ACCOUNT_KEY])
            .build();

        ExecuteRequestBuilder::new().push_deploy(deploy).build()
    };

    let mut builder = InMemoryWasmTestBuilder::default();

    builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(exec_request)
        .commit()
        .finish();

    let initial_balance: U512 = U512::from(DEFAULT_ACCOUNT_INITIAL_BALANCE);
    let expected_reward_balance: U512 = U512::from(MAX_PAYMENT);

    let modified_balance = builder.get_purse_balance(
        builder
            .get_account(DEFAULT_ACCOUNT_ADDR)
            .expect("should have account")
            .main_purse(),
    );
    let reward_balance = get_pos_rewards_purse_balance(&builder);

    assert_eq!(
        modified_balance,
        initial_balance - expected_reward_balance,
        "modified balance is incorrect"
    );

    assert_eq!(
        reward_balance, expected_reward_balance,
        "reward balance is incorrect"
    );

    assert_eq!(
        initial_balance,
        (modified_balance + reward_balance),
        "no net resources should be gained or lost post-distribution"
    );

    let response = builder
        .get_exec_response(0)
        .expect("there should be a response");

    let execution_result = utils::get_success_result(response);
    let error_message = format!("{}", execution_result.error().expect("should have error"));

    assert!(
        error_message.contains("Revert(65636)"),
        "expected payment error",
    );
}

#[ignore]
#[test]
fn should_forward_payment_execution_gas_limit_error() {
    let account_1_public_key = ACCOUNT_1_ADDR;
    let transferred_amount = U512::from(1);

    let exec_request = {
        let deploy = DeployItemBuilder::new()
            .with_address(DEFAULT_ACCOUNT_ADDR)
            .with_deploy_hash([1; 32])
            .with_payment_code(ENDLESS_LOOP_WASM, ())
            .with_session_code(
                TRANSFER_PURSE_TO_ACCOUNT_WASM,
                (account_1_public_key, transferred_amount),
            )
            .with_authorization_keys(&[DEFAULT_ACCOUNT_KEY])
            .build();

        ExecuteRequestBuilder::new().push_deploy(deploy).build()
    };

    let mut builder = InMemoryWasmTestBuilder::default();

    builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(exec_request)
        .commit()
        .finish();

    let initial_balance: U512 = U512::from(DEFAULT_ACCOUNT_INITIAL_BALANCE);
    let expected_reward_balance: U512 = U512::from(MAX_PAYMENT);

    let modified_balance = builder.get_purse_balance(
        builder
            .get_account(DEFAULT_ACCOUNT_ADDR)
            .expect("should have account")
            .main_purse(),
    );
    let reward_balance = get_pos_rewards_purse_balance(&builder);

    assert_eq!(
        modified_balance,
        initial_balance - expected_reward_balance,
        "modified balance is incorrect"
    );

    assert_eq!(
        reward_balance, expected_reward_balance,
        "reward balance is incorrect"
    );

    assert_eq!(
        initial_balance,
        (modified_balance + reward_balance),
        "no net resources should be gained or lost post-distribution"
    );

    let response = builder
        .get_exec_response(0)
        .expect("there should be a response");

    let execution_result = utils::get_success_result(response);
    let error_message = format!("{}", execution_result.error().expect("should have error"));

    assert!(
        error_message.contains("GasLimit"),
        "expected gas limit error"
    );
}

#[ignore]
#[test]
fn should_run_out_of_gas_when_session_code_exceeds_gas_limit() {
    let account_1_public_key = ACCOUNT_1_ADDR;
    let payment_purse_amount = 10_000_000;
    let transferred_amount = 1;

    let exec_request = {
        let deploy = DeployItemBuilder::new()
            .with_address(DEFAULT_ACCOUNT_ADDR)
            .with_deploy_hash([1; 32])
            .with_empty_payment_bytes((U512::from(payment_purse_amount),))
            .with_session_code(
                ENDLESS_LOOP_WASM,
                (account_1_public_key, U512::from(transferred_amount)),
            )
            .with_authorization_keys(&[DEFAULT_ACCOUNT_KEY])
            .build();

        ExecuteRequestBuilder::new().push_deploy(deploy).build()
    };

    let mut builder = InMemoryWasmTestBuilder::default();

    let transfer_result = builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(exec_request)
        .commit()
        .finish();

    let response = transfer_result
        .builder()
        .get_exec_response(0)
        .expect("there should be a response");

    let execution_result = utils::get_success_result(response);
    let error_message = format!("{}", execution_result.error().expect("should have error"));

    assert!(
        error_message.contains("GasLimit"),
        "expected gas limit, got {}",
        error_message
    );
}

#[ignore]
#[test]
fn should_correctly_charge_when_session_code_runs_out_of_gas() {
    let payment_purse_amount = 10_000_000;

    let exec_request = {
        let deploy = DeployItemBuilder::new()
            .with_address(DEFAULT_ACCOUNT_ADDR)
            .with_deploy_hash([1; 32])
            .with_empty_payment_bytes((U512::from(payment_purse_amount),))
            .with_session_code(ENDLESS_LOOP_WASM, ())
            .with_authorization_keys(&[DEFAULT_ACCOUNT_KEY])
            .build();

        ExecuteRequestBuilder::new().push_deploy(deploy).build()
    };

    let mut builder = InMemoryWasmTestBuilder::default();

    builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(exec_request)
        .commit()
        .finish();

    let default_account = builder
        .get_account(DEFAULT_ACCOUNT_ADDR)
        .expect("should get genesis account");
    let modified_balance: U512 = builder.get_purse_balance(default_account.main_purse());
    let initial_balance: U512 = U512::from(DEFAULT_ACCOUNT_INITIAL_BALANCE);

    assert_ne!(
        modified_balance, initial_balance,
        "balance should be less than initial balance"
    );

    let response = builder
        .get_exec_response(0)
        .expect("there should be a response");

    let success_result = utils::get_success_result(&response);
    let gas = success_result.cost();
    let motes = Motes::from_gas(gas, CONV_RATE).expect("should have motes");

    let tally = motes.value() + modified_balance;

    assert_eq!(
        initial_balance, tally,
        "no net resources should be gained or lost post-distribution"
    );

    let execution_result = utils::get_success_result(response);
    let error_message = format!("{}", execution_result.error().expect("should have error"));

    assert!(error_message.contains("GasLimit"), "expected gas limit");
}

#[ignore]
#[test]
fn should_correctly_charge_when_session_code_fails() {
    let account_1_public_key = ACCOUNT_1_ADDR;
    let payment_purse_amount = 10_000_000;
    let transferred_amount = 1;

    let exec_request = {
        let deploy = DeployItemBuilder::new()
            .with_address(DEFAULT_ACCOUNT_ADDR)
            .with_deploy_hash([1; 32])
            .with_empty_payment_bytes((U512::from(payment_purse_amount),))
            .with_session_code(
                REVERT_WASM,
                (account_1_public_key, U512::from(transferred_amount)),
            )
            .with_authorization_keys(&[DEFAULT_ACCOUNT_KEY])
            .build();

        ExecuteRequestBuilder::new().push_deploy(deploy).build()
    };

    let mut builder = InMemoryWasmTestBuilder::default();

    builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(exec_request)
        .commit()
        .finish();

    let default_account = builder
        .get_account(DEFAULT_ACCOUNT_ADDR)
        .expect("should get genesis account");
    let modified_balance: U512 = builder.get_purse_balance(default_account.main_purse());
    let initial_balance: U512 = U512::from(DEFAULT_ACCOUNT_INITIAL_BALANCE);

    assert_ne!(
        modified_balance, initial_balance,
        "balance should be less than initial balance"
    );

    let response = builder
        .get_exec_response(0)
        .expect("there should be a response")
        .clone();

    let success_result = utils::get_success_result(&response);
    let gas = success_result.cost();
    let motes = Motes::from_gas(gas, CONV_RATE).expect("should have motes");
    let tally = motes.value() + modified_balance;

    assert_eq!(
        initial_balance, tally,
        "no net resources should be gained or lost post-distribution"
    );
}

#[ignore]
#[test]
fn should_correctly_charge_when_session_code_succeeds() {
    let account_1_public_key = ACCOUNT_1_ADDR;
    let payment_purse_amount = 10_000_000;
    let transferred_amount = 1;

    let exec_request = {
        let deploy = DeployItemBuilder::new()
            .with_address(DEFAULT_ACCOUNT_ADDR)
            .with_deploy_hash([1; 32])
            .with_session_code(
                TRANSFER_PURSE_TO_ACCOUNT_WASM,
                (account_1_public_key, U512::from(transferred_amount)),
            )
            .with_empty_payment_bytes((U512::from(payment_purse_amount),))
            .with_authorization_keys(&[DEFAULT_ACCOUNT_KEY])
            .build();

        ExecuteRequestBuilder::new().push_deploy(deploy).build()
    };

    let mut builder = InMemoryWasmTestBuilder::default();

    builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(exec_request)
        .expect_success()
        .commit()
        .finish();

    let default_account = builder
        .get_account(DEFAULT_ACCOUNT_ADDR)
        .expect("should get genesis account");
    let modified_balance: U512 = builder.get_purse_balance(default_account.main_purse());
    let initial_balance: U512 = U512::from(DEFAULT_ACCOUNT_INITIAL_BALANCE);

    assert_ne!(
        modified_balance, initial_balance,
        "balance should be less than initial balance"
    );

    let response = builder
        .get_exec_response(0)
        .expect("there should be a response")
        .clone();

    let success_result = utils::get_success_result(&response);
    let gas = success_result.cost();
    let motes = Motes::from_gas(gas, CONV_RATE).expect("should have motes");
    let total = motes.value() + U512::from(transferred_amount);
    let tally = total + modified_balance;

    assert_eq!(
        initial_balance, tally,
        "no net resources should be gained or lost post-distribution"
    );
    assert_eq!(
        initial_balance, tally,
        "no net resources should be gained or lost post-distribution"
    )
}

fn get_pos_purse_by_name(builder: &InMemoryWasmTestBuilder, purse_name: &str) -> Option<URef> {
    let pos_contract = builder.get_pos_contract();

    pos_contract
        .named_keys()
        .get(purse_name)
        .and_then(Key::as_uref)
        .cloned()
}

fn get_pos_rewards_purse_balance(builder: &InMemoryWasmTestBuilder) -> U512 {
    let purse =
        get_pos_purse_by_name(builder, POS_REWARDS_PURSE).expect("should find PoS payment purse");
    builder.get_purse_balance(purse)
}

#[ignore]
#[test]
fn should_finalize_to_rewards_purse() {
    let account_1_public_key = ACCOUNT_1_ADDR;
    let payment_purse_amount = 10_000_000;
    let transferred_amount = 1;

    let exec_request = {
        let deploy = DeployItemBuilder::new()
            .with_address(DEFAULT_ACCOUNT_ADDR)
            .with_session_code(
                TRANSFER_PURSE_TO_ACCOUNT_WASM,
                (account_1_public_key, U512::from(transferred_amount)),
            )
            .with_empty_payment_bytes((U512::from(payment_purse_amount),))
            .with_authorization_keys(&[DEFAULT_ACCOUNT_KEY])
            .with_deploy_hash([1; 32])
            .build();

        ExecuteRequestBuilder::new().push_deploy(deploy).build()
    };

    let mut builder = InMemoryWasmTestBuilder::default();

    builder.run_genesis(&DEFAULT_GENESIS_CONFIG);

    let rewards_purse_balance = get_pos_rewards_purse_balance(&builder);
    assert!(rewards_purse_balance.is_zero());

    builder.exec(exec_request).expect_success().commit();

    let rewards_purse_balance = get_pos_rewards_purse_balance(&builder);
    assert!(!rewards_purse_balance.is_zero());
}

#[ignore]
#[test]
fn independent_standard_payments_should_not_write_the_same_keys() {
    let account_1_public_key = ACCOUNT_1_ADDR;
    let payment_purse_amount = 10_000_000;
    let transfer_amount = 10_000_000;

    let mut builder = InMemoryWasmTestBuilder::default();

    let setup_exec_request = {
        let deploy = DeployItemBuilder::new()
            .with_address(DEFAULT_ACCOUNT_ADDR)
            .with_session_code(
                TRANSFER_PURSE_TO_ACCOUNT_WASM,
                (account_1_public_key, U512::from(transfer_amount)),
            )
            .with_empty_payment_bytes((U512::from(payment_purse_amount),))
            .with_authorization_keys(&[DEFAULT_ACCOUNT_KEY])
            .with_deploy_hash([1; 32])
            .build();

        ExecuteRequestBuilder::new().push_deploy(deploy).build()
    };

    // create another account via transfer
    builder
        .run_genesis(&DEFAULT_GENESIS_CONFIG)
        .exec(setup_exec_request)
        .expect_success()
        .commit();

    let exec_request_from_genesis = {
        let deploy = DeployItemBuilder::new()
            .with_address(DEFAULT_ACCOUNT_ADDR)
            .with_session_code(DO_NOTHING_WASM, ())
            .with_empty_payment_bytes((U512::from(payment_purse_amount),))
            .with_authorization_keys(&[DEFAULT_ACCOUNT_KEY])
            .with_deploy_hash([2; 32])
            .build();

        ExecuteRequestBuilder::new().push_deploy(deploy).build()
    };

    let exec_request_from_account_1 = {
        let deploy = DeployItemBuilder::new()
            .with_address(ACCOUNT_1_ADDR)
            .with_session_code(DO_NOTHING_WASM, ())
            .with_empty_payment_bytes((U512::from(payment_purse_amount),))
            .with_authorization_keys(&[account_1_public_key])
            .with_deploy_hash([1; 32])
            .build();

        ExecuteRequestBuilder::new().push_deploy(deploy).build()
    };

    // run two independent deploys
    builder
        .exec(exec_request_from_genesis)
        .expect_success()
        .commit()
        .exec(exec_request_from_account_1)
        .expect_success()
        .commit();

    let transforms = builder.get_transforms();
    let transforms_from_genesis = &transforms[1];
    let transforms_from_account_1 = &transforms[2];

    // confirm the two deploys have no overlapping writes
    let common_write_keys = transforms_from_genesis.keys().filter(|k| {
        match (
            transforms_from_genesis.get(k),
            transforms_from_account_1.get(k),
        ) {
            (Some(Transform::Write(_)), Some(Transform::Write(_))) => true,
            _ => false,
        }
    });

    assert_eq!(common_write_keys.count(), 0);
}

use engine_core::{
    engine_state::{
        execution_effect::ExecutionEffect, execution_result::ExecutionResult,
        Error as EngineStateError,
    },
    execution::Error as ExecutionError,
};
use engine_shared::gas::Gas;

use crate::engine_server::ipc::{DeployError_OutOfGasError, DeployResult};

impl From<ExecutionResult> for DeployResult {
    fn from(execution_result: ExecutionResult) -> DeployResult {
        match execution_result {
            ExecutionResult::Success { effect, cost } => detail::execution_success(effect, cost),
            ExecutionResult::Failure {
                error,
                effect,
                cost,
            } => (error, effect, cost).into(),
        }
    }
}

impl From<(EngineStateError, ExecutionEffect, Gas)> for DeployResult {
    fn from((engine_state_error, effect, cost): (EngineStateError, ExecutionEffect, Gas)) -> Self {
        match engine_state_error {
            // TODO(mateusz.gorski): Fix error model for the storage errors.
            // We don't have separate IPC messages for storage errors so for the time being they are
            // all reported as "wasm errors".
            error @ EngineStateError::InvalidHashLength { .. }
            | error @ EngineStateError::InvalidPublicKeyLength { .. }
            | error @ EngineStateError::InvalidProtocolVersion { .. }
            | error @ EngineStateError::InvalidUpgradeConfig
            | error @ EngineStateError::WasmPreprocessing(_)
            | error @ EngineStateError::WasmSerialization(_)
            | error @ EngineStateError::DeploymentAuthorizationFailure
            | error @ EngineStateError::Authorization => {
                detail::precondition_error(error.to_string())
            }
            EngineStateError::Storage(storage_error) => {
                detail::storage_error(storage_error, effect, cost)
            }
            EngineStateError::MissingSystemContract(msg) => {
                detail::missing_system_contract(msg, effect, cost)
            }
            error @ EngineStateError::InsufficientPayment
            | error @ EngineStateError::Deploy
            | error @ EngineStateError::Finalization
            | error @ EngineStateError::Serialization(_)
            | error @ EngineStateError::Mint(_) => detail::execution_error(error, effect, cost),
            EngineStateError::Exec(exec_error) => (exec_error, effect, cost).into(),
        }
    }
}

impl From<(ExecutionError, ExecutionEffect, Gas)> for DeployResult {
    fn from((exec_error, effect, cost): (ExecutionError, ExecutionEffect, Gas)) -> Self {
        match exec_error {
            ExecutionError::GasLimit => detail::out_of_gas_error(effect, cost),
            ExecutionError::KeyNotFound(key) => {
                detail::execution_error(format!("Key {:?} not found.", key), effect, cost)
            }
            ExecutionError::Revert(status) => {
                detail::execution_error(format!("Exit code: {}", status), effect, cost)
            }
            ExecutionError::Interpreter(error) => {
                // If the error happens during contract execution it's mapped to HostError and
                // wrapped in Interpreter error, so we may end up with
                // InterpreterError(HostError(InterpreterError))).  In order to provide clear error
                // messages we have to downcast and match on the inner error, otherwise we end up
                // with `Host(Trap(Trap(TrapKind:InterpreterError)))`.
                // TODO: This really should be happening in the `Executor::exec`.
                let msg = match error
                    .as_host_error()
                    .and_then(|host_error| host_error.downcast_ref::<ExecutionError>())
                {
                    Some(&ExecutionError::Revert(status)) => format!("Exit code: {}", status),
                    Some(&ExecutionError::KeyNotFound(key)) => format!("Key {:?} not found.", key),
                    Some(&ExecutionError::InvalidContext) => {
                        // TODO: https://casperlabs.atlassian.net/browse/EE-771
                        "Invalid execution context.".to_string()
                    }
                    Some(other) => format!("{:?}", other),
                    None => format!("{:?}", error),
                };
                detail::execution_error(msg, effect, cost)
            }
            // TODO(mateusz.gorski): Be more specific about execution errors
            other => detail::execution_error(format!("{:?}", other), effect, cost),
        }
    }
}

mod detail {
    use super::{DeployError_OutOfGasError, DeployResult, ExecutionEffect, Gas};

    /// Constructs an instance of `DeployResult` with no error set, i.e. a successful
    /// result.
    pub(super) fn execution_success(effect: ExecutionEffect, cost: Gas) -> DeployResult {
        make_deploy_result(effect, cost)
    }

    /// Constructs an instance of `DeployResult` with an error set to
    /// `ProtobufPreconditionFailure`.
    pub(super) fn precondition_error(msg: String) -> DeployResult {
        let mut pb_deploy_result = DeployResult::new();
        pb_deploy_result.mut_precondition_failure().set_message(msg);
        pb_deploy_result
    }

    /// Constructs an instance of `DeployResult` with an error set to
    /// `ProtobufExecutionError`.
    pub(super) fn execution_error<T: ToString>(
        msg: T,
        effect: ExecutionEffect,
        cost: Gas,
    ) -> DeployResult {
        let mut deploy_result = make_deploy_result(effect, cost);
        deploy_result
            .mut_execution_result()
            .mut_error()
            .mut_exec_error()
            .set_message(msg.to_string());
        deploy_result
    }

    /// Constructs an instance of `DeployResult` with an error set to `StorageError`.
    pub(super) fn storage_error<T: ToString>(
        msg: T,
        effect: ExecutionEffect,
        cost: Gas,
    ) -> DeployResult {
        let mut deploy_result = make_deploy_result(effect, cost);
        deploy_result
            .mut_execution_result()
            .mut_error()
            .mut_storage_error()
            .set_message(msg.to_string());
        deploy_result
    }

    /// Constructs an instance of `DeployResult` with an error set to `MissingSystemContract`.
    pub(super) fn missing_system_contract<T: ToString>(
        msg: T,
        effect: ExecutionEffect,
        cost: Gas,
    ) -> DeployResult {
        let mut deploy_result = make_deploy_result(effect, cost);
        deploy_result
            .mut_execution_result()
            .mut_error()
            .mut_missing_system_contract()
            .set_message(msg.to_string());
        deploy_result
    }



    /// Constructs an instance of `DeployResult` with an error set to
    /// `DeployError_OutOfGasError`.
    pub(super) fn out_of_gas_error(effect: ExecutionEffect, cost: Gas) -> DeployResult {
        let mut deploy_result = make_deploy_result(effect, cost);
        deploy_result
            .mut_execution_result()
            .mut_error()
            .set_gas_error(DeployError_OutOfGasError::new());
        deploy_result
    }

    /// Constructs an instance of `DeployResult` with an error set to
    /// `DeployError_OutOfGasError` or `ProtobufExecutionError` or with no error set, depending on
    /// the value of `error_type`.
    fn make_deploy_result(effect: ExecutionEffect, cost: Gas) -> DeployResult {
        let mut pb_deploy_result = DeployResult::new();

        let pb_execution_result = pb_deploy_result.mut_execution_result();
        pb_execution_result.set_effects(effect.into());
        pb_execution_result.set_cost(cost.value().into());

        pb_deploy_result
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use engine_shared::{additive_map::AdditiveMap, transform::Transform};
    use types::{bytesrepr::Error as BytesReprError, AccessRights, Key, URef, U512};

    use super::*;

    #[test]
    fn deploy_result_to_ipc_success() {
        let input_transforms: AdditiveMap<Key, Transform> = {
            let mut tmp_map = AdditiveMap::new();
            tmp_map.insert(
                Key::URef(URef::new([1u8; 32], AccessRights::ADD)),
                Transform::AddInt32(10),
            );
            tmp_map
        };
        let execution_effect = ExecutionEffect::new(AdditiveMap::new(), input_transforms.clone());
        let cost = Gas::new(U512::from(123));
        let execution_result = ExecutionResult::Success {
            effect: execution_effect,
            cost,
        };
        let mut ipc_deploy_result: DeployResult = execution_result.into();
        assert!(ipc_deploy_result.has_execution_result());
        let mut success = ipc_deploy_result.take_execution_result();
        let execution_cost: U512 = success.take_cost().try_into().expect("should map to U512");
        assert_eq!(execution_cost, cost.value());

        // Extract transform map from the IPC message and parse it back to the domain
        let ipc_transforms: AdditiveMap<Key, Transform> = {
            let mut ipc_effects = success.take_effects();
            let ipc_effects_tnfs = ipc_effects.take_transform_map().into_vec();
            ipc_effects_tnfs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<AdditiveMap<Key, Transform>, _>>()
                .unwrap()
        };
        assert_eq!(input_transforms, ipc_transforms);
    }

    fn test_cost<E: Into<EngineStateError>>(expected_cost: Gas, error: E) -> Gas {
        let execution_failure = ExecutionResult::Failure {
            error: error.into(),
            effect: Default::default(),
            cost: expected_cost,
        };
        let mut ipc_deploy_result: DeployResult = execution_failure.into();
        assert!(ipc_deploy_result.has_execution_result());
        let execution_result = ipc_deploy_result.mut_execution_result();
        let execution_cost: U512 = execution_result
            .take_cost()
            .try_into()
            .expect("should map to U512");
        Gas::new(execution_cost)
    }

    #[test]
    fn storage_error_has_cost() {
        let cost = Gas::new(U512::from(100));
        // TODO: actually create an Rkv error
        // assert_eq!(test_cost(cost, RkvError("Error".to_owned())), cost);
        let bytesrepr_err = BytesReprError::EarlyEndOfStream;
        assert_eq!(
            test_cost(cost, ExecutionError::BytesRepr(bytesrepr_err)),
            cost
        );
    }

    #[test]
    fn exec_err_has_cost() {
        let cost = Gas::new(U512::from(100));
        // GasLimit error is treated differently at the moment so test separately
        assert_eq!(test_cost(cost, ExecutionError::GasLimit), cost);
        // for the time being all other execution errors are treated in the same way
        let forged_ref_error =
            ExecutionError::ForgedReference(URef::new([1u8; 32], AccessRights::READ_ADD_WRITE));
        assert_eq!(test_cost(cost, forged_ref_error), cost);
    }

    #[test]
    fn revert_error_maps_to_execution_error() {
        const REVERT: u32 = 10;
        let revert_error = ExecutionError::Revert(REVERT);
        let amount = U512::from(15);
        let exec_result = ExecutionResult::Failure {
            error: EngineStateError::Exec(revert_error),
            effect: Default::default(),
            cost: Gas::new(amount),
        };
        let mut ipc_result: DeployResult = exec_result.into();
        assert!(
            ipc_result.has_execution_result(),
            "should have execution result"
        );
        let ipc_execution_result = ipc_result.mut_execution_result();
        let execution_cost: U512 = ipc_execution_result
            .take_cost()
            .try_into()
            .expect("should map to U512");
        assert_eq!(execution_cost, amount, "execution cost should equal amount");
        assert_eq!(
            ipc_execution_result
                .get_error()
                .get_exec_error()
                .get_message(),
            format!("Exit code: {}", REVERT)
        );
    }
}

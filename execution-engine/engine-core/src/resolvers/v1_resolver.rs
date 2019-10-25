use std::cell::RefCell;

use wasmi::memory_units::Pages;
use wasmi::{
    Error as InterpreterError, FuncRef, MemoryDescriptor, MemoryInstance, Signature, ValueType,
};
use wasmi::{FuncInstance, MemoryRef, ModuleImportResolver};

use super::error::ResolverError;
use super::memory_resolver::MemoryResolver;
use super::v1_function_index::FunctionIndex;

pub struct RuntimeModuleImportResolver {
    memory: RefCell<Option<MemoryRef>>,
    max_memory: u32,
}

impl Default for RuntimeModuleImportResolver {
    fn default() -> Self {
        RuntimeModuleImportResolver {
            memory: RefCell::new(None),
            max_memory: 64,
        }
    }
}

impl MemoryResolver for RuntimeModuleImportResolver {
    fn memory_ref(&self) -> Result<MemoryRef, ResolverError> {
        self.memory
            .borrow()
            .as_ref()
            .map(Clone::clone)
            .ok_or(ResolverError::NoImportedMemory)
    }
}

impl ModuleImportResolver for RuntimeModuleImportResolver {
    fn resolve_func(
        &self,
        field_name: &str,
        _signature: &Signature,
    ) -> Result<FuncRef, InterpreterError> {
        let func_ref = match field_name {
            "read_value" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 2][..], Some(ValueType::I32)),
                FunctionIndex::ReadFuncIndex.into(),
            ),
            "read_value_local" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 2][..], Some(ValueType::I32)),
                FunctionIndex::ReadLocalFuncIndex.into(),
            ),
            "serialize_named_keys" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 0][..], Some(ValueType::I32)),
                FunctionIndex::SerNamedKeysFuncIndex.into(),
            ),
            "write" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 4][..], None),
                FunctionIndex::WriteFuncIndex.into(),
            ),
            "write_local" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 4][..], None),
                FunctionIndex::WriteLocalFuncIndex.into(),
            ),
            "get_read" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], None),
                FunctionIndex::GetReadFuncIndex.into(),
            ),
            "get_function" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], None),
                FunctionIndex::GetFnFuncIndex.into(),
            ),
            "add" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 4][..], None),
                FunctionIndex::AddFuncIndex.into(),
            ),
            "new_uref" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 3][..], None),
                FunctionIndex::NewFuncIndex.into(),
            ),
            "load_arg" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], Some(ValueType::I32)),
                FunctionIndex::LoadArgFuncIndex.into(),
            ),
            "get_arg" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], None),
                FunctionIndex::GetArgFuncIndex.into(),
            ),
            "ret" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 4][..], None),
                FunctionIndex::RetFuncIndex.into(),
            ),
            "call_contract" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 6][..], Some(ValueType::I32)),
                FunctionIndex::CallContractFuncIndex.into(),
            ),
            "get_call_result" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], None),
                FunctionIndex::GetCallResultFuncIndex.into(),
            ),
            "get_key" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 2][..], Some(ValueType::I32)),
                FunctionIndex::GetKeyFuncIndex.into(),
            ),
            "has_key" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 2][..], Some(ValueType::I32)),
                FunctionIndex::HasKeyFuncIndex.into(),
            ),
            "put_key" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 4][..], None),
                FunctionIndex::PutKeyFuncIndex.into(),
            ),
            "gas" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], None),
                FunctionIndex::GasFuncIndex.into(),
            ),
            "store_function" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 5][..], None),
                FunctionIndex::StoreFnIndex.into(),
            ),
            "store_function_at_hash" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 5][..], None),
                FunctionIndex::StoreFnAtHashIndex.into(),
            ),
            "is_valid" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 2][..], Some(ValueType::I32)),
                FunctionIndex::IsValidFnIndex.into(),
            ),
            "revert" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], None),
                FunctionIndex::RevertFuncIndex.into(),
            ),
            "add_associated_key" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 2][..], Some(ValueType::I32)),
                FunctionIndex::AddAssociatedKeyFuncIndex.into(),
            ),
            "remove_associated_key" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], Some(ValueType::I32)),
                FunctionIndex::RemoveAssociatedKeyFuncIndex.into(),
            ),
            "update_associated_key" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 2][..], Some(ValueType::I32)),
                FunctionIndex::UpdateAssociatedKeyFuncIndex.into(),
            ),
            "set_action_threshold" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 2][..], Some(ValueType::I32)),
                FunctionIndex::SetActionThresholdFuncIndex.into(),
            ),
            "list_named_keys" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], None),
                FunctionIndex::ListNamedKeysFuncIndex.into(),
            ),
            "remove_key" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 2][..], None),
                FunctionIndex::RemoveKeyFuncIndex.into(),
            ),
            "get_caller" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], None),
                FunctionIndex::GetCallerIndex.into(),
            ),
            "get_blocktime" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], None),
                FunctionIndex::GetBlocktimeIndex.into(),
            ),
            "create_purse" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 2][..], Some(ValueType::I32)),
                FunctionIndex::CreatePurseIndex.into(),
            ),
            "transfer_to_account" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 4][..], Some(ValueType::I32)),
                FunctionIndex::TransferToAccountIndex.into(),
            ),
            "transfer_from_purse_to_account" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 6][..], Some(ValueType::I32)),
                FunctionIndex::TransferFromPurseToAccountIndex.into(),
            ),
            "get_balance" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 2][..], Some(ValueType::I32)),
                FunctionIndex::GetBalanceIndex.into(),
            ),
            "get_phase" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], None),
                FunctionIndex::GetPhaseIndex.into(),
            ),
            "upgrade_contract_at_uref" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 4][..], Some(ValueType::I32)),
                FunctionIndex::UpgradeContractAtURefIndex.into(),
            ),
            "get_system_contract" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 3][..], Some(ValueType::I32)),
                FunctionIndex::GetSystemContractIndex.into(),
            ),
            "get_main_purse" => FuncInstance::alloc_host(
                Signature::new(&[ValueType::I32; 1][..], None),
                FunctionIndex::GetMainPurseIndex.into(),
            ),
            _ => {
                return Err(InterpreterError::Function(format!(
                    "host module doesn't export function with name {}",
                    field_name
                )));
            }
        };
        Ok(func_ref)
    }

    fn resolve_memory(
        &self,
        field_name: &str,
        descriptor: &MemoryDescriptor,
    ) -> Result<MemoryRef, InterpreterError> {
        if field_name == "memory" {
            let effective_max = descriptor.maximum().unwrap_or(self.max_memory + 1);
            if descriptor.initial() > self.max_memory || effective_max > self.max_memory {
                Err(InterpreterError::Instantiation(
                    "Module requested too much memory".to_owned(),
                ))
            } else {
                // Note: each "page" is 64 KiB
                let mem = MemoryInstance::alloc(
                    Pages(descriptor.initial() as usize),
                    descriptor.maximum().map(|x| Pages(x as usize)),
                )?;
                *self.memory.borrow_mut() = Some(mem.clone());
                Ok(mem)
            }
        } else {
            Err(InterpreterError::Instantiation(
                "Memory imported under unknown name".to_owned(),
            ))
        }
    }
}

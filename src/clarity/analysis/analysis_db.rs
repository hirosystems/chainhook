use std::collections::{HashMap, BTreeMap, BTreeSet};
use crate::clarity::types::{TypeSignature, FunctionType, QualifiedContractIdentifier, TraitIdentifier};
use crate::clarity::types::signatures::FunctionSignature;
use crate::clarity::analysis::errors::{CheckError, CheckErrors, CheckResult};
use crate::clarity::analysis::type_checker::{ContractAnalysis};
use crate::clarity::representations::{ClarityName};
use crate::clarity::database::{Datastore, RollbackWrapper, ClarityBackingStore, ClaritySerializable, ClarityDeserializable};

impl ClaritySerializable for ContractAnalysis {
    fn serialize(&self) -> String {
        serde_json::to_string(self)
            .expect("Failed to serialize vm.Value")
    }
}

impl ClarityDeserializable<ContractAnalysis> for ContractAnalysis {
    fn deserialize(json: &str) -> Self {
        serde_json::from_str(json)
            .expect("Failed to serialize vm.Value")
    }
}

pub struct AnalysisDatabase <'a> {
    store: RollbackWrapper<'a>
}

impl <'a> AnalysisDatabase <'a> {
    pub fn new(store: &'a mut dyn ClarityBackingStore) -> AnalysisDatabase<'a> {
        AnalysisDatabase {
            store: RollbackWrapper::new(store)
        }
    }

    pub fn new_with_rollback_wrapper(store: RollbackWrapper<'a>) -> AnalysisDatabase<'a> {
        AnalysisDatabase { store }
    }

    pub fn execute <F, T, E> (&mut self, f: F) -> Result<T,E> where F: FnOnce(&mut Self) -> Result<T,E>, {
        self.begin();
        let result = f(self)
            .or_else(|e| {
                self.roll_back();
                Err(e)
            })?;
        self.commit();
        Ok(result)
    }

    pub fn begin(&mut self) {
        self.store.nest();
    }

    pub fn commit(&mut self) {
        self.store.commit();
    }

    pub fn roll_back(&mut self) {
        self.store.rollback();
    }

    fn storage_key() -> &'static str {
        "analysis"
    }

    pub fn has_contract(&mut self, contract_identifier: &QualifiedContractIdentifier) -> bool {
        self.store.has_metadata_entry(contract_identifier, AnalysisDatabase::storage_key())
    }

    pub fn load_contract(&mut self, contract_identifier: &QualifiedContractIdentifier) -> Option<ContractAnalysis> {
        self.store.get_metadata(contract_identifier, AnalysisDatabase::storage_key())
            // treat NoSuchContract error thrown by get_metadata as an Option::None --
            //    the analysis will propagate that as a CheckError anyways.
            .ok()?
            .map(|x| ContractAnalysis::deserialize(&x))
    }

    pub fn insert_contract(&mut self, contract_identifier: &QualifiedContractIdentifier, contract: &ContractAnalysis) -> CheckResult<()> {
        let key = AnalysisDatabase::storage_key();
        if self.store.has_metadata_entry(contract_identifier, key) {
            return Err(CheckErrors::ContractAlreadyExists(contract_identifier.to_string()).into())
        }

        self.store.insert_metadata(contract_identifier, key, &contract.serialize());
        Ok(())
    }

    pub fn get_public_function_type(&mut self, contract_identifier: &QualifiedContractIdentifier, function_name: &str) -> CheckResult<Option<FunctionType>> {
        let contract = self.load_contract(contract_identifier)
            .ok_or(CheckErrors::NoSuchContract(contract_identifier.to_string()))?;
        Ok(contract.get_public_function_type(function_name)
           .cloned())
    }

    pub fn get_read_only_function_type(&mut self, contract_identifier: &QualifiedContractIdentifier, function_name: &str) -> CheckResult<Option<FunctionType>> {
        let contract = self.load_contract(contract_identifier)
            .ok_or(CheckErrors::NoSuchContract(contract_identifier.to_string()))?;
        Ok(contract.get_read_only_function_type(function_name)
           .cloned())
    }

    pub fn get_defined_trait(&mut self, contract_identifier: &QualifiedContractIdentifier, trait_name: &str) -> CheckResult<Option<BTreeMap<ClarityName, FunctionSignature>>> {
        let contract = self.load_contract(contract_identifier)
            .ok_or(CheckErrors::NoSuchContract(contract_identifier.to_string()))?;
        Ok(contract.get_defined_trait(trait_name)
           .cloned())
    }

    pub fn get_implemented_traits(&mut self, contract_identifier: &QualifiedContractIdentifier) -> CheckResult<BTreeSet<TraitIdentifier>> {
        let contract = self.load_contract(contract_identifier)
            .ok_or(CheckErrors::NoSuchContract(contract_identifier.to_string()))?;
        Ok(contract.implemented_traits)
    }

    pub fn get_map_type(&mut self, contract_identifier: &QualifiedContractIdentifier, map_name: &str) -> CheckResult<(TypeSignature, TypeSignature)> {
        let contract = self.load_contract(contract_identifier)
            .ok_or(CheckErrors::NoSuchContract(contract_identifier.to_string()))?;
        let map_type = contract.get_map_type(map_name)
            .ok_or(CheckErrors::NoSuchMap(map_name.to_string()))?;
        Ok(map_type.clone())
    }

    pub fn destroy(self) -> RollbackWrapper<'a> {
        self.store
    }
}

use crate::analysis::annotation::Annotation;
use crate::analysis::ast_visitor::{traverse, ASTVisitor};
use crate::analysis::{AnalysisPass, AnalysisResult, Settings};
use crate::clarity::analysis::analysis_db::AnalysisDatabase;
pub use crate::clarity::analysis::types::ContractAnalysis;
use crate::clarity::ast::ContractAST;
use crate::clarity::representations::{SymbolicExpression, TraitDefinition};
use crate::clarity::types::{
    FixedFunction, FunctionSignature, FunctionType, PrincipalData, QualifiedContractIdentifier,
    TraitIdentifier, TypeSignature, Value,
};
use crate::clarity::{ClarityName, SymbolicExpressionType};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::iter::FromIterator;
use std::process;

use super::ast_visitor::TypedVar;

pub struct ASTDependencyDetector<'a> {
    dependencies: HashMap<QualifiedContractIdentifier, HashSet<QualifiedContractIdentifier>>,
    current_contract: Option<&'a QualifiedContractIdentifier>,
    defined_functions:
        HashMap<(&'a QualifiedContractIdentifier, &'a ClarityName), Vec<TypeSignature>>,
    defined_traits: HashMap<
        (&'a QualifiedContractIdentifier, &'a ClarityName),
        BTreeMap<ClarityName, FunctionSignature>,
    >,
    pending_function_checks: HashMap<
        // function identifier whose type is not yet defined
        (&'a QualifiedContractIdentifier, &'a ClarityName),
        // list of contracts that need to be checked once this function is
        // defined, together with the associated args
        Vec<(&'a QualifiedContractIdentifier, &'a [SymbolicExpression])>,
    >,
    pending_trait_checks: HashMap<
        // trait that is not yet defined
        &'a TraitIdentifier,
        // list of contracts that need to be checked once this trait is
        // defined, together with the function called and the associated args.
        Vec<(
            &'a QualifiedContractIdentifier,
            &'a ClarityName,
            &'a [SymbolicExpression],
        )>,
    >,
    params: Option<Vec<TypedVar<'a>>>,
}

impl<'a> ASTDependencyDetector<'a> {
    pub fn detect_dependencies(
        contract_asts: &'a HashMap<QualifiedContractIdentifier, ContractAST>,
    ) -> HashMap<QualifiedContractIdentifier, HashSet<QualifiedContractIdentifier>> {
        let mut detector = Self {
            dependencies: HashMap::new(),
            current_contract: None,
            defined_functions: HashMap::new(),
            defined_traits: HashMap::new(),
            pending_function_checks: HashMap::new(),
            pending_trait_checks: HashMap::new(),
            params: None,
        };

        for (contract_identifier, ast) in contract_asts {
            detector
                .dependencies
                .insert(contract_identifier.clone(), HashSet::new());
            detector.current_contract = Some(contract_identifier);
            traverse(&mut detector, &ast.expressions);
        }

        detector.dependencies
    }

    pub fn order_contracts(
        dependencies: &HashMap<QualifiedContractIdentifier, HashSet<QualifiedContractIdentifier>>,
    ) -> Vec<&QualifiedContractIdentifier> {
        let mut lookup = BTreeMap::new();
        let mut reverse_lookup = Vec::new();

        let mut index: usize = 0;

        if dependencies.is_empty() {
            return vec![];
        }

        for (contract, _) in dependencies {
            lookup.insert(contract, index);
            reverse_lookup.push(contract);
            index += 1;
        }

        let mut graph = Graph::new();
        for (contract, contract_dependencies) in dependencies {
            let contract_id = lookup.get(contract).unwrap();
            graph.add_node(*contract_id);
            for dep in contract_dependencies.iter() {
                let dep_id = match lookup.get(dep) {
                    Some(id) => id,
                    None => {
                        println!(
                            "{}: {} depends on {}, but {} is not found",
                            red!("error"),
                            contract,
                            dep,
                            dep
                        );
                        process::exit(1);
                    }
                };
                graph.add_directed_edge(*contract_id, *dep_id);
            }
        }

        let mut walker = GraphWalker::new();
        let sorted_indexes = walker.get_sorted_dependencies(&graph);

        let cyclic_deps = walker.get_cycling_dependencies(&graph, &sorted_indexes);
        if let Some(deps) = cyclic_deps {
            let mut contracts = vec![];
            for index in deps.iter() {
                let contract = reverse_lookup[*index];
                contracts.push(contract.name.as_str());
            }
            println!(
                "{}: cycling dependencies: {}",
                red!("error"),
                contracts.join(", ")
            );
            process::exit(1);
        }

        sorted_indexes
            .iter()
            .map(|index| reverse_lookup[*index])
            .collect()
    }

    fn add_dependency(
        &mut self,
        from: &QualifiedContractIdentifier,
        to: &QualifiedContractIdentifier,
    ) {
        if let Some(set) = self.dependencies.get_mut(from) {
            set.insert(to.clone());
        } else {
            let mut set = HashSet::new();
            set.insert(to.clone());
            self.dependencies.insert(from.clone(), set);
        }
    }

    fn add_defined_function(
        &mut self,
        contract_identifier: &'a QualifiedContractIdentifier,
        name: &'a ClarityName,
        param_types: Vec<TypeSignature>,
    ) {
        if let Some(pending) = self
            .pending_function_checks
            .remove(&(contract_identifier, name))
        {
            for (caller, args) in pending {
                for dependency in self.check_callee_type(&param_types, args) {
                    self.add_dependency(caller, &dependency);
                }
            }
        }

        self.defined_functions
            .insert((contract_identifier, name), param_types);
    }

    fn add_pending_function_check(
        &mut self,
        caller: &'a QualifiedContractIdentifier,
        callee: (&'a QualifiedContractIdentifier, &'a ClarityName),
        args: &'a [SymbolicExpression],
    ) {
        if let Some(list) = self.pending_function_checks.get_mut(&callee) {
            list.push((caller, args));
        } else {
            self.pending_function_checks
                .insert(callee, vec![(caller, args)]);
        }
    }

    fn add_defined_trait(
        &mut self,
        contract_identifier: &'a QualifiedContractIdentifier,
        name: &'a ClarityName,
        trait_definition: BTreeMap<ClarityName, FunctionSignature>,
    ) {
        if let Some(pending) = self.pending_trait_checks.remove(&TraitIdentifier {
            name: name.clone(),
            contract_identifier: contract_identifier.clone(),
        }) {
            for (caller, function, args) in pending {
                for dependency in self.check_trait_dependencies(&trait_definition, function, args) {
                    self.add_dependency(caller, &dependency);
                }
            }
        }

        self.defined_traits
            .insert((contract_identifier, name), trait_definition);
    }

    fn add_pending_trait_check(
        &mut self,
        caller: &'a QualifiedContractIdentifier,
        callee: &'a TraitIdentifier,
        function: &'a ClarityName,
        args: &'a [SymbolicExpression],
    ) {
        if let Some(list) = self.pending_trait_checks.get_mut(callee) {
            list.push((caller, function, args));
        } else {
            self.pending_trait_checks
                .insert(callee, vec![(caller, function, args)]);
        }
    }

    fn check_callee_type(
        &self,
        arg_types: &Vec<TypeSignature>,
        args: &'a [SymbolicExpression],
    ) -> Vec<QualifiedContractIdentifier> {
        let mut dependencies = Vec::new();
        for (i, arg) in arg_types.iter().enumerate() {
            if matches!(arg, TypeSignature::TraitReferenceType(_)) {
                if let Some(Value::Principal(PrincipalData::Contract(contract))) =
                    args[i].match_literal_value()
                {
                    dependencies.push(contract.clone());
                }
            }
        }
        dependencies
    }

    fn check_trait_dependencies(
        &self,
        trait_definition: &BTreeMap<ClarityName, FunctionSignature>,
        function_name: &ClarityName,
        args: &'a [SymbolicExpression],
    ) -> Vec<QualifiedContractIdentifier> {
        let function_signature = trait_definition.get(function_name).unwrap();
        self.check_callee_type(&function_signature.args, args)
    }

    // A trait can only come from a parameter (cannot be a let binding or a return value), so
    // find the corresponding parameter and return it.
    fn get_param_trait(&self, name: &ClarityName) -> Option<&'a TraitIdentifier> {
        let params = match &self.params {
            None => return None,
            Some(params) => params,
        };
        for param in params {
            if param.name == name {
                if let SymbolicExpressionType::TraitReference(_, trait_def) = &param.type_expr.expr
                {
                    return match trait_def {
                        TraitDefinition::Defined(identifier) => Some(identifier),
                        TraitDefinition::Imported(identifier) => Some(identifier),
                    };
                } else {
                    return None;
                }
            }
        }
        None
    }
}

impl<'a> ASTVisitor<'a> for ASTDependencyDetector<'a> {
    // For the following traverse_define_* functions, we just want to store a
    // map of the parameter types, to be used to extract the trait type in a
    // dynamic contract call.
    fn traverse_define_private(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        self.params = parameters.clone();
        let res =
            self.traverse_expr(body) && self.visit_define_private(expr, name, parameters, body);
        self.params = None;
        res
    }

    fn visit_define_private(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        if let Some(parameters) = parameters {
            let param_types: Vec<TypeSignature> = parameters
                .iter()
                .map(|typed_var| {
                    TypeSignature::parse_type_repr(typed_var.type_expr, &mut ()).unwrap()
                })
                .collect();

            self.add_defined_function(self.current_contract.unwrap(), name, param_types);
        };
        true
    }

    fn traverse_define_read_only(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        self.params = parameters.clone();
        let res =
            self.traverse_expr(body) && self.visit_define_read_only(expr, name, parameters, body);
        self.params = None;
        res
    }

    fn visit_define_read_only(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        if let Some(parameters) = parameters {
            let param_types: Vec<TypeSignature> = parameters
                .iter()
                .map(|typed_var| {
                    TypeSignature::parse_type_repr(typed_var.type_expr, &mut ()).unwrap()
                })
                .collect();

            self.add_defined_function(self.current_contract.unwrap(), name, param_types);
        };
        true
    }

    fn traverse_define_public(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        self.params = parameters.clone();
        let res =
            self.traverse_expr(body) && self.visit_define_public(expr, name, parameters, body);
        self.params = None;
        res
    }

    fn visit_define_public(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        if let Some(parameters) = parameters {
            let param_types: Vec<TypeSignature> = parameters
                .iter()
                .map(|typed_var| {
                    TypeSignature::parse_type_repr(typed_var.type_expr, &mut ()).unwrap()
                })
                .collect();

            self.add_defined_function(self.current_contract.unwrap(), name, param_types);
        };
        true
    }

    fn visit_define_trait(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        functions: &'a [SymbolicExpression],
    ) -> bool {
        if let Ok(trait_definition) = TypeSignature::parse_trait_type_repr(functions, &mut ()) {
            self.add_defined_trait(self.current_contract.unwrap(), name, trait_definition);
        }
        true
    }

    fn visit_static_contract_call(
        &mut self,
        expr: &'a SymbolicExpression,
        contract_identifier: &'a QualifiedContractIdentifier,
        function_name: &'a ClarityName,
        args: &'a [SymbolicExpression],
    ) -> bool {
        self.add_dependency(self.current_contract.unwrap(), contract_identifier);
        let dependencies = if let Some(arg_types) = self
            .defined_functions
            .get(&(contract_identifier, function_name))
        {
            // If we know the type of this function, check the parameters for traits
            self.check_callee_type(arg_types, args)
        } else {
            // If we do not yet know the type of this function, record it to re-analyze later
            self.add_pending_function_check(
                self.current_contract.unwrap(),
                (contract_identifier, function_name),
                args,
            );
            return true;
        };
        for dependency in dependencies {
            self.add_dependency(self.current_contract.unwrap(), &dependency);
        }
        true
    }

    fn visit_dynamic_contract_call(
        &mut self,
        expr: &'a SymbolicExpression,
        trait_ref: &'a SymbolicExpression,
        function_name: &'a ClarityName,
        args: &'a [SymbolicExpression],
    ) -> bool {
        let trait_instance = trait_ref.match_atom().unwrap();
        if let Some(trait_identifier) = self.get_param_trait(trait_instance) {
            let dependencies = if let Some(trait_definition) = self.defined_traits.get(&(
                &trait_identifier.contract_identifier,
                &trait_identifier.name,
            )) {
                self.check_trait_dependencies(trait_definition, function_name, args)
            } else {
                self.add_pending_trait_check(
                    &self.current_contract.unwrap(),
                    trait_identifier,
                    function_name,
                    args,
                );
                return true;
            };

            for dependency in dependencies {
                self.add_dependency(self.current_contract.unwrap(), &dependency);
            }
        }
        true
    }

    fn visit_call_user_defined(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        args: &'a [SymbolicExpression],
    ) -> bool {
        let mut dependencies = Vec::new();
        if let Some(arg_types) = self
            .defined_functions
            .get(&(&self.current_contract.unwrap(), name))
        {
            for (i, arg) in arg_types.iter().enumerate() {
                if matches!(arg, TypeSignature::TraitReferenceType(_)) {
                    if let Some(Value::Principal(PrincipalData::Contract(contract))) =
                        args[i].match_literal_value()
                    {
                        dependencies.push(contract);
                    }
                }
            }
        }

        for dependency in dependencies {
            self.add_dependency(self.current_contract.unwrap(), dependency);
        }

        true
    }

    fn visit_use_trait(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        trait_identifier: &TraitIdentifier,
    ) -> bool {
        self.add_dependency(
            self.current_contract.unwrap(),
            &trait_identifier.contract_identifier,
        );
        true
    }

    fn visit_impl_trait(
        &mut self,
        expr: &'a SymbolicExpression,
        trait_identifier: &TraitIdentifier,
    ) -> bool {
        self.add_dependency(
            self.current_contract.unwrap(),
            &trait_identifier.contract_identifier,
        );
        true
    }
}

struct Graph {
    pub adjacency_list: Vec<Vec<usize>>,
}

impl Graph {
    fn new() -> Self {
        Self {
            adjacency_list: Vec::new(),
        }
    }

    fn add_node(&mut self, _expr_index: usize) {
        self.adjacency_list.push(vec![]);
    }

    fn add_directed_edge(&mut self, src_expr_index: usize, dst_expr_index: usize) {
        let list = self.adjacency_list.get_mut(src_expr_index).unwrap();
        list.push(dst_expr_index);
    }

    fn get_node_descendants(&self, expr_index: usize) -> Vec<usize> {
        self.adjacency_list[expr_index].clone()
    }

    fn has_node_descendants(&self, expr_index: usize) -> bool {
        self.adjacency_list[expr_index].len() > 0
    }

    fn nodes_count(&self) -> usize {
        self.adjacency_list.len()
    }
}

struct GraphWalker {
    seen: HashSet<usize>,
}

impl GraphWalker {
    fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    /// Depth-first search producing a post-order sort
    fn get_sorted_dependencies(&mut self, graph: &Graph) -> Vec<usize> {
        let mut sorted_indexes = Vec::<usize>::new();
        for expr_index in 0..graph.nodes_count() {
            self.sort_dependencies_recursion(expr_index, graph, &mut sorted_indexes);
        }

        sorted_indexes
    }

    fn sort_dependencies_recursion(
        &mut self,
        tle_index: usize,
        graph: &Graph,
        branch: &mut Vec<usize>,
    ) {
        if self.seen.contains(&tle_index) {
            return;
        }

        self.seen.insert(tle_index);
        if let Some(list) = graph.adjacency_list.get(tle_index) {
            for neighbor in list.iter() {
                self.sort_dependencies_recursion(neighbor.clone(), graph, branch);
            }
        }
        branch.push(tle_index);
    }

    fn get_cycling_dependencies(
        &mut self,
        graph: &Graph,
        sorted_indexes: &Vec<usize>,
    ) -> Option<Vec<usize>> {
        let mut tainted: HashSet<usize> = HashSet::new();

        for node in sorted_indexes.iter() {
            let mut tainted_descendants_count = 0;
            let descendants = graph.get_node_descendants(*node);
            for descendant in descendants.iter() {
                if !graph.has_node_descendants(*descendant) || tainted.contains(descendant) {
                    tainted.insert(*descendant);
                    tainted_descendants_count += 1;
                }
            }
            if tainted_descendants_count == descendants.len() {
                tainted.insert(*node);
            }
        }

        if tainted.len() == sorted_indexes.len() {
            return None;
        }

        let nodes = HashSet::from_iter(sorted_indexes.iter().cloned());
        let deps = nodes.difference(&tainted).map(|i| *i).collect();
        Some(deps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repl::session::Session;
    use crate::repl::SessionSettings;

    #[test]
    fn no_deps() {
        let mut session = Session::new(SessionSettings::default());
        let snippet = "
(define-public (hello)
    (ok (print \"hello\"))
)
"
        .to_string();
        match session.build_ast(&snippet, None) {
            Ok((contract_identifier, ast, _)) => {
                let mut contracts = HashMap::new();
                contracts.insert(contract_identifier.clone(), ast);
                let dependencies = ASTDependencyDetector::detect_dependencies(&contracts);
                assert_eq!(dependencies[&contract_identifier].len(), 0);
            }
            Err(_) => panic!("expected success"),
        }
    }

    #[test]
    fn contract_call() {
        let mut session = Session::new(SessionSettings::default());
        let mut contracts = HashMap::new();
        let snippet1 = "
(define-public (hello (a int))
    (ok u0)
)"
        .to_string();
        let foo = match session.build_ast(&snippet1, Some("foo")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let snippet = "
(define-public (call-foo)
    (contract-call? .foo hello 4)
)
"
        .to_string();
        let test_identifier = match session.build_ast(&snippet, Some("test")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let dependencies = ASTDependencyDetector::detect_dependencies(&contracts);
        assert_eq!(dependencies[&test_identifier].len(), 1);
        assert!(dependencies[&test_identifier].contains(&foo));
    }

    // This test is disabled because it is currently not possible to refer to a
    // trait defined in the same contract. An issue has been opened to discuss
    // whether this will be fixed or documented.
    // #[test]
    fn dynamic_contract_call_local_trait() {
        let mut session = Session::new(SessionSettings::default());
        let mut contracts = HashMap::new();
        let snippet1 = "
(define-public (hello (a int))
    (ok u0)
)"
        .to_string();
        let bar = match session.build_ast(&snippet1, Some("bar")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let snippet = "
(define-trait my-trait
    ((hello (int) (response uint uint)))
)
(define-trait dyn-trait
    ((call-hello (<my-trait>) (response uint uint)))
)
(define-public (call-dyn (dt <dyn-trait>))
    (contract-call? dt call-hello .bar)
)
"
        .to_string();
        let test_identifier = match session.build_ast(&snippet, Some("test")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let dependencies = ASTDependencyDetector::detect_dependencies(&contracts);
        assert_eq!(dependencies[&test_identifier].len(), 1);
        assert!(dependencies[&test_identifier].contains(&bar));
    }

    #[test]
    fn dynamic_contract_call_remote_trait() {
        let mut session = Session::new(SessionSettings::default());
        let mut contracts = HashMap::new();
        let snippet1 = "
(define-trait my-trait
    ((hello (int) (response uint uint)))
)
(define-public (hello (a int))
    (ok u0)
)"
        .to_string();
        let bar = match session.build_ast(&snippet1, Some("bar")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let snippet = "
(use-trait my-trait .bar.my-trait)
(define-trait dyn-trait
    ((call-hello (<my-trait>) (response uint uint)))
)
(define-public (call-dyn (dt <dyn-trait>))
    (contract-call? dt call-hello .bar)
)
"
        .to_string();
        let test_identifier = match session.build_ast(&snippet, Some("test")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let dependencies = ASTDependencyDetector::detect_dependencies(&contracts);
        assert_eq!(dependencies[&test_identifier].len(), 1);
        assert!(dependencies[&test_identifier].contains(&bar));
    }

    #[test]
    fn pass_contract_local() {
        let mut session = Session::new(SessionSettings::default());
        let mut contracts = HashMap::new();
        let snippet1 = "
(define-public (hello (a int))
    (ok u0)
)"
        .to_string();
        let bar = match session.build_ast(&snippet1, Some("bar")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let snippet2 = "
(define-trait my-trait
    ((hello (int) (response uint uint)))
)"
        .to_string();
        let my_trait = match session.build_ast(&snippet2, Some("my-trait")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let snippet = "
(use-trait my-trait .my-trait.my-trait)
(define-private (pass-trait (a <my-trait>))
    (print a)
)
(define-public (call-it)
    (ok (pass-trait .bar))
)
"
        .to_string();
        let test_identifier = match session.build_ast(&snippet, Some("test")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let dependencies = ASTDependencyDetector::detect_dependencies(&contracts);
        assert_eq!(dependencies[&test_identifier].len(), 2);
        assert!(dependencies[&test_identifier].contains(&bar));
        assert!(dependencies[&test_identifier].contains(&my_trait));
    }

    #[test]
    fn impl_trait() {
        let mut session = Session::new(SessionSettings::default());
        let mut contracts = HashMap::new();
        let snippet1 = "
(define-trait something
    ((hello (int) (response uint uint)))
)"
        .to_string();
        let other = match session.build_ast(&snippet1, Some("other")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let snippet = "
(impl-trait .other.something)
(define-public (hello (a int))
    (ok u0)
)
"
        .to_string();
        let test_identifier = match session.build_ast(&snippet, Some("test")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let dependencies = ASTDependencyDetector::detect_dependencies(&contracts);
        assert_eq!(dependencies[&test_identifier].len(), 1);
        assert!(dependencies[&test_identifier].contains(&other));
    }

    #[test]
    fn use_trait() {
        let mut session = Session::new(SessionSettings::default());
        let mut contracts = HashMap::new();
        let snippet1 = "
(define-trait something
    ((hello (int) (response uint uint)))
)"
        .to_string();
        let other = match session.build_ast(&snippet1, Some("other")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let snippet = "
(use-trait my-trait .other.something)
;; FIXME: If there is not a second line here, the interpreter will fail.
;; See https://github.com/hirosystems/clarity-repl/issues/109.
(define-public (foo) (ok true))
"
        .to_string();
        let test_identifier = match session.build_ast(&snippet, Some("test")) {
            Ok((contract_identifier, ast, _)) => {
                contracts.insert(contract_identifier.clone(), ast);
                contract_identifier
            }
            Err(_) => panic!("expected success"),
        };

        let dependencies = ASTDependencyDetector::detect_dependencies(&contracts);
        assert_eq!(dependencies[&test_identifier].len(), 1);
        assert!(dependencies[&test_identifier].contains(&other));
    }
}

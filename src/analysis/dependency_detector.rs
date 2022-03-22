use crate::analysis::annotation::Annotation;
use crate::analysis::ast_visitor::{traverse, ASTVisitor};
use crate::analysis::{AnalysisPass, AnalysisResult, Settings};
use crate::clarity::analysis::analysis_db::AnalysisDatabase;
pub use crate::clarity::analysis::types::ContractAnalysis;
use crate::clarity::ast::ContractAST;
use crate::clarity::representations::{SymbolicExpression, TraitDefinition};
use crate::clarity::types::{
    FunctionSignature, FunctionType, PrincipalData, QualifiedContractIdentifier, TraitIdentifier,
    TypeSignature, Value,
};
use crate::clarity::{ClarityName, SymbolicExpressionType};
use std::collections::{BTreeMap, BTreeSet};

use super::ast_visitor::TypedVar;

pub struct DependencyDetector<'a, 'b> {
    current_contract: QualifiedContractIdentifier,
    analysis_db: &'a mut AnalysisDatabase<'b>,
    function_signatures: BTreeMap<ClarityName, FunctionType>,
    defined_traits: BTreeMap<ClarityName, BTreeMap<ClarityName, FunctionSignature>>,
    deps: BTreeSet<QualifiedContractIdentifier>,
    params: Option<Vec<TypedVar<'a>>>,
}

impl<'a, 'b> DependencyDetector<'a, 'b> {
    fn new(
        current_contract: QualifiedContractIdentifier,
        analysis_db: &'a mut AnalysisDatabase<'b>,
        function_signatures: BTreeMap<ClarityName, FunctionType>,
        defined_traits: BTreeMap<ClarityName, BTreeMap<ClarityName, FunctionSignature>>,
    ) -> DependencyDetector<'a, 'b> {
        Self {
            current_contract,
            analysis_db,
            function_signatures,
            defined_traits,
            deps: BTreeSet::new(),
            params: None,
        }
    }

    fn run(&mut self, contract_analysis: &'a ContractAnalysis) -> AnalysisResult {
        traverse(self, &contract_analysis.expressions);
        Ok(vec![])
    }

    fn get_param_trait(&self, name: &ClarityName) -> Option<TraitIdentifier> {
        let params = match &self.params {
            None => return None,
            Some(params) => params,
        };
        for param in params {
            if param.name == name {
                if let SymbolicExpressionType::TraitReference(_, trait_def) =
                    param.type_expr.expr.clone()
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

impl<'a, 'b> ASTVisitor<'a> for DependencyDetector<'a, 'b> {
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

    fn visit_static_contract_call(
        &mut self,
        expr: &'a SymbolicExpression,
        contract_identifier: &QualifiedContractIdentifier,
        function_name: &'a ClarityName,
        args: &'a [SymbolicExpression],
    ) -> bool {
        self.deps.insert(contract_identifier.clone());
        if let Ok(Some(function_type)) = self
            .analysis_db
            .get_public_function_type(contract_identifier, function_name.as_str())
        {
            match function_type {
                FunctionType::Fixed(fixed_func) => {
                    for (i, arg) in fixed_func.args.iter().enumerate() {
                        if matches!(arg.signature, TypeSignature::TraitReferenceType(_)) {
                            if let Some(Value::Principal(PrincipalData::Contract(contract))) =
                                args[i].match_literal_value()
                            {
                                self.deps.insert(contract.clone());
                            }
                        }
                    }
                }
                _ => (),
            };
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
            let trait_definition = if trait_identifier.contract_identifier == self.current_contract
            {
                self.defined_traits
                    .get(trait_identifier.name.as_str())
                    .unwrap()
                    .clone()
            } else {
                match self.analysis_db.get_defined_trait(
                    &trait_identifier.contract_identifier,
                    trait_identifier.name.as_str(),
                ) {
                    Ok(Some(trait_definition)) => trait_definition,
                    _ => panic!("expected to find trait definition"),
                }
            };
            let function_signature = trait_definition.get(function_name).unwrap();
            for (i, arg) in function_signature.args.iter().enumerate() {
                if matches!(arg, TypeSignature::TraitReferenceType(_)) {
                    if let Some(Value::Principal(PrincipalData::Contract(contract))) =
                        args[i].match_literal_value()
                    {
                        self.deps.insert(contract.clone());
                    }
                }
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
        if let Some(function_type) = self.function_signatures.get(name) {
            match function_type {
                FunctionType::Fixed(fixed_func) => {
                    for (i, arg) in fixed_func.args.iter().enumerate() {
                        if matches!(arg.signature, TypeSignature::TraitReferenceType(_)) {
                            if let Some(Value::Principal(PrincipalData::Contract(contract))) =
                                args[i].match_literal_value()
                            {
                                self.deps.insert(contract.clone());
                            }
                        }
                    }
                }
                _ => (),
            };
        }
        true
    }

    fn visit_use_trait(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        trait_identifier: &TraitIdentifier,
    ) -> bool {
        self.deps
            .insert(trait_identifier.contract_identifier.clone());
        true
    }

    fn visit_impl_trait(
        &mut self,
        expr: &'a SymbolicExpression,
        trait_identifier: &TraitIdentifier,
    ) -> bool {
        self.deps
            .insert(trait_identifier.contract_identifier.clone());
        true
    }
}

impl<'a, 'b> AnalysisPass for DependencyDetector<'a, 'b> {
    fn run_pass(
        contract_analysis: &mut ContractAnalysis,
        analysis_db: &mut AnalysisDatabase,
        annotations: &Vec<Annotation>,
        settings: &Settings,
    ) -> AnalysisResult {
        let mut function_signatures = contract_analysis.public_function_types.clone();
        function_signatures.append(&mut contract_analysis.private_function_types.clone());
        let mut detector = DependencyDetector::new(
            contract_analysis.contract_identifier.clone(),
            analysis_db,
            function_signatures,
            contract_analysis.defined_traits.clone(),
        );
        let res = detector.run(contract_analysis);
        for dep in detector.deps.into_iter() {
            contract_analysis.add_dependency(dep);
        }
        res
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
        match session.formatted_interpretation(
            snippet,
            Some("test".to_string()),
            false,
            false,
            None,
        ) {
            Ok((_, result)) => {
                let (_, _, _, _, ref analysis) = result.contract.unwrap();
                assert_eq!(analysis.dependencies.len(), 0);
            }
            Err(e) => {
                for line in e {
                    println!("{}", line);
                }
                panic!("expected success");
            }
        }
    }

    #[test]
    fn contract_call() {
        let mut session = Session::new(SessionSettings::default());
        let snippet1 = "
(define-public (hello (a int))
    (ok u0)
)"
        .to_string();
        let _ = session
            .formatted_interpretation(snippet1, Some("foo".to_string()), false, false, None)
            .unwrap();

        let snippet = "
(define-public (call-foo)
    (contract-call? .foo hello 4)
)
"
        .to_string();
        match session.formatted_interpretation(
            snippet,
            Some("test".to_string()),
            false,
            false,
            None,
        ) {
            Ok((_, result)) => {
                let (_, _, _, _, ref analysis) = result.contract.unwrap();
                assert_eq!(analysis.dependencies.len(), 1);
                assert_eq!(analysis.dependencies[0].name.as_str(), "foo");
            }
            Err(e) => {
                for line in e {
                    println!("{}", line);
                }
                panic!("expected success");
            }
        }
    }

    // This test is disabled because it is currently not possible to refer to a
    // trait defined in the same contract. An issue has been opened to discuss
    // whether this will be fixed or documented.
    // #[test]
    fn dynamic_contract_call_local_trait() {
        let mut session = Session::new(SessionSettings::default());
        let snippet1 = "
(define-public (hello (a int))
    (ok u0)
)"
        .to_string();
        let _ = session
            .formatted_interpretation(snippet1, Some("bar".to_string()), false, false, None)
            .unwrap();

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
        match session.formatted_interpretation(
            snippet,
            Some("test".to_string()),
            false,
            false,
            None,
        ) {
            Ok((_, result)) => {
                let (_, _, _, _, ref analysis) = result.contract.unwrap();
                assert_eq!(analysis.dependencies.len(), 1);
                assert_eq!(analysis.dependencies[0].name.as_str(), "bar");
            }
            Err(e) => {
                for line in e {
                    println!("{}", line);
                }
                panic!("expected success");
            }
        }
    }

    #[test]
    fn dynamic_contract_call_remote_trait() {
        let mut session = Session::new(SessionSettings::default());
        let snippet1 = "
(define-trait my-trait
    ((hello (int) (response uint uint)))
)
(define-public (hello (a int))
    (ok u0)
)"
        .to_string();
        let _ = session
            .formatted_interpretation(snippet1, Some("bar".to_string()), false, false, None)
            .unwrap();

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
        match session.formatted_interpretation(
            snippet,
            Some("test".to_string()),
            false,
            false,
            None,
        ) {
            Ok((_, result)) => {
                let (_, _, _, _, ref analysis) = result.contract.unwrap();
                assert_eq!(analysis.dependencies.len(), 1);
                assert_eq!(analysis.dependencies[0].name.as_str(), "bar");
            }
            Err(e) => {
                for line in e {
                    println!("{}", line);
                }
                panic!("expected success");
            }
        }
    }

    #[test]
    fn pass_contract_local() {
        let mut session = Session::new(SessionSettings::default());
        let snippet1 = "
(define-public (hello (a int))
    (ok u0)
)"
        .to_string();
        let _ = session
            .formatted_interpretation(snippet1, Some("bar".to_string()), false, false, None)
            .unwrap();

        let snippet2 = "
(define-trait my-trait
    ((hello (int) (response uint uint)))
)"
        .to_string();
        let _ = session
            .formatted_interpretation(snippet2, Some("my-trait".to_string()), false, false, None)
            .unwrap();

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
        match session.formatted_interpretation(
            snippet,
            Some("test".to_string()),
            false,
            false,
            None,
        ) {
            Ok((_, result)) => {
                let (_, _, _, _, ref analysis) = result.contract.unwrap();
                assert_eq!(analysis.dependencies.len(), 2);
                assert_eq!(analysis.dependencies[0].name.as_str(), "bar");
                assert_eq!(analysis.dependencies[1].name.as_str(), "my-trait");
            }
            Err(e) => {
                for line in e {
                    println!("{}", line);
                }
                panic!("expected success");
            }
        }
    }

    #[test]
    fn impl_trait() {
        let mut session = Session::new(SessionSettings::default());
        let snippet1 = "
(define-trait something
    ((hello (int) (response uint uint)))
)"
        .to_string();
        let _ = session
            .formatted_interpretation(snippet1, Some("other".to_string()), false, false, None)
            .unwrap();

        let snippet = "
(impl-trait .other.something)
(define-public (hello (a int))
    (ok u0)
)
"
        .to_string();
        match session.formatted_interpretation(
            snippet,
            Some("test".to_string()),
            false,
            false,
            None,
        ) {
            Ok((_, result)) => {
                let (_, _, _, _, ref analysis) = result.contract.unwrap();
                assert_eq!(analysis.dependencies.len(), 1);
                assert_eq!(analysis.dependencies[0].name.as_str(), "other");
            }
            Err(e) => {
                for line in e {
                    println!("{}", line);
                }
                panic!("expected success");
            }
        }
    }

    #[test]
    fn use_trait() {
        let mut session = Session::new(SessionSettings::default());
        let snippet1 = "
(define-trait something
    ((hello (int) (response uint uint)))
)"
        .to_string();
        let _ = session
            .formatted_interpretation(snippet1, Some("other".to_string()), false, false, None)
            .unwrap();

        let snippet = "
(use-trait my-trait .other.something)
;; FIXME: If there is not a second line here, the interpreter will fail.
;; See https://github.com/hirosystems/clarity-repl/issues/109.
(define-public (foo) (ok true))
"
        .to_string();
        match session.formatted_interpretation(
            snippet,
            Some("test".to_string()),
            false,
            false,
            None,
        ) {
            Ok((_, result)) => {
                let (_, _, _, _, ref analysis) = result.contract.unwrap();
                assert_eq!(analysis.dependencies.len(), 1);
                assert_eq!(analysis.dependencies[0].name.as_str(), "other");
            }
            Err(e) => {
                for line in e {
                    println!("{}", line);
                }
                panic!("expected success");
            }
        }
    }
}

use crate::analysis::ast_visitor::{traverse, ASTVisitor, TypedVar};
use crate::analysis::{AnalysisPass, AnalysisResult};
use crate::clarity::analysis::analysis_db::AnalysisDatabase;
use crate::clarity::analysis::types::ContractAnalysis;
use crate::clarity::diagnostic::{DiagnosableError, Diagnostic, Level};
use crate::clarity::functions::define::DefineFunctions;
use crate::clarity::functions::NativeFunctions;
use crate::clarity::representations::{Span, TraitDefinition};
use crate::clarity::types::{TraitIdentifier, Value};
use crate::clarity::{ClarityName, SymbolicExpression};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

pub struct CheckError;

impl DiagnosableError for CheckError {
    fn message(&self) -> String {
        "Use of potentially unchecked data".to_string()
    }
    fn suggestion(&self) -> Option<String> {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Copy)]
enum Node<'a> {
    Symbol(&'a ClarityName),
    Expr(u64),
}

#[derive(Clone, Debug)]
struct TaintSource<'a> {
    span: Span,
    children: HashSet<Node<'a>>,
}

#[derive(Clone, Debug)]
struct TaintedNode<'a> {
    sources: HashSet<Node<'a>>,
}

pub struct CheckChecker<'a, 'b> {
    db: &'a mut AnalysisDatabase<'b>,
    taint_sources: HashMap<Node<'a>, TaintSource<'a>>,
    tainted_nodes: HashMap<Node<'a>, TaintedNode<'a>>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a, 'b> CheckChecker<'a, 'b> {
    fn new(db: &'a mut AnalysisDatabase<'b>) -> CheckChecker<'a, 'b> {
        Self {
            db,
            taint_sources: HashMap::new(),
            tainted_nodes: HashMap::new(),
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self, contract_analysis: &'a ContractAnalysis) -> AnalysisResult {
        traverse(&mut self, &contract_analysis.expressions);
        Ok(self.diagnostics)
    }

    fn add_taint_source(&mut self, node: Node<'a>, span: Span) {
        let source_node = self.taint_sources.insert(
            node,
            TaintSource {
                span: span,
                children: HashSet::new(),
            },
        );
        let mut sources = HashSet::new();
        sources.insert(node);
        self.tainted_nodes.insert(node, TaintedNode { sources });
    }

    fn add_taint_source_expr(&mut self, expr: &SymbolicExpression) {
        self.add_taint_source(Node::Expr(expr.id), expr.span.clone());
    }

    fn add_taint_source_symbol(&mut self, name: &'a ClarityName, span: Span) {
        self.add_taint_source(Node::Symbol(name), span);
    }

    fn add_tainted_node_to_sources(&mut self, node: Node<'a>, sources: &HashSet<Node<'a>>) {
        for source_node in sources {
            let source = self.taint_sources.get_mut(source_node).unwrap();
            source.children.insert(node);
        }
    }

    fn add_tainted_expr(&mut self, expr: &SymbolicExpression, sources: HashSet<Node<'a>>) {
        let node = Node::Expr(expr.id);
        self.add_tainted_node_to_sources(node, &sources);
        self.tainted_nodes.insert(node, TaintedNode { sources });
    }

    fn add_tainted_symbol(&mut self, name: &'a ClarityName, sources: HashSet<Node<'a>>) {
        let node = Node::Symbol(name);
        self.add_tainted_node_to_sources(node, &sources);
        self.tainted_nodes.insert(node, TaintedNode { sources });
    }

    // If this expression is tainted, add a diagnostic
    fn taint_check(&mut self, expr: &SymbolicExpression) {
        if let Some(tainted) = self.tainted_nodes.get(&Node::Expr(expr.id)) {
            let diagnostic = Diagnostic {
                level: Level::Warning,
                message: "use of potentially unchecked data".to_string(),
                spans: vec![expr.span.clone()],
                suggestion: None,
            };
            self.diagnostics.push(diagnostic);

            // Add a note for each source, ordered by span
            let mut source_spans = vec![];
            for source in &tainted.sources {
                let span = self.taint_sources[source].span.clone();
                let pos = source_spans.binary_search(&span).unwrap_or_else(|e| e);
                source_spans.insert(pos, span);
            }
            for span in source_spans {
                let diagnostic = Diagnostic {
                    level: Level::Note,
                    message: "source of untrusted input here".to_string(),
                    spans: vec![span],
                    suggestion: None,
                };
                self.diagnostics.push(diagnostic);
            }
        }
    }

    // Filter any taint sources used in this expression
    fn filter_taint(&mut self, expr: &SymbolicExpression) {
        let node = Node::Expr(expr.id);
        // Remove this node from the set of tainted nodes
        if let Some(removed_node) = self.tainted_nodes.remove(&node) {
            // Remove its sources of taint
            for source_node in &removed_node.sources {
                let source = self.taint_sources.remove(&source_node).unwrap();
                self.tainted_nodes.remove(&source_node);
                // Remove each taint source from its children
                for child in &source.children {
                    if let Some(mut child_node) = self.tainted_nodes.remove(child) {
                        child_node.sources.remove(&source_node);
                        // If the child is still tainted (by another source), add it back to the set
                        if child_node.sources.len() > 0 {
                            self.tainted_nodes.insert(child.clone(), child_node);
                        }
                    }
                }
            }
        }
    }
}

impl<'a> ASTVisitor<'a> for CheckChecker<'a, '_> {
    fn traverse_define_public(
        &mut self,
        expr: &SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        self.taint_sources.clear();
        self.tainted_nodes.clear();

        // Upon entering a public function, all parameters are tainted
        if let Some(params) = parameters {
            for param in params {
                self.add_taint_source(Node::Symbol(param.name), param.decl_span);
            }
        }
        self.traverse_expr(body)
    }

    fn traverse_if(
        &mut self,
        expr: &SymbolicExpression,
        cond: &'a SymbolicExpression,
        then_expr: &'a SymbolicExpression,
        else_expr: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(cond);
        self.filter_taint(cond);

        self.traverse_expr(then_expr);
        self.traverse_expr(else_expr);
        true
    }

    fn traverse_lazy_logical(
        &mut self,
        expr: &SymbolicExpression,
        function: NativeFunctions,
        operands: &'a [SymbolicExpression],
    ) -> bool {
        for operand in operands {
            self.traverse_expr(operand);
            self.filter_taint(operand);
        }
        true
    }

    fn traverse_let(
        &mut self,
        expr: &SymbolicExpression,
        bindings: &HashMap<&'a ClarityName, &'a SymbolicExpression>,
        body: &'a [SymbolicExpression],
    ) -> bool {
        for (name, val) in bindings {
            if !self.traverse_expr(val) {
                return false;
            }
            if let Some(tainted) = self.tainted_nodes.get(&Node::Expr(val.id)) {
                let sources = tainted.sources.clone();
                // If the expression is tainted, add it to the map
                self.add_taint_source_symbol(name, expr.span.clone());
                self.add_tainted_symbol(name, sources);
            }
        }

        for expr in body {
            if !self.traverse_expr(expr) {
                return false;
            }
        }

        // The let expression returns the value of the last body expression,
        // so use that to determine if the let itself is tainted.
        if let Some(last_expr) = body.last() {
            if let Some(tainted) = self.tainted_nodes.get(&Node::Expr(last_expr.id)) {
                let sources = tainted.sources.clone();
                self.add_tainted_expr(expr, sources);
            }
        }

        for (name, val) in bindings {
            // Outside the scope of the let, remove this name
            let node = Node::Symbol(name);
            self.taint_sources.remove(&node);
            self.tainted_nodes.remove(&node);
        }
        true
    }

    fn visit_asserts(
        &mut self,
        expr: &SymbolicExpression,
        cond: &'a SymbolicExpression,
        thrown: &'a SymbolicExpression,
    ) -> bool {
        self.filter_taint(cond);
        true
    }

    fn visit_atom(&mut self, expr: &SymbolicExpression, atom: &'a ClarityName) -> bool {
        if let Some(tainted) = self.tainted_nodes.get(&Node::Symbol(atom)) {
            let sources = tainted.sources.clone();
            self.add_tainted_expr(expr, sources);
        }
        true
    }

    fn visit_list(&mut self, expr: &SymbolicExpression, list: &[SymbolicExpression]) -> bool {
        let mut sources = HashSet::new();

        // For expressions with unique properties, tainted-ness is handled
        // inside the traverse_* method.
        if let Some((function_name, args)) = list.split_first() {
            if let Some(function_name) = function_name.match_atom() {
                if let Some(define_function) = DefineFunctions::lookup_by_name(function_name) {
                    return true;
                } else if let Some(native_function) = NativeFunctions::lookup_by_name(function_name)
                {
                    use crate::clarity::functions::NativeFunctions::*;
                    match native_function {
                        Let => return true,
                        _ => {}
                    }
                }
            }
        }

        // For other nodes, if any of the children are tainted, the node is
        // tainted.
        for child in list {
            if let Some(tainted) = self.tainted_nodes.get(&Node::Expr(child.id)) {
                sources.extend(tainted.sources.clone());
            }
        }
        if sources.len() > 0 {
            self.add_tainted_expr(expr, sources);
        }
        true
    }

    fn visit_stx_burn(
        &mut self,
        expr: &SymbolicExpression,
        amount: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
    ) -> bool {
        // Input from the sender can be used un-checked to interact with the
        // sender's assets. The sender is protected by post-conditions.
        if sender.match_tx_sender() {
            return true;
        }
        self.taint_check(amount);
        self.taint_check(sender);
        true
    }

    fn visit_stx_transfer(
        &mut self,
        expr: &SymbolicExpression,
        amount: &SymbolicExpression,
        sender: &SymbolicExpression,
        recipient: &SymbolicExpression,
    ) -> bool {
        // Input from the sender can be used un-checked to interact with the
        // sender's assets. The sender is protected by post-conditions.
        if sender.match_tx_sender() {
            return true;
        }
        self.taint_check(amount);
        self.taint_check(sender);
        self.taint_check(recipient);
        true
    }

    fn visit_ft_burn(
        &mut self,
        expr: &SymbolicExpression,
        token: &'a ClarityName,
        amount: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
    ) -> bool {
        // Input from the sender can be used un-checked to interact with the
        // sender's assets. The sender is protected by post-conditions.
        if sender.match_tx_sender() {
            return true;
        }
        self.taint_check(amount);
        self.taint_check(sender);
        true
    }

    fn visit_ft_transfer(
        &mut self,
        expr: &SymbolicExpression,
        token: &'a ClarityName,
        amount: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        // Input from the sender can be used un-checked to interact with the
        // sender's assets. The sender is protected by post-conditions.
        if sender.match_tx_sender() {
            return true;
        }
        self.taint_check(amount);
        self.taint_check(sender);
        self.taint_check(recipient);
        true
    }

    fn visit_ft_mint(
        &mut self,
        expr: &SymbolicExpression,
        token: &'a ClarityName,
        amount: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        self.taint_check(amount);
        self.taint_check(recipient);
        true
    }

    fn visit_nft_burn(
        &mut self,
        expr: &SymbolicExpression,
        token: &'a ClarityName,
        identifier: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
    ) -> bool {
        // Input from the sender can be used un-checked to interact with the
        // sender's assets. The sender is protected by post-conditions.
        if sender.match_tx_sender() {
            return true;
        }
        self.taint_check(identifier);
        self.taint_check(sender);
        true
    }

    fn visit_nft_transfer(
        &mut self,
        expr: &SymbolicExpression,
        token: &'a ClarityName,
        identifier: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        // Input from the sender can be used un-checked to interact with the
        // sender's assets. The sender is protected by post-conditions.
        if sender.match_tx_sender() {
            return true;
        }
        self.taint_check(identifier);
        self.taint_check(sender);
        self.taint_check(recipient);
        true
    }

    fn visit_nft_mint(
        &mut self,
        expr: &SymbolicExpression,
        token: &'a ClarityName,
        identifier: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        self.taint_check(identifier);
        self.taint_check(recipient);
        true
    }

    fn visit_var_set(
        &mut self,
        expr: &SymbolicExpression,
        name: &'a ClarityName,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.taint_check(value);
        true
    }

    fn visit_map_set(
        &mut self,
        expr: &SymbolicExpression,
        name: &'a ClarityName,
        key: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
        value: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        for (_, key_val) in key {
            self.taint_check(key_val);
        }
        for (_, val_val) in value {
            self.taint_check(val_val);
        }
        true
    }

    fn visit_map_insert(
        &mut self,
        expr: &SymbolicExpression,
        name: &'a ClarityName,
        key: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
        value: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        for (_, key_val) in key {
            self.taint_check(key_val);
        }
        for (_, val_val) in value {
            self.taint_check(val_val);
        }
        true
    }

    fn visit_map_delete(
        &mut self,
        expr: &SymbolicExpression,
        name: &'a ClarityName,
        key: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        for (_, val) in key {
            self.taint_check(val);
        }
        true
    }
}

impl AnalysisPass for CheckChecker<'_, '_> {
    fn run_pass(
        contract_analysis: &mut ContractAnalysis,
        analysis_db: &mut AnalysisDatabase,
    ) -> AnalysisResult {
        let tc = CheckChecker::new(analysis_db);
        tc.run(contract_analysis)
    }
}

impl<'a> SymbolicExpression {
    fn match_tx_sender(&'a self) -> bool {
        if let Some(name) = self.match_atom() {
            if name.as_str() == "tx-sender" {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repl::session::Session;
    use crate::repl::SessionSettings;

    #[test]
    fn define_public() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (tainted (amount uint))
    (stx-transfer? amount (as-contract tx-sender) tx-sender)
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:3:20: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "    (stx-transfer? amount (as-contract tx-sender) tx-sender)"
                );
                assert_eq!(output[2], "                   ^~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:2:26: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (tainted (amount uint))");
                assert_eq!(output[5], "                         ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn expr_tainted() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (expr-tainted (amount uint))
    (stx-transfer? (+ u10 amount) (as-contract tx-sender) tx-sender)
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:3:20: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "    (stx-transfer? (+ u10 amount) (as-contract tx-sender) tx-sender)"
                );
                assert_eq!(output[2], "                   ^~~~~~~~~~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:2:31: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (expr-tainted (amount uint))");
                assert_eq!(output[5], "                              ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn let_tainted() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (let-tainted (amount uint))
    (let ((x amount))
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:24: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "        (stx-transfer? x (as-contract tx-sender) tx-sender)"
                );
                assert_eq!(output[2], "                       ^");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:2:30: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (let-tainted (amount uint))");
                assert_eq!(output[5], "                             ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn filtered() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (filtered (amount uint))
    (begin
        (asserts! (< amount u100) (err u100))
        (stx-transfer? amount (as-contract tx-sender) tx-sender)
    )
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn filtered_expr() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (filtered-expr (amount uint))
    (begin
        (asserts! (< (+ amount u10) u100) (err u100))
        (stx-transfer? amount (as-contract tx-sender) tx-sender)
    )
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn let_filtered() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (let-filtered (amount uint))
    (let ((x amount))
        (asserts! (< x u100) (err u100))
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn let_filtered_parent() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (let-filtered-parent (amount uint))
    (let ((x amount))
        (asserts! (< amount u100) (err u100))
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn let_tainted_twice() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (let-tainted-twice (amount1 uint) (amount2 uint))
    (let ((x (+ amount1 amount2)))
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 10);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:24: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "        (stx-transfer? x (as-contract tx-sender) tx-sender)"
                );
                assert_eq!(output[2], "                       ^");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:2:36: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[4],
                    "(define-public (let-tainted-twice (amount1 uint) (amount2 uint))"
                );
                assert_eq!(output[5], "                                   ^~~~~~~");
                assert_eq!(
                    output[6],
                    format!(
                        "checker:2:51: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[7],
                    "(define-public (let-tainted-twice (amount1 uint) (amount2 uint))"
                );
                assert_eq!(
                    output[8],
                    "                                                  ^~~~~~~"
                );
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn let_tainted_twice_filtered_once() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (let-tainted-twice-filtered-once (amount1 uint) (amount2 uint))
    (let ((x (+ amount1 amount2)))
        (asserts! (< amount1 u100) (err u100))
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:5:24: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "        (stx-transfer? x (as-contract tx-sender) tx-sender)"
                );
                assert_eq!(output[2], "                       ^");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:2:65: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (let-tainted-twice-filtered-once (amount1 uint) (amount2 uint))");
                assert_eq!(
                    output[5],
                    "                                                                ^~~~~~~"
                );
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn let_tainted_twice_filtered_twice() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (let-tainted-twice-filtered-twice (amount1 uint) (amount2 uint))
    (let ((x (+ amount1 amount2)))
        (asserts! (< amount1 u100) (err u100))
        (asserts! (< amount2 u100) (err u101))
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn let_tainted_twice_filtered_together() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (let-tainted-twice-filtered-together (amount1 uint) (amount2 uint))
    (let ((x (+ amount1 amount2)))
        (asserts! (< (+ amount1 amount2) u100) (err u100))
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn if_filter() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (if-filter (amount uint))
    (stx-transfer? (if (< amount u100) amount u100) (as-contract tx-sender) tx-sender)
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn if_not_filtered() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (if-not-filtered (amount uint))
    (stx-transfer? (if (< u50 u100) amount u100) (as-contract tx-sender) tx-sender)
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:3:20: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(output[1], "    (stx-transfer? (if (< u50 u100) amount u100) (as-contract tx-sender) tx-sender)");
                assert_eq!(
                    output[2],
                    "                   ^~~~~~~~~~~~~~~~~~~~~~~~~~~~~"
                );
                assert_eq!(
                    output[3],
                    format!(
                        "checker:2:34: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (if-not-filtered (amount uint))");
                assert_eq!(output[5], "                                 ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn and_tainted() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (and-tainted (amount uint))
    (ok (and
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
    ))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:38: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(output[1], "        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))");
                assert_eq!(output[2], "                                     ^~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:2:30: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (and-tainted (amount uint))");
                assert_eq!(output[5], "                             ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn and_filter() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (and-filter (amount uint))
    (ok (and
        (< amount u100)
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
    ))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn and_filter_after() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (and-filter-after (amount uint))
    (ok (and
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
        (< amount u100)
    ))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:38: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(output[1], "        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))");
                assert_eq!(output[2], "                                     ^~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:2:35: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (and-filter-after (amount uint))");
                assert_eq!(output[5], "                                  ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn or_tainted() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (or-tainted (amount uint))
    (ok (or
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
    ))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:38: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(output[1], "        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))");
                assert_eq!(output[2], "                                     ^~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:2:29: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (or-tainted (amount uint))");
                assert_eq!(output[5], "                            ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn or_filter() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (or-filter (amount uint))
    (ok (or
        (< amount u100)
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
    ))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn or_filter_after() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (or-filter-after (amount uint))
    (ok (or
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
        (< amount u100)
    ))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:38: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(output[1], "        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))");
                assert_eq!(output[2], "                                     ^~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:2:34: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (or-filter-after (amount uint))");
                assert_eq!(output[5], "                                 ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn stx_burn_senders() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (stx-burn-senders (amount uint))
    (stx-burn? amount tx-sender)
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_stx_burn() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (tainted-stx-burn (amount uint))
    (stx-burn? amount (as-contract tx-sender))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:3:16: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(output[1], "    (stx-burn? amount (as-contract tx-sender))");
                assert_eq!(output[2], "               ^~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:2:35: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (tainted-stx-burn (amount uint))");
                assert_eq!(output[5], "                                  ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn stx_transfer_senders() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-public (stx-transfer-senders (amount uint) (recipient principal))
    (stx-transfer? amount tx-sender recipient)
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_ft_burn() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-fungible-token stackaroo)
(define-public (tainted-ft-burn (amount uint))
    (ft-burn? stackaroo amount (as-contract tx-sender))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:25: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "    (ft-burn? stackaroo amount (as-contract tx-sender))"
                );
                assert_eq!(output[2], "                        ^~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:3:34: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (tainted-ft-burn (amount uint))");
                assert_eq!(output[5], "                                 ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn ft_burn_senders() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-fungible-token stackaroo)
(define-public (ft-burn-senders (amount uint))
    (ft-burn? stackaroo amount tx-sender)
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_ft_transfer() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-fungible-token stackaroo)
(define-public (tainted-ft-transfer (amount uint))
    (ft-transfer? stackaroo amount (as-contract tx-sender) tx-sender)
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:29: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "    (ft-transfer? stackaroo amount (as-contract tx-sender) tx-sender)"
                );
                assert_eq!(output[2], "                            ^~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:3:38: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[4],
                    "(define-public (tainted-ft-transfer (amount uint))"
                );
                assert_eq!(output[5], "                                     ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn ft_transfer_senders() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-fungible-token stackaroo)
(define-public (ft-transfer-senders (amount uint) (recipient principal))
    (ft-transfer? stackaroo amount tx-sender recipient)
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_ft_mint() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-fungible-token stackaroo)
(define-public (tainted-ft-mint (amount uint))
    (ft-mint? stackaroo amount (as-contract tx-sender))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:25: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "    (ft-mint? stackaroo amount (as-contract tx-sender))"
                );
                assert_eq!(output[2], "                        ^~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:3:34: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (tainted-ft-mint (amount uint))");
                assert_eq!(output[5], "                                 ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_nft_burn() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-non-fungible-token stackaroo uint)
(define-public (tainted-nft-burn (identifier uint))
    (nft-burn? stackaroo identifier (as-contract tx-sender))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:26: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "    (nft-burn? stackaroo identifier (as-contract tx-sender))"
                );
                assert_eq!(output[2], "                         ^~~~~~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:3:35: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[4],
                    "(define-public (tainted-nft-burn (identifier uint))"
                );
                assert_eq!(output[5], "                                  ^~~~~~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn nft_burn_senders() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-non-fungible-token stackaroo uint)
(define-public (nft-burn-senders (identifier uint))
    (nft-burn? stackaroo identifier tx-sender)
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_nft_transfer() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-non-fungible-token stackaroo uint)
(define-public (tainted-nft-transfer (identifier uint))
    (nft-transfer? stackaroo identifier (as-contract tx-sender) tx-sender)
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:30: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "    (nft-transfer? stackaroo identifier (as-contract tx-sender) tx-sender)"
                );
                assert_eq!(output[2], "                             ^~~~~~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:3:39: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[4],
                    "(define-public (tainted-nft-transfer (identifier uint))"
                );
                assert_eq!(
                    output[5],
                    "                                      ^~~~~~~~~~"
                );
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn nft_transfer_senders() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-non-fungible-token stackaroo uint)
(define-public (nft-transfer-senders (identifier uint) (recipient principal))
    (nft-transfer? stackaroo identifier tx-sender recipient)
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((_, result)) => {
                assert_eq!(result.diagnostics.len(), 0);
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_nft_mint() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-non-fungible-token stackaroo uint)
(define-public (tainted-nft-mint (identifier uint))
    (nft-mint? stackaroo identifier (as-contract tx-sender))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:26: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "    (nft-mint? stackaroo identifier (as-contract tx-sender))"
                );
                assert_eq!(output[2], "                         ^~~~~~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:3:35: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[4],
                    "(define-public (tainted-nft-mint (identifier uint))"
                );
                assert_eq!(output[5], "                                  ^~~~~~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_var_set() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-data-var myvar uint u0)
(define-public (tainted-var-set (amount uint))
    (ok (var-set myvar amount))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:24: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(output[1], "    (ok (var-set myvar amount))");
                assert_eq!(output[2], "                       ^~~~~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:3:34: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (tainted-var-set (amount uint))");
                assert_eq!(output[5], "                                 ^~~~~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_map_set() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-map mymap { key-name-1: uint } { val-name-1: int })
(define-public (tainted-map-set (key uint) (value int))
    (ok (map-set mymap {key-name-1: key} {val-name-1: value}))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 13);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:37: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "    (ok (map-set mymap {key-name-1: key} {val-name-1: value}))"
                );
                assert_eq!(output[2], "                                    ^~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:3:34: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[4],
                    "(define-public (tainted-map-set (key uint) (value int))"
                );
                assert_eq!(output[5], "                                 ^~~");
                assert_eq!(
                    output[6],
                    format!(
                        "checker:4:55: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[7],
                    "    (ok (map-set mymap {key-name-1: key} {val-name-1: value}))"
                );
                assert_eq!(
                    output[8],
                    "                                                      ^~~~~"
                );
                assert_eq!(
                    output[9],
                    format!(
                        "checker:3:45: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[10],
                    "(define-public (tainted-map-set (key uint) (value int))"
                );
                assert_eq!(
                    output[11],
                    "                                            ^~~~~"
                );
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_map_set2() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-map mymap uint int)
(define-public (tainted-map-set (key uint) (value int))
    (ok (map-set mymap key value))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 13);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:24: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(output[1], "    (ok (map-set mymap key value))");
                assert_eq!(output[2], "                       ^~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:3:34: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[4],
                    "(define-public (tainted-map-set (key uint) (value int))"
                );
                assert_eq!(output[5], "                                 ^~~");
                assert_eq!(
                    output[6],
                    format!(
                        "checker:4:28: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(output[7], "    (ok (map-set mymap key value))");
                assert_eq!(output[8], "                           ^~~~~");
                assert_eq!(
                    output[9],
                    format!(
                        "checker:3:45: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[10],
                    "(define-public (tainted-map-set (key uint) (value int))"
                );
                assert_eq!(
                    output[11],
                    "                                            ^~~~~"
                );
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_map_insert() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-map mymap { key-name-1: uint } { val-name-1: int })
(define-public (tainted-map-insert (key uint) (value int))
    (ok (map-insert mymap {key-name-1: key} {val-name-1: value}))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 13);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:40: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[1],
                    "    (ok (map-insert mymap {key-name-1: key} {val-name-1: value}))"
                );
                assert_eq!(output[2], "                                       ^~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:3:37: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[4],
                    "(define-public (tainted-map-insert (key uint) (value int))"
                );
                assert_eq!(output[5], "                                    ^~~");
                assert_eq!(
                    output[6],
                    format!(
                        "checker:4:58: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(
                    output[7],
                    "    (ok (map-insert mymap {key-name-1: key} {val-name-1: value}))"
                );
                assert_eq!(
                    output[8],
                    "                                                         ^~~~~"
                );
                assert_eq!(
                    output[9],
                    format!(
                        "checker:3:48: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[10],
                    "(define-public (tainted-map-insert (key uint) (value int))"
                );
                assert_eq!(
                    output[11],
                    "                                               ^~~~~"
                );
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_map_insert2() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-map mymap uint int)
(define-public (tainted-map-insert (key uint) (value int))
    (ok (map-insert mymap key value))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 13);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:27: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(output[1], "    (ok (map-insert mymap key value))");
                assert_eq!(output[2], "                          ^~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:3:37: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[4],
                    "(define-public (tainted-map-insert (key uint) (value int))"
                );
                assert_eq!(output[5], "                                    ^~~");
                assert_eq!(
                    output[6],
                    format!(
                        "checker:4:31: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(output[7], "    (ok (map-insert mymap key value))");
                assert_eq!(output[8], "                              ^~~~~");
                assert_eq!(
                    output[9],
                    format!(
                        "checker:3:48: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(
                    output[10],
                    "(define-public (tainted-map-insert (key uint) (value int))"
                );
                assert_eq!(
                    output[11],
                    "                                               ^~~~~"
                );
            }
            _ => panic!("Expected successful interpretation"),
        };
    }

    #[test]
    fn tainted_map_delete() {
        let mut settings = SessionSettings::default();
        settings.analysis = vec!["check-checker".to_string()];
        let mut session = Session::new(settings);
        let snippet = "
(define-map mymap { key-name-1: uint } { val-name-1: int })
(define-public (tainted-map-delete (key uint))
    (ok (map-delete mymap {key-name-1: key}))
)
"
        .to_string();
        match session.formatted_interpretation(snippet, Some("checker".to_string()), false, None) {
            Ok((output, _)) => {
                assert_eq!(output.len(), 7);
                assert_eq!(
                    output[0],
                    format!(
                        "checker:4:40: {}: use of potentially unchecked data",
                        yellow!("warning")
                    )
                );
                assert_eq!(output[1], "    (ok (map-delete mymap {key-name-1: key}))");
                assert_eq!(output[2], "                                       ^~~");
                assert_eq!(
                    output[3],
                    format!(
                        "checker:3:37: {}: source of untrusted input here",
                        blue!("note")
                    )
                );
                assert_eq!(output[4], "(define-public (tainted-map-delete (key uint))");
                assert_eq!(output[5], "                                    ^~~");
            }
            _ => panic!("Expected successful interpretation"),
        };
    }
}

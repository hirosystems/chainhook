use crate::analysis::ast_visitor::{traverse, ASTVisitor, TypedVar};
use crate::analysis::{AnalysisPass, AnalysisResult};
use crate::clarity::analysis::analysis_db::AnalysisDatabase;
use crate::clarity::analysis::types::ContractAnalysis;
use crate::clarity::diagnostic::{DiagnosableError, Diagnostic, Level};
use crate::clarity::functions::NativeFunctions;
use crate::clarity::representations::{Span, TraitDefinition};
use crate::clarity::types::{TraitIdentifier, Value};
use crate::clarity::{ClarityName, SymbolicExpression};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

pub struct TaintError;

impl DiagnosableError for TaintError {
    fn message(&self) -> String {
        "Use of potentially tainted data".to_string()
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

pub struct TaintChecker<'a, 'b> {
    db: &'a mut AnalysisDatabase<'b>,
    taint_sources: HashMap<Node<'a>, TaintSource<'a>>,
    tainted_nodes: HashMap<Node<'a>, TaintedNode<'a>>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a, 'b> TaintChecker<'a, 'b> {
    fn new(db: &'a mut AnalysisDatabase<'b>) -> TaintChecker<'a, 'b> {
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
                message: "use of potentially tainted data".to_string(),
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
                    message: "source of taint here".to_string(),
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

impl<'a> ASTVisitor<'a> for TaintChecker<'a, '_> {
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
        let mut sources /*: HashSet<Node<'a>>*/ = HashSet::new();
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

impl AnalysisPass for TaintChecker<'_, '_> {
    fn run_pass(
        contract_analysis: &mut ContractAnalysis,
        analysis_db: &mut AnalysisDatabase,
    ) -> AnalysisResult {
        let tc = TaintChecker::new(analysis_db);
        tc.run(contract_analysis)
    }
}

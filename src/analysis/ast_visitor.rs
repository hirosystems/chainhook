use crate::clarity::functions::define::DefineFunctions;
use crate::clarity::functions::NativeFunctions;
use crate::clarity::representations::SymbolicExpressionType::*;
use crate::clarity::representations::{Span, TraitDefinition};
use crate::clarity::types::{PrincipalData, QualifiedContractIdentifier, TraitIdentifier, Value};
use crate::clarity::{ClarityName, SymbolicExpression, SymbolicExpressionType};
use std::collections::HashMap;

#[derive(Clone)]
pub struct TypedVar<'a> {
    pub name: &'a ClarityName,
    pub type_expr: &'a SymbolicExpression,
    pub decl_span: Span,
}

pub trait ASTVisitor<'a> {
    fn traverse_expr(&mut self, expr: &'a SymbolicExpression) -> bool {
        match &expr.expr {
            AtomValue(value) => self.visit_atom_value(expr, value),
            Atom(name) => self.visit_atom(expr, name),
            List(exprs) => self.traverse_list(expr, &exprs),
            LiteralValue(value) => self.visit_literal_value(expr, value),
            Field(field) => self.visit_field(expr, field),
            TraitReference(name, trait_def) => self.visit_trait_reference(expr, name, trait_def),
        }
    }

    // AST level traverse/visit methods

    fn traverse_list(
        &mut self,
        expr: &'a SymbolicExpression,
        list: &'a [SymbolicExpression],
    ) -> bool {
        let mut rv = true;
        if let Some((function_name, args)) = list.split_first() {
            if let Some(function_name) = function_name.match_atom() {
                if let Some(define_function) = DefineFunctions::lookup_by_name(function_name) {
                    rv = match define_function {
                        DefineFunctions::Constant => self.traverse_define_constant(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1],
                        ),
                        DefineFunctions::PrivateFunction => {
                            match args[0].match_list() {
                                Some(signature) => {
                                    let name = signature[0].match_atom().unwrap();
                                    let params = match signature.len() {
                                        1 => None,
                                        _ => match_pairs(&signature[1..]),
                                    };
                                    self.traverse_define_private(expr, name, params, &args[1]);
                                }
                                _ => {
                                    false;
                                }
                            }
                            true
                        }
                        DefineFunctions::ReadOnlyFunction => match args[0].match_list() {
                            Some(signature) => {
                                let name = signature[0].match_atom().unwrap();
                                let params = match signature.len() {
                                    1 => None,
                                    _ => match_pairs(&signature[1..]),
                                };
                                self.traverse_define_read_only(expr, name, params, &args[1])
                            }
                            _ => false,
                        },
                        DefineFunctions::PublicFunction => match args[0].match_list() {
                            Some(signature) => {
                                let name = signature[0].match_atom().unwrap();
                                let params = match signature.len() {
                                    1 => None,
                                    _ => match_pairs(&signature[1..]),
                                };
                                self.traverse_define_public(expr, name, params, &args[1])
                            }
                            _ => false,
                        },
                        DefineFunctions::NonFungibleToken => {
                            self.traverse_define_nft(expr, args[0].match_atom().unwrap(), &args[1])
                        }
                        DefineFunctions::FungibleToken => self.traverse_define_ft(
                            expr,
                            args[0].match_atom().unwrap(),
                            args.get(1),
                        ),
                        DefineFunctions::Map => self.traverse_define_map(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1],
                            &args[2],
                        ),
                        DefineFunctions::PersistedVariable => self.traverse_define_data_var(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1],
                            &args[2],
                        ),
                        DefineFunctions::Trait => self.traverse_define_trait(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1..],
                        ),
                        DefineFunctions::UseTrait => self.traverse_use_trait(
                            expr,
                            args[0].match_atom().unwrap(),
                            args[1].match_field().unwrap(),
                        ),
                        DefineFunctions::ImplTrait => {
                            self.traverse_impl_trait(expr, args[0].match_field().unwrap())
                        }
                    };
                } else if let Some(native_function) = NativeFunctions::lookup_by_name(function_name)
                {
                    use crate::clarity::functions::NativeFunctions::*;
                    rv = match native_function {
                        Add | Subtract | Multiply | Divide | Modulo | Power | Sqrti | Log2 => {
                            self.traverse_arithmetic(expr, native_function, &args)
                        }
                        BitwiseXOR => {
                            self.traverse_binary_bitwise(expr, native_function, &args[0], &args[1])
                        }
                        CmpLess | CmpLeq | CmpGreater | CmpGeq | Equals => {
                            self.traverse_comparison(expr, native_function, &args)
                        }
                        And | Or => self.traverse_lazy_logical(expr, native_function, &args),
                        Not => self.traverse_logical(expr, native_function, &args),
                        ToInt | ToUInt => self.traverse_int_cast(expr, &args[0]),
                        If => self.traverse_if(expr, &args[0], &args[1], &args[2]),
                        Let => {
                            let bindings = args[0].match_pairs().unwrap();
                            self.traverse_let(expr, &bindings, &args[1..])
                        }
                        ElementAt => self.traverse_element_at(expr, &args[0], &args[1]),
                        IndexOf => self.traverse_index_of(expr, &args[0], &args[1]),
                        Map => {
                            let name = args[0].match_atom().unwrap();
                            self.traverse_map(expr, name, &args[1..])
                        }
                        Fold => {
                            let name = args[0].match_atom().unwrap();
                            self.traverse_fold(expr, name, &args[1], &args[2])
                        }
                        Append => self.traverse_append(expr, &args[0], &args[1]),
                        Concat => self.traverse_concat(expr, &args[0], &args[1]),
                        AsMaxLen => match args[1].match_literal_value() {
                            Some(Value::UInt(length)) => {
                                self.traverse_as_max_len(expr, &args[0], *length)
                            }
                            _ => panic!("check_checker: invalid length expression in as-max-len?"),
                        },
                        Len => self.traverse_len(expr, &args[0]),
                        ListCons => self.traverse_list_cons(expr, &args),
                        FetchVar => self.traverse_var_get(expr, args[0].match_atom().unwrap()),
                        SetVar => {
                            self.traverse_var_set(expr, args[0].match_atom().unwrap(), &args[1])
                        }
                        FetchEntry => {
                            let name = args[0].match_atom().unwrap();
                            let key = args[1].match_tuple().unwrap_or_else(|| {
                                let mut tuple_map = HashMap::new();
                                tuple_map.insert(None, &args[1]);
                                tuple_map
                            });
                            self.traverse_map_get(expr, name, &key)
                        }
                        SetEntry => {
                            let name = args[0].match_atom().unwrap();
                            let key = args[1].match_tuple().unwrap_or_else(|| {
                                let mut tuple_map = HashMap::new();
                                tuple_map.insert(None, &args[1]);
                                tuple_map
                            });
                            let value = args[2].match_tuple().unwrap_or_else(|| {
                                let mut tuple_map = HashMap::new();
                                tuple_map.insert(None, &args[2]);
                                tuple_map
                            });
                            self.traverse_map_set(expr, name, &key, &value)
                        }
                        InsertEntry => {
                            let name = args[0].match_atom().unwrap();
                            let key = args[1].match_tuple().unwrap_or_else(|| {
                                let mut tuple_map = HashMap::new();
                                tuple_map.insert(None, &args[1]);
                                tuple_map
                            });
                            let value = args[2].match_tuple().unwrap_or_else(|| {
                                let mut tuple_map = HashMap::new();
                                tuple_map.insert(None, &args[2]);
                                tuple_map
                            });
                            self.traverse_map_insert(expr, name, &key, &value)
                        }
                        DeleteEntry => {
                            let name = args[0].match_atom().unwrap();
                            let key = args[1].match_tuple().unwrap_or_else(|| {
                                let mut tuple_map = HashMap::new();
                                tuple_map.insert(None, &args[1]);
                                tuple_map
                            });
                            self.traverse_map_delete(expr, name, &key)
                        }
                        TupleCons => self.traverse_tuple(expr, &expr.match_tuple().unwrap()),
                        TupleGet => {
                            self.traverse_get(expr, args[0].match_atom().unwrap(), &args[1])
                        }
                        TupleMerge => self.traverse_merge(expr, &args[0], &args[1]),
                        Begin => self.traverse_begin(expr, &args),
                        Hash160 | Sha256 | Sha512 | Sha512Trunc256 | Keccak256 => {
                            self.traverse_hash(expr, native_function, &args[0])
                        }
                        Secp256k1Recover => {
                            self.traverse_secp256k1_recover(expr, &args[0], &args[1])
                        }
                        Secp256k1Verify => {
                            self.traverse_secp256k1_verify(expr, &args[0], &args[1], &args[2])
                        }
                        Print => self.traverse_print(expr, &args[0]),
                        ContractCall => {
                            let function_name = args[1].match_atom().unwrap();
                            if let SymbolicExpressionType::LiteralValue(Value::Principal(
                                PrincipalData::Contract(ref contract_identifier),
                            )) = &args[0].expr
                            {
                                self.traverse_static_contract_call(
                                    expr,
                                    contract_identifier,
                                    function_name,
                                    &args[2..],
                                )
                            } else {
                                self.traverse_dynamic_contract_call(
                                    expr,
                                    &args[0],
                                    function_name,
                                    &args[2..],
                                )
                            }
                        }
                        AsContract => self.traverse_as_contract(expr, &args[0]),
                        ContractOf => self.traverse_contract_of(expr, &args[0]),
                        PrincipalOf => self.traverse_principal_of(expr, &args[0]),
                        AtBlock => self.traverse_at_block(expr, &args[0], &args[1]),
                        GetBlockInfo => self.traverse_get_block_info(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1],
                        ),
                        ConsError => self.traverse_err(expr, &args[0]),
                        ConsOkay => self.traverse_ok(expr, &args[0]),
                        ConsSome => self.traverse_some(expr, &args[0]),
                        DefaultTo => self.traverse_default_to(expr, &args[0], &args[1]),
                        Asserts => self.traverse_asserts(expr, &args[0], &args[1]),
                        UnwrapRet => self.traverse_unwrap(expr, &args[0], &args[1]),
                        Unwrap => self.traverse_unwrap_panic(expr, &args[0]),
                        IsOkay => self.traverse_is_ok(expr, &args[0]),
                        IsNone => self.traverse_is_none(expr, &args[0]),
                        IsErr => self.traverse_is_err(expr, &args[0]),
                        IsSome => self.traverse_is_some(expr, &args[0]),
                        Filter => {
                            self.traverse_filter(expr, args[0].match_atom().unwrap(), &args[1])
                        }
                        UnwrapErrRet => self.traverse_unwrap_err(expr, &args[0], &args[1]),
                        UnwrapErr => self.traverse_unwrap_err(expr, &args[0], &args[1]),
                        Match => {
                            if args.len() == 4 {
                                self.traverse_match_option(
                                    expr,
                                    &args[0],
                                    args[1].match_atom().unwrap(),
                                    &args[2],
                                    &args[3],
                                )
                            } else {
                                self.traverse_match_response(
                                    expr,
                                    &args[0],
                                    args[1].match_atom().unwrap(),
                                    &args[2],
                                    args[3].match_atom().unwrap(),
                                    &args[4],
                                )
                            }
                        }
                        TryRet => self.traverse_try(expr, &args[0]),
                        StxBurn => self.traverse_stx_burn(expr, &args[0], &args[1]),
                        StxTransfer => {
                            self.traverse_stx_transfer(expr, &args[0], &args[1], &args[2])
                        }
                        GetStxBalance => self.traverse_stx_get_balance(expr, &args[0]),
                        BurnToken => self.traverse_ft_burn(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1],
                            &args[2],
                        ),
                        TransferToken => self.traverse_ft_transfer(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1],
                            &args[2],
                            &args[3],
                        ),
                        GetTokenBalance => self.traverse_ft_get_balance(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1],
                        ),
                        GetTokenSupply => {
                            self.traverse_ft_get_supply(expr, args[0].match_atom().unwrap())
                        }
                        MintToken => self.traverse_ft_mint(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1],
                            &args[2],
                        ),
                        BurnAsset => self.traverse_nft_burn(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1],
                            &args[2],
                        ),
                        TransferAsset => self.traverse_nft_transfer(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1],
                            &args[2],
                            &args[3],
                        ),
                        MintAsset => self.traverse_nft_mint(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1],
                            &args[2],
                        ),
                        GetAssetOwner => self.traverse_nft_get_owner(
                            expr,
                            args[0].match_atom().unwrap(),
                            &args[1],
                        ),
                    };
                } else {
                    rv = self.traverse_call_user_defined(expr, function_name, args);
                }
            }
        }

        rv && self.visit_list(expr, list)
    }

    fn visit_list(&mut self, expr: &'a SymbolicExpression, list: &'a [SymbolicExpression]) -> bool {
        true
    }

    fn visit_atom_value(&mut self, expr: &'a SymbolicExpression, value: &Value) -> bool {
        true
    }

    fn visit_atom(&mut self, expr: &'a SymbolicExpression, atom: &'a ClarityName) -> bool {
        true
    }

    fn visit_literal_value(&mut self, expr: &'a SymbolicExpression, value: &Value) -> bool {
        true
    }

    fn visit_field(&mut self, expr: &'a SymbolicExpression, field: &TraitIdentifier) -> bool {
        true
    }

    fn visit_trait_reference(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        trait_def: &TraitDefinition,
    ) -> bool {
        true
    }

    // Higher level traverse/visit methods

    fn traverse_define_constant(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(value) && self.visit_define_constant(expr, name, value)
    }

    fn visit_define_constant(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        value: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_define_private(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(body) && self.visit_define_private(expr, name, parameters, body)
    }

    fn visit_define_private(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_define_read_only(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(body) && self.visit_define_read_only(expr, name, parameters, body)
    }

    fn visit_define_read_only(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_define_public(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(body) && self.visit_define_public(expr, name, parameters, body)
    }

    fn visit_define_public(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        parameters: Option<Vec<TypedVar<'a>>>,
        body: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_define_nft(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        nft_type: &'a SymbolicExpression,
    ) -> bool {
        self.visit_define_nft(expr, name, nft_type)
    }

    fn visit_define_nft(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        nft_type: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_define_ft(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        supply: Option<&'a SymbolicExpression>,
    ) -> bool {
        if let Some(supply_expr) = supply {
            if !self.traverse_expr(supply_expr) {
                return false;
            }
        }

        self.visit_define_ft(expr, name, supply)
    }

    fn visit_define_ft(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        supply: Option<&'a SymbolicExpression>,
    ) -> bool {
        true
    }

    fn traverse_define_map(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        key_type: &'a SymbolicExpression,
        value_type: &'a SymbolicExpression,
    ) -> bool {
        self.visit_define_map(expr, name, key_type, value_type)
    }

    fn visit_define_map(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        key_type: &'a SymbolicExpression,
        value_type: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_define_data_var(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        data_type: &'a SymbolicExpression,
        initial: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(initial) && self.visit_define_data_var(expr, name, data_type, initial)
    }

    fn visit_define_data_var(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        data_type: &'a SymbolicExpression,
        initial: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_define_trait(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        functions: &'a [SymbolicExpression],
    ) -> bool {
        self.visit_define_trait(expr, name, functions)
    }

    fn visit_define_trait(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        functions: &'a [SymbolicExpression],
    ) -> bool {
        true
    }

    fn traverse_use_trait(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        trait_identifier: &TraitIdentifier,
    ) -> bool {
        self.visit_use_trait(expr, name, trait_identifier)
    }

    fn visit_use_trait(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        trait_identifier: &TraitIdentifier,
    ) -> bool {
        true
    }

    fn traverse_impl_trait(
        &mut self,
        expr: &'a SymbolicExpression,
        trait_identifier: &TraitIdentifier,
    ) -> bool {
        self.visit_impl_trait(expr, trait_identifier)
    }

    fn visit_impl_trait(
        &mut self,
        expr: &'a SymbolicExpression,
        trait_identifier: &TraitIdentifier,
    ) -> bool {
        true
    }

    fn traverse_arithmetic(
        &mut self,
        expr: &'a SymbolicExpression,
        func: NativeFunctions,
        operands: &'a [SymbolicExpression],
    ) -> bool {
        for operand in operands {
            if !self.traverse_expr(operand) {
                return false;
            }
        }
        self.visit_arithmetic(expr, func, operands)
    }

    fn visit_arithmetic(
        &mut self,
        expr: &'a SymbolicExpression,
        func: NativeFunctions,
        operands: &'a [SymbolicExpression],
    ) -> bool {
        true
    }

    fn traverse_binary_bitwise(
        &mut self,
        expr: &'a SymbolicExpression,
        func: NativeFunctions,
        lhs: &'a SymbolicExpression,
        rhs: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(lhs)
            && self.traverse_expr(rhs)
            && self.visit_binary_bitwise(expr, func, lhs, rhs)
    }

    fn visit_binary_bitwise(
        &mut self,
        expr: &'a SymbolicExpression,
        func: NativeFunctions,
        lhs: &'a SymbolicExpression,
        rhs: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_comparison(
        &mut self,
        expr: &'a SymbolicExpression,
        func: NativeFunctions,
        operands: &'a [SymbolicExpression],
    ) -> bool {
        for operand in operands {
            if !self.traverse_expr(operand) {
                return false;
            }
        }
        self.visit_comparison(expr, func, operands)
    }

    fn visit_comparison(
        &mut self,
        expr: &'a SymbolicExpression,
        func: NativeFunctions,
        operands: &'a [SymbolicExpression],
    ) -> bool {
        true
    }

    fn traverse_lazy_logical(
        &mut self,
        expr: &'a SymbolicExpression,
        function: NativeFunctions,
        operands: &'a [SymbolicExpression],
    ) -> bool {
        for operand in operands {
            if !self.traverse_expr(operand) {
                return false;
            }
        }
        self.visit_lazy_logical(expr, function, operands)
    }

    fn visit_lazy_logical(
        &mut self,
        expr: &'a SymbolicExpression,
        function: NativeFunctions,
        operands: &'a [SymbolicExpression],
    ) -> bool {
        true
    }

    fn traverse_logical(
        &mut self,
        expr: &'a SymbolicExpression,
        function: NativeFunctions,
        operands: &'a [SymbolicExpression],
    ) -> bool {
        for operand in operands {
            if !self.traverse_expr(operand) {
                return false;
            }
        }
        self.visit_logical(expr, function, operands)
    }

    fn visit_logical(
        &mut self,
        expr: &'a SymbolicExpression,
        function: NativeFunctions,
        operands: &'a [SymbolicExpression],
    ) -> bool {
        true
    }

    fn traverse_int_cast(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(input) && self.visit_int_cast(expr, input)
    }

    fn visit_int_cast(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_if(
        &mut self,
        expr: &'a SymbolicExpression,
        cond: &'a SymbolicExpression,
        then_expr: &'a SymbolicExpression,
        else_expr: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(cond)
            && self.traverse_expr(then_expr)
            && self.traverse_expr(else_expr)
            && self.visit_if(expr, cond, then_expr, else_expr)
    }

    fn visit_if(
        &mut self,
        expr: &'a SymbolicExpression,
        cond: &'a SymbolicExpression,
        then_expr: &'a SymbolicExpression,
        else_expr: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_var_get(&mut self, expr: &'a SymbolicExpression, name: &'a ClarityName) -> bool {
        self.visit_var_get(expr, name)
    }

    fn visit_var_get(&mut self, expr: &'a SymbolicExpression, name: &'a ClarityName) -> bool {
        true
    }

    fn traverse_var_set(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(value) && self.visit_var_set(expr, name, value)
    }

    fn visit_var_set(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        value: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_map_get(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        key: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        for (_, val) in key {
            if !self.traverse_expr(val) {
                return false;
            }
        }
        self.visit_map_get(expr, name, key)
    }

    fn visit_map_get(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        key: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        true
    }

    fn traverse_map_set(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        key: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
        value: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        for (_, key_val) in key {
            if !self.traverse_expr(key_val) {
                return false;
            }
        }
        for (_, val_val) in value {
            if !self.traverse_expr(val_val) {
                return false;
            }
        }
        self.visit_map_set(expr, name, key, value)
    }

    fn visit_map_set(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        key: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
        value: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        true
    }

    fn traverse_map_insert(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        key: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
        value: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        for (_, key_val) in key {
            if !self.traverse_expr(key_val) {
                return false;
            }
        }
        for (_, val_val) in value {
            if !self.traverse_expr(val_val) {
                return false;
            }
        }
        self.visit_map_insert(expr, name, key, value)
    }

    fn visit_map_insert(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        key: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
        value: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        true
    }

    fn traverse_map_delete(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        key: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        for (_, val) in key {
            if !self.traverse_expr(val) {
                return false;
            }
        }
        self.visit_map_delete(expr, name, key)
    }

    fn visit_map_delete(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        key: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        true
    }

    fn traverse_tuple(
        &mut self,
        expr: &'a SymbolicExpression,
        values: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        for (_, val) in values {
            if !self.traverse_expr(val) {
                return false;
            }
        }
        self.visit_tuple(expr, values)
    }

    fn visit_tuple(
        &mut self,
        expr: &'a SymbolicExpression,
        values: &HashMap<Option<&'a ClarityName>, &'a SymbolicExpression>,
    ) -> bool {
        true
    }

    fn traverse_get(
        &mut self,
        expr: &'a SymbolicExpression,
        key: &'a ClarityName,
        tuple: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(tuple) && self.visit_get(expr, key, tuple)
    }

    fn visit_get(
        &mut self,
        expr: &'a SymbolicExpression,
        key: &'a ClarityName,
        tuple: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_merge(
        &mut self,
        expr: &'a SymbolicExpression,
        tuple1: &'a SymbolicExpression,
        tuple2: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(tuple1)
            && self.traverse_expr(tuple2)
            && self.visit_merge(expr, tuple1, tuple2)
    }

    fn visit_merge(
        &mut self,
        expr: &'a SymbolicExpression,
        tuple1: &'a SymbolicExpression,
        tuple2: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_begin(
        &mut self,
        expr: &'a SymbolicExpression,
        statements: &'a [SymbolicExpression],
    ) -> bool {
        for stmt in statements {
            if !self.traverse_expr(stmt) {
                return false;
            }
        }
        self.visit_begin(expr, statements)
    }

    fn visit_begin(
        &mut self,
        expr: &'a SymbolicExpression,
        statements: &'a [SymbolicExpression],
    ) -> bool {
        true
    }

    fn traverse_hash(
        &mut self,
        expr: &'a SymbolicExpression,
        func: NativeFunctions,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(value) && self.visit_hash(expr, func, value)
    }

    fn visit_hash(
        &mut self,
        expr: &'a SymbolicExpression,
        func: NativeFunctions,
        value: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_secp256k1_recover(
        &mut self,
        expr: &'a SymbolicExpression,
        hash: &'a SymbolicExpression,
        signature: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(hash)
            && self.traverse_expr(signature)
            && self.visit_secp256k1_recover(expr, hash, signature)
    }

    fn visit_secp256k1_recover(
        &mut self,
        expr: &'a SymbolicExpression,
        hash: &SymbolicExpression,
        signature: &SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_secp256k1_verify(
        &mut self,
        expr: &'a SymbolicExpression,
        hash: &'a SymbolicExpression,
        signature: &'a SymbolicExpression,
        public_key: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(hash)
            && self.traverse_expr(signature)
            && self.visit_secp256k1_verify(expr, hash, signature, public_key)
    }

    fn visit_secp256k1_verify(
        &mut self,
        expr: &'a SymbolicExpression,
        hash: &SymbolicExpression,
        signature: &SymbolicExpression,
        public_key: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_print(
        &mut self,
        expr: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(value) && self.visit_print(expr, value)
    }

    fn visit_print(&mut self, expr: &'a SymbolicExpression, value: &'a SymbolicExpression) -> bool {
        true
    }

    fn traverse_static_contract_call(
        &mut self,
        expr: &'a SymbolicExpression,
        contract_identifier: &'a QualifiedContractIdentifier,
        function_name: &'a ClarityName,
        args: &'a [SymbolicExpression],
    ) -> bool {
        for arg in args.iter() {
            if !self.traverse_expr(arg) {
                return false;
            }
        }
        self.visit_static_contract_call(expr, contract_identifier, function_name, args)
    }

    fn visit_static_contract_call(
        &mut self,
        expr: &'a SymbolicExpression,
        contract_identifier: &'a QualifiedContractIdentifier,
        function_name: &'a ClarityName,
        args: &'a [SymbolicExpression],
    ) -> bool {
        true
    }

    fn traverse_dynamic_contract_call(
        &mut self,
        expr: &'a SymbolicExpression,
        trait_ref: &'a SymbolicExpression,
        function_name: &'a ClarityName,
        args: &'a [SymbolicExpression],
    ) -> bool {
        self.traverse_expr(trait_ref);
        for arg in args.iter() {
            if !self.traverse_expr(arg) {
                return false;
            }
        }
        self.visit_dynamic_contract_call(expr, trait_ref, function_name, args)
    }

    fn visit_dynamic_contract_call(
        &mut self,
        expr: &'a SymbolicExpression,
        trait_ref: &'a SymbolicExpression,
        function_name: &'a ClarityName,
        args: &'a [SymbolicExpression],
    ) -> bool {
        true
    }

    fn traverse_as_contract(
        &mut self,
        expr: &'a SymbolicExpression,
        inner: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(inner) && self.visit_as_contract(expr, inner)
    }

    fn visit_as_contract(
        &mut self,
        expr: &'a SymbolicExpression,
        inner: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_contract_of(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(name) && self.visit_contract_of(expr, name)
    }

    fn visit_contract_of(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_principal_of(
        &mut self,
        expr: &'a SymbolicExpression,
        public_key: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(public_key) && self.visit_principal_of(expr, public_key)
    }

    fn visit_principal_of(
        &mut self,
        expr: &'a SymbolicExpression,
        public_key: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_at_block(
        &mut self,
        expr: &'a SymbolicExpression,
        block: &'a SymbolicExpression,
        inner: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(block)
            && self.traverse_expr(inner)
            && self.visit_at_block(expr, block, inner)
    }

    fn visit_at_block(
        &mut self,
        expr: &'a SymbolicExpression,
        block: &'a SymbolicExpression,
        inner: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_get_block_info(
        &mut self,
        expr: &'a SymbolicExpression,
        prop_name: &'a ClarityName,
        block: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(block) && self.visit_get_block_info(expr, prop_name, block)
    }

    fn visit_get_block_info(
        &mut self,
        expr: &'a SymbolicExpression,
        prop_name: &'a ClarityName,
        block: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_err(
        &mut self,
        expr: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(value) && self.visit_err(expr, value)
    }

    fn visit_err(&mut self, expr: &'a SymbolicExpression, value: &'a SymbolicExpression) -> bool {
        true
    }

    fn traverse_ok(&mut self, expr: &'a SymbolicExpression, value: &'a SymbolicExpression) -> bool {
        self.traverse_expr(value) && self.visit_ok(expr, value)
    }

    fn visit_ok(&mut self, expr: &'a SymbolicExpression, value: &'a SymbolicExpression) -> bool {
        true
    }

    fn traverse_some(
        &mut self,
        expr: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(value) && self.visit_some(expr, value)
    }

    fn visit_some(&mut self, expr: &'a SymbolicExpression, value: &'a SymbolicExpression) -> bool {
        true
    }

    fn traverse_default_to(
        &mut self,
        expr: &'a SymbolicExpression,
        default: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(default)
            && self.traverse_expr(value)
            && self.visit_default_to(expr, default, value)
    }

    fn visit_default_to(
        &mut self,
        expr: &'a SymbolicExpression,
        default: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_unwrap(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
        throws: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(input)
            && self.traverse_expr(throws)
            && self.visit_unwrap(expr, input, throws)
    }

    fn visit_unwrap(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
        throws: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_unwrap_err(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
        throws: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(input)
            && self.traverse_expr(throws)
            && self.visit_unwrap_err(expr, input, throws)
    }

    fn visit_unwrap_err(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
        throws: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_is_ok(
        &mut self,
        expr: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(value) && self.visit_is_ok(expr, value)
    }

    fn visit_is_ok(&mut self, expr: &'a SymbolicExpression, value: &'a SymbolicExpression) -> bool {
        true
    }

    fn traverse_is_none(
        &mut self,
        expr: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(value) && self.visit_is_none(expr, value)
    }

    fn visit_is_none(
        &mut self,
        expr: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_is_err(
        &mut self,
        expr: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(value) && self.visit_is_err(expr, value)
    }

    fn visit_is_err(
        &mut self,
        expr: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_is_some(
        &mut self,
        expr: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(value) && self.visit_is_some(expr, value)
    }

    fn visit_is_some(
        &mut self,
        expr: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_filter(
        &mut self,
        expr: &'a SymbolicExpression,
        func: &'a ClarityName,
        sequence: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(sequence) && self.visit_filter(expr, func, sequence)
    }

    fn visit_filter(
        &mut self,
        expr: &'a SymbolicExpression,
        func: &'a ClarityName,
        sequence: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_unwrap_panic(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(input) && self.visit_unwrap_panic(expr, input)
    }

    fn visit_unwrap_panic(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_unwrap_err_panic(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(input) && self.visit_unwrap_err_panic(expr, input)
    }

    fn visit_unwrap_err_panic(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_match_option(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
        some_name: &'a ClarityName,
        some_branch: &'a SymbolicExpression,
        none_branch: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(input)
            && self.traverse_expr(some_branch)
            && self.traverse_expr(none_branch)
            && self.visit_match_option(expr, input, some_name, some_branch, none_branch)
    }

    fn visit_match_option(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
        some_name: &'a ClarityName,
        some_branch: &'a SymbolicExpression,
        none_branch: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_match_response(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
        ok_name: &'a ClarityName,
        ok_branch: &'a SymbolicExpression,
        err_name: &'a ClarityName,
        err_branch: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(input)
            && self.traverse_expr(ok_branch)
            && self.traverse_expr(err_branch)
            && self.visit_match_response(expr, input, ok_name, ok_branch, err_name, err_branch)
    }

    fn visit_match_response(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
        ok_name: &'a ClarityName,
        ok_branch: &'a SymbolicExpression,
        err_name: &'a ClarityName,
        err_branch: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_try(
        &mut self,
        expr: &'a SymbolicExpression,
        input: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(input) && self.visit_try(expr, input)
    }

    fn visit_try(&mut self, expr: &'a SymbolicExpression, input: &'a SymbolicExpression) -> bool {
        true
    }

    fn traverse_asserts(
        &mut self,
        expr: &'a SymbolicExpression,
        cond: &'a SymbolicExpression,
        thrown: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(cond)
            && self.traverse_expr(thrown)
            && self.visit_asserts(expr, cond, thrown)
    }

    fn visit_asserts(
        &mut self,
        expr: &'a SymbolicExpression,
        cond: &'a SymbolicExpression,
        thrown: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_stx_burn(
        &mut self,
        expr: &'a SymbolicExpression,
        amount: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(amount)
            && self.traverse_expr(sender)
            && self.visit_stx_burn(expr, amount, sender)
    }

    fn visit_stx_burn(
        &mut self,
        expr: &'a SymbolicExpression,
        amount: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_stx_transfer(
        &mut self,
        expr: &'a SymbolicExpression,
        amount: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(amount)
            && self.traverse_expr(sender)
            && self.traverse_expr(recipient)
            && self.visit_stx_transfer(expr, amount, sender, recipient)
    }

    fn visit_stx_transfer(
        &mut self,
        expr: &'a SymbolicExpression,
        amount: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_stx_get_balance(
        &mut self,
        expr: &'a SymbolicExpression,
        owner: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(owner) && self.visit_stx_get_balance(expr, owner)
    }

    fn visit_stx_get_balance(
        &mut self,
        expr: &'a SymbolicExpression,
        owner: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_ft_burn(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        amount: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(amount)
            && self.traverse_expr(sender)
            && self.visit_ft_burn(expr, token, amount, sender)
    }

    fn visit_ft_burn(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        amount: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_ft_transfer(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        amount: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(amount)
            && self.traverse_expr(sender)
            && self.traverse_expr(recipient)
            && self.visit_ft_transfer(expr, token, amount, sender, recipient)
    }

    fn visit_ft_transfer(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        amount: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_ft_get_balance(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        owner: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(owner) && self.visit_ft_get_balance(expr, token, owner)
    }

    fn visit_ft_get_balance(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        owner: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_ft_get_supply(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
    ) -> bool {
        self.visit_ft_get_supply(expr, token)
    }

    fn visit_ft_get_supply(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
    ) -> bool {
        true
    }

    fn traverse_ft_mint(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        amount: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(amount)
            && self.traverse_expr(recipient)
            && self.visit_ft_mint(expr, token, amount, recipient)
    }

    fn visit_ft_mint(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        amount: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_nft_burn(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        identifier: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(identifier)
            && self.traverse_expr(sender)
            && self.visit_nft_burn(expr, token, identifier, sender)
    }

    fn visit_nft_burn(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        identifier: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_nft_transfer(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        identifier: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(identifier)
            && self.traverse_expr(sender)
            && self.traverse_expr(recipient)
            && self.visit_nft_transfer(expr, token, identifier, sender, recipient)
    }

    fn visit_nft_transfer(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        identifier: &'a SymbolicExpression,
        sender: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_nft_mint(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        identifier: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(identifier)
            && self.traverse_expr(recipient)
            && self.visit_nft_mint(expr, token, identifier, recipient)
    }

    fn visit_nft_mint(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        identifier: &'a SymbolicExpression,
        recipient: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_nft_get_owner(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        identifier: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(identifier) && self.visit_nft_get_owner(expr, token, identifier)
    }

    fn visit_nft_get_owner(
        &mut self,
        expr: &'a SymbolicExpression,
        token: &'a ClarityName,
        identifier: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_let(
        &mut self,
        expr: &'a SymbolicExpression,
        bindings: &HashMap<&'a ClarityName, &'a SymbolicExpression>,
        body: &'a [SymbolicExpression],
    ) -> bool {
        for (_, val) in bindings {
            if !self.traverse_expr(val) {
                return false;
            }
        }
        for expr in body {
            if !self.traverse_expr(expr) {
                return false;
            }
        }
        self.visit_let(expr, bindings, body)
    }

    fn visit_let(
        &mut self,
        expr: &'a SymbolicExpression,
        bindings: &HashMap<&'a ClarityName, &'a SymbolicExpression>,
        body: &'a [SymbolicExpression],
    ) -> bool {
        true
    }

    fn traverse_map(
        &mut self,
        expr: &'a SymbolicExpression,
        func: &'a ClarityName,
        sequences: &'a [SymbolicExpression],
    ) -> bool {
        for sequence in sequences {
            if !self.traverse_expr(sequence) {
                return false;
            }
        }
        self.visit_map(expr, func, sequences)
    }

    fn visit_map(
        &mut self,
        expr: &'a SymbolicExpression,
        func: &'a ClarityName,
        sequences: &'a [SymbolicExpression],
    ) -> bool {
        true
    }

    fn traverse_fold(
        &mut self,
        expr: &'a SymbolicExpression,
        func: &'a ClarityName,
        sequence: &'a SymbolicExpression,
        initial: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(sequence)
            && self.traverse_expr(initial)
            && self.visit_fold(expr, func, sequence, initial)
    }

    fn visit_fold(
        &mut self,
        expr: &'a SymbolicExpression,
        func: &'a ClarityName,
        sequence: &'a SymbolicExpression,
        initial: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_append(
        &mut self,
        expr: &'a SymbolicExpression,
        list: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(list)
            && self.traverse_expr(value)
            && self.visit_append(expr, list, value)
    }

    fn visit_append(
        &mut self,
        expr: &'a SymbolicExpression,
        list: &'a SymbolicExpression,
        value: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_concat(
        &mut self,
        expr: &'a SymbolicExpression,
        lhs: &'a SymbolicExpression,
        rhs: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(lhs) && self.traverse_expr(rhs) && self.visit_concat(expr, lhs, rhs)
    }

    fn visit_concat(
        &mut self,
        expr: &'a SymbolicExpression,
        lhs: &'a SymbolicExpression,
        rhs: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_as_max_len(
        &mut self,
        expr: &'a SymbolicExpression,
        sequence: &'a SymbolicExpression,
        length: u128,
    ) -> bool {
        self.traverse_expr(sequence) && self.visit_as_max_len(expr, sequence, length)
    }

    fn visit_as_max_len(
        &mut self,
        expr: &'a SymbolicExpression,
        sequence: &'a SymbolicExpression,
        length: u128,
    ) -> bool {
        true
    }

    fn traverse_len(
        &mut self,
        expr: &'a SymbolicExpression,
        sequence: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(sequence) && self.visit_len(expr, sequence)
    }

    fn visit_len(
        &mut self,
        expr: &'a SymbolicExpression,
        sequence: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_element_at(
        &mut self,
        expr: &'a SymbolicExpression,
        sequence: &'a SymbolicExpression,
        index: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(sequence)
            && self.traverse_expr(index)
            && self.visit_element_at(expr, sequence, index)
    }

    fn visit_element_at(
        &mut self,
        expr: &'a SymbolicExpression,
        sequence: &'a SymbolicExpression,
        index: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_index_of(
        &mut self,
        expr: &'a SymbolicExpression,
        sequence: &'a SymbolicExpression,
        item: &'a SymbolicExpression,
    ) -> bool {
        self.traverse_expr(sequence)
            && self.traverse_expr(item)
            && self.visit_element_at(expr, sequence, item)
    }

    fn visit_index_of(
        &mut self,
        expr: &'a SymbolicExpression,
        sequence: &'a SymbolicExpression,
        item: &'a SymbolicExpression,
    ) -> bool {
        true
    }

    fn traverse_list_cons(
        &mut self,
        expr: &'a SymbolicExpression,
        args: &'a [SymbolicExpression],
    ) -> bool {
        for arg in args.iter() {
            if !self.traverse_expr(arg) {
                return false;
            }
        }
        self.visit_list_cons(expr, args)
    }

    fn visit_list_cons(
        &mut self,
        expr: &'a SymbolicExpression,
        args: &'a [SymbolicExpression],
    ) -> bool {
        true
    }

    fn traverse_call_user_defined(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        args: &'a [SymbolicExpression],
    ) -> bool {
        for arg in args.iter() {
            if !self.traverse_expr(arg) {
                return false;
            }
        }
        self.visit_call_user_defined(expr, name, args)
    }

    fn visit_call_user_defined(
        &mut self,
        expr: &'a SymbolicExpression,
        name: &'a ClarityName,
        args: &'a [SymbolicExpression],
    ) -> bool {
        true
    }
}

pub fn traverse<'a>(visitor: &mut impl ASTVisitor<'a>, exprs: &'a [SymbolicExpression]) -> bool {
    for expr in exprs {
        if !visitor.traverse_expr(expr) {
            return false;
        }
    }
    true
}

impl<'a> SymbolicExpression {
    fn match_tuple(&'a self) -> Option<HashMap<Option<&'a ClarityName>, &SymbolicExpression>> {
        if let Some(list) = self.match_list() {
            if let Some((function_name, args)) = list.split_first() {
                if let Some(function_name) = function_name.match_atom() {
                    if NativeFunctions::lookup_by_name(function_name)
                        == Some(NativeFunctions::TupleCons)
                    {
                        let mut tuple_map = HashMap::new();
                        for element in args {
                            let pair = element.match_list().unwrap();
                            tuple_map.insert(pair[0].match_atom(), &pair[1]);
                        }
                        return Some(tuple_map);
                    }
                }
            }
        }
        None
    }

    fn match_pairs(&'a self) -> Option<HashMap<&'a ClarityName, &SymbolicExpression>> {
        let list = self.match_list()?;
        let mut tuple_map = HashMap::new();
        for pair_list in list {
            let pair = pair_list.match_list()?;
            if pair.len() != 2 {
                return None;
            }
            tuple_map.insert(pair[0].match_atom()?, &pair[1]);
        }
        return Some(tuple_map);
    }
}

fn match_pairs<'a>(list: &'a [SymbolicExpression]) -> Option<Vec<TypedVar<'a>>> {
    let mut vars = Vec::new();
    for pair_list in list {
        let pair = pair_list.match_list()?;
        if pair.len() != 2 {
            return None;
        }
        let name = pair[0].match_atom()?;
        vars.push(TypedVar {
            name: name,
            type_expr: &pair[1],
            decl_span: pair[0].span.clone(),
        });
    }
    return Some(vars);
}

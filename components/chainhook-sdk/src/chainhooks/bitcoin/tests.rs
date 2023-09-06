use super::super::types::MatchingRule;
use super::*;
use crate::types::BitcoinTransactionMetadata;
use chainhook_types::bitcoin::TxOut;

use test_case::test_case;

#[test_case(
    "0x6affAAAA",
     MatchingRule::Equals(String::from("0xAAAA")),
    true;
    "OpReturn: Equals matches Hex value"
)]
#[test_case(
    "0x60ff0000",
     MatchingRule::Equals(String::from("0x0000")),
    false;
    "OpReturn: Invalid OP_RETURN opcode"
)]
#[test_case(
    "0x6aff012345",
     MatchingRule::Equals(String::from("0x0000")),
    false;
    "OpReturn: Equals does not match Hex value"
)]
#[test_case(
    "0x6aff68656C6C6F",
     MatchingRule::Equals(String::from("hello")),
    true;
    "OpReturn: Equals matches ASCII value"
)]
#[test_case(
    "0x6affAA0000",
     MatchingRule::StartsWith(String::from("0xAA")),
    true;
    "OpReturn: StartsWith matches Hex value"
)]
#[test_case(
    "0x6aff585858", // 0x585858 => XXX
     MatchingRule::StartsWith(String::from("X")),
    true;
    "OpReturn: StartsWith matches ASCII value"
)]
#[test_case(
    "0x6aff0000AA",
     MatchingRule::EndsWith(String::from("0xAA")),
    true;
    "OpReturn: EndsWith matches Hex value"
)]
#[test_case(
    "0x6aff000058",
     MatchingRule::EndsWith(String::from("X")),
    true;
    "OpReturn: EndsWith matches ASCII value"
)]

fn test_script_pubkey_evaluation(script_pubkey: &str, rule: MatchingRule, matches: bool) {
    let predicate = BitcoinPredicateType::Outputs(OutputPredicate::OpReturn(rule));

    let outputs = vec![TxOut {
        value: 0,
        script_pubkey: String::from(script_pubkey),
    }];

    let tx = BitcoinTransactionData {
        transaction_identifier: TransactionIdentifier {
            hash: String::from(""),
        },
        operations: vec![],
        metadata: BitcoinTransactionMetadata {
            fee: 0,
            proof: None,
            inputs: vec![],
            stacks_operations: vec![],
            ordinal_operations: vec![],

            outputs,
        },
    };

    let ctx = Context {
        logger: None,
        tracer: false,
    };

    assert_eq!(matches, predicate.evaluate_transaction_predicate(&tx, &ctx),);
}

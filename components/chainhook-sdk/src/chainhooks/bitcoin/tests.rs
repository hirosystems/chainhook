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
fn test_opreturn_evaluation(script_pubkey: &str, rule: MatchingRule, matches: bool) {
    script_pubkey_evaluation(OutputPredicate::OpReturn(rule), script_pubkey, matches)
}

// Descriptor test cases have been taken from
// https://github.com/bitcoin/bitcoin/blob/master/doc/descriptors.md#examples
// To generate the address run:
// `bdk-cli -n testnet wallet --descriptor <descriptor> get_new_address`
#[test_case(
    "tb1q0ht9tyks4vh7p5p904t340cr9nvahy7um9zdem",
    "wpkh(02f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9)";
    "Descriptor: P2WPKH"
)]
#[test_case(
    "2NBtBzAJ84E3sTy1KooEHYVwmMhUVdJAyEa",
    "sh(wpkh(03fff97bd5755eeea420453a14355235d382f6472f8568a18b2f057a1460297556))";
    "Descriptor: P2SH-P2WPKH"
)]
#[test_case(
    "tb1qwu7hp9vckakyuw6htsy244qxtztrlyez4l7qlrpg68v6drgvj39qya5jch",
    "wsh(multi(2,03a0434d9e47f3c86235477c7b1ae6ae5d3442d49b1943c2b752a68e2a47e247c7,03774ae7f858a9411e5ef4246b70c65aac5649980be5c17891bbec17895da008cb,03d01115d548e7561b15c38f004d734633687cf4419620095bc5b0f47070afe85a))";
    "Descriptor: P2WSH 2-of-3 multisig output"
)]
fn test_descriptor_evaluation(addr: &str, expr: &str) {
    // turn the address into a script_pubkey with a 0x prefix, as expected by the evaluator.
    let script_pubkey = Address::from_str(addr)
        .unwrap()
        .assume_checked()
        .script_pubkey();
    let matching_script_pubkey = format!("0x{}", hex::encode(script_pubkey));

    let rule = DescriptorMatchingRule {
        expression: expr.to_string(),
        // TODO: test ranges
        range: None,
    };

    // matching against the script_pubkey generated from the address should match.
    script_pubkey_evaluation(
        OutputPredicate::Descriptor(rule.clone()),
        &matching_script_pubkey,
        true,
    );

    // matching against a fake script_pubkey should not match.
    script_pubkey_evaluation(OutputPredicate::Descriptor(rule.clone()), "0xffff", false);
}

// script_pubkey_evaluation is a helper that evaluates a a script_pubkey against a transaction predicate.
fn script_pubkey_evaluation(output: OutputPredicate, script_pubkey: &str, matches: bool) {
    let predicate = BitcoinPredicateType::Outputs(output);

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

    assert_eq!(matches, predicate.evaluate_transaction_predicate(&tx, &ctx));
}

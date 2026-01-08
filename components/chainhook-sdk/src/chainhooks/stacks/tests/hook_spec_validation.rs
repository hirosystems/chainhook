use std::collections::BTreeMap;
use crate::chainhooks::stacks::{StacksChainhookSpecification, StacksChainhookSpecificationNetworkMap, StacksContractCallBasedPredicate, StacksContractDeploymentPredicate, StacksPredicate, StacksPrintEventBasedPredicate};
#[cfg(feature = "stacks-signers")]
use crate::chainhooks::stacks::StacksSignerMessagePredicate;
use crate::chainhooks::types::*;
use crate::chainhooks::types::HttpHook;
use chainhook_types::StacksNetwork;
use test_case::test_case;

lazy_static! {
    static ref TXID_NO_PREFIX: String = "1234567890123456789012345678901234567890123456789012345678901234".into();
    static ref TXID_NOT_HEX: String = "0xw234567890123456789012345678901234567890123456789012345678901234".into();
    static ref TXID_SHORT: String = "0x234567890123456789012345678901234567890123456789012345678901234".into();
    static ref TXID_LONG: String = "0x11234567890123456789012345678901234567890123456789012345678901234".into();
    static ref TXID_VALID: String = "0x1234567890123456789012345678901234567890123456789012345678901234".into();
    static ref STACKS_ADDRESS_INVALID: String = "SQ1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM".into();
    static ref STACKS_ADDRESS_VALID_MAINNET: String = "SP000000000000000000002Q6VF78".into();
    static ref STACKS_ADDRESS_VALID_TESTNET: String = "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM".into();
    static ref STACKS_ADDRESS_VALID_MULTISIG: String = "SN2QE43MMXFDMAT3TPRGQ38BQ50VSRMBRQ6B16W5J".into();
    static ref CONTRACT_ID_INVALID_ADDRESS: String = "SQ1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.contract-name".into();
    static ref CONTRACT_ID_NO_PERIOD: String = "SQ1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGMcontract-name".into();
    static ref CONTRACT_ID_INVALID_NAME: String = "SQ1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.!&*!".into();
    static ref CONTRACT_ID_VALID: String = "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.contract-name".into();
    static ref INVALID_METHOD: String = "!@*&!*".into();
    static ref INVALID_REGEX: String = "[\\]".into();
    static ref VALID_REGEX: String = "anything".into();

    static ref TXID_PREDICATE_ERR: String = "invalid predicate for scope 'txid': txid must be a 32 byte (64 character) hexadecimal string prefixed with '0x'".into();
    static ref INPUT_TXID_ERR: String = "invalid predicate for scope 'inputs': txid must be a 32 byte (64 character) hexadecimal string prefixed with '0x'".into();
    static ref INVALID_SPEC_NETWORK_MAP_ERR: String = "invalid Stacks predicate 'test' for network simnet: invalid 'then_that' value: invalid 'http_post' data: url string must be a valid Url: relative URL without a base\ninvalid Stacks predicate 'test' for network simnet: invalid 'then_that' value: invalid 'http_post' data: auth header must be a valid header value: failed to parse header value\ninvalid Stacks predicate 'test' for network simnet: invalid 'if_this' value: invalid predicate for scope 'print_event': invalid contract identifier: ParseError(\"Invalid principal literal: base58ck checksum 0x147e6835 does not match expected 0x9b3dfe6a\")\ninvalid Stacks predicate 'test' for network simnet: invalid 'if_this' value: invalid predicate for scope 'print_event': invalid regex: regex parse error:\n    [\\]\n    ^\nerror: unclosed character class\ninvalid Stacks predicate 'test' for network devnet: invalid 'then_that' value: invalid 'http_post' data: url string must be a valid Url: relative URL without a base\ninvalid Stacks predicate 'test' for network devnet: invalid 'then_that' value: invalid 'http_post' data: auth header must be a valid header value: failed to parse header value\ninvalid Stacks predicate 'test' for network devnet: invalid 'if_this' value: invalid predicate for scope 'print_event': invalid contract identifier: ParseError(\"Invalid principal literal: base58ck checksum 0x147e6835 does not match expected 0x9b3dfe6a\")\ninvalid Stacks predicate 'test' for network devnet: invalid 'if_this' value: invalid predicate for scope 'print_event': invalid regex: regex parse error:\n    [\\]\n    ^\nerror: unclosed character class\ninvalid Stacks predicate 'test' for network testnet: invalid 'then_that' value: invalid 'http_post' data: url string must be a valid Url: relative URL without a base\ninvalid Stacks predicate 'test' for network testnet: invalid 'then_that' value: invalid 'http_post' data: auth header must be a valid header value: failed to parse header value\ninvalid Stacks predicate 'test' for network testnet: invalid 'if_this' value: invalid predicate for scope 'print_event': invalid contract identifier: ParseError(\"Invalid principal literal: base58ck checksum 0x147e6835 does not match expected 0x9b3dfe6a\")\ninvalid Stacks predicate 'test' for network testnet: invalid 'if_this' value: invalid predicate for scope 'print_event': invalid regex: regex parse error:\n    [\\]\n    ^\nerror: unclosed character class\ninvalid Stacks predicate 'test' for network mainnet: invalid 'then_that' value: invalid 'http_post' data: url string must be a valid Url: relative URL without a base\ninvalid Stacks predicate 'test' for network mainnet: invalid 'then_that' value: invalid 'http_post' data: auth header must be a valid header value: failed to parse header value\ninvalid Stacks predicate 'test' for network mainnet: invalid 'if_this' value: invalid predicate for scope 'print_event': invalid contract identifier: ParseError(\"Invalid principal literal: base58ck checksum 0x147e6835 does not match expected 0x9b3dfe6a\")\ninvalid Stacks predicate 'test' for network mainnet: invalid 'if_this' value: invalid predicate for scope 'print_event': invalid regex: regex parse error:\n    [\\]\n    ^\nerror: unclosed character class".into();
    static ref CONTRACT_DEPLOYER_ERR: String = "invalid predicate for scope 'contract_deployment': contract deployer must be a valid Stacks address: ParseError(\"Invalid principal literal: base58ck checksum 0x147e6835 does not match expected 0x9b3dfe6a\")".into();
    static ref CONTRACT_ID_ERR: String = "invalid predicate for scope 'contract_call': invalid contract identifier: ParseError(\"Invalid principal literal: base58ck checksum 0x147e6835 does not match expected 0x9b3dfe6a\")".into();
    static ref CONTRACT_ID_NO_PERIOD_ERR: String = "invalid predicate for scope 'contract_call': invalid contract identifier: ParseError(\"Invalid principal literal: expected a `.` in a qualified contract name\")".into();
    static ref CONTRACT_METHOD_ERR: String = "invalid predicate for scope 'contract_call': invalid contract method: BadNameValue(\"ClarityName\", \"!@*&!*\")".into();
    static ref PRINT_EVENT_ID_ERR: String = "invalid predicate for scope 'print_event': invalid contract identifier: ParseError(\"Invalid principal literal: base58ck checksum 0x147e6835 does not match expected 0x9b3dfe6a\")".into();
    static ref INVALID_REGEX_ERR: String = "invalid predicate for scope 'print_event': invalid regex: regex parse error:\n    [\\]\n    ^\nerror: unclosed character class".into();
    
    // Signer message predicates (secp256k1 pubkeys: compressed=66 hex chars, uncompressed=130 hex chars)
    static ref COMPRESSED_PUBKEY_VALID_WITH_PREFIX: String = "0x02a1b2c3d4e5f67890123456789012345678901234567890123456789012345678".into();
    static ref COMPRESSED_PUBKEY_VALID_NO_PREFIX: String = "02a1b2c3d4e5f67890123456789012345678901234567890123456789012345678".into();
    static ref UNCOMPRESSED_PUBKEY_VALID: String = "0x04a1b2c3d4e5f6789012345678901234567890123456789012345678901234567890ab123456789012345678901234567890123456789012345678901234567890".into();
    static ref PUBKEY_INVALID_LENGTH: String = "0x02a1b2c3d4".into();
    static ref PUBKEY_INVALID_HEX: String = "0x02g1b2c3d4e5f67890123456789012345678901234567890123456789012345678".into();
    static ref PUBKEY_INVALID_COMPRESSED_PREFIX: String = "0x01a1b2c3d4e5f67890123456789012345678901234567890123456789012345678".into();
    static ref PUBKEY_INVALID_UNCOMPRESSED_PREFIX: String = "0x05a1b2c3d4e5f6789012345678901234567890123456789012345678901234567890ab123456789012345678901234567890123456789012345678901234567890".into();
    static ref TIMESTAMP_VALID: u64 = 1704067200000; // Jan 1, 2024 in milliseconds
    static ref TIMESTAMP_ZERO: u64 = 0;
    static ref TIMESTAMP_TOO_FAR_FUTURE: u64 = 5000000000000; // Beyond year 2100
    
    static ref SIGNER_PUBKEY_LENGTH_ERR: String = "invalid predicate for scope 'signer_message': signer public key must be a valid secp256k1 public key (33 bytes compressed or 65 bytes uncompressed), represented as a hexadecimal string".into();
    static ref SIGNER_PUBKEY_HEX_ERR: String = "invalid predicate for scope 'signer_message': signer public key must be a hexadecimal string".into();
    static ref SIGNER_PUBKEY_COMPRESSED_PREFIX_ERR: String = "invalid predicate for scope 'signer_message': compressed signer public key must start with '02' or '03'".into();
    static ref SIGNER_PUBKEY_UNCOMPRESSED_PREFIX_ERR: String = "invalid predicate for scope 'signer_message': uncompressed signer public key must start with '04'".into();
    static ref SIGNER_TIMESTAMP_ZERO_ERR: String = "invalid predicate for scope 'signer_message': timestamp must be greater than 0".into();
    static ref SIGNER_TIMESTAMP_FAR_FUTURE_ERR: String = "invalid predicate for scope 'signer_message': timestamp must be a reasonable Unix timestamp in milliseconds (before year 2100)".into();
    
    static ref INVALID_PREDICATE: StacksPredicate = StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate::MatchesRegex { contract_identifier: CONTRACT_ID_INVALID_ADDRESS.clone(), regex:  INVALID_REGEX.clone() });
    static ref INVALID_HOOK_ACTION: HookAction = 
        HookAction::HttpPost(HttpHook { url: "".into(), authorization_header: "\n".into() });
    static ref ALL_INVALID_SPEC: StacksChainhookSpecification = StacksChainhookSpecification::new(INVALID_PREDICATE.clone(), INVALID_HOOK_ACTION.clone());
    static ref ALL_INVALID_SPEC_NETWORK_MAP: ChainhookSpecificationNetworkMap = 
        ChainhookSpecificationNetworkMap::Stacks(
            StacksChainhookSpecificationNetworkMap { 
                uuid: "test".into(), 
                owner_uuid: None, 
                name: "test".into(), 
                version: 1, 
                networks: BTreeMap::from([
                    (StacksNetwork::Simnet, ALL_INVALID_SPEC.clone()),
                    (StacksNetwork::Devnet, ALL_INVALID_SPEC.clone()),
                    (StacksNetwork::Testnet, ALL_INVALID_SPEC.clone()),
                    (StacksNetwork::Mainnet, ALL_INVALID_SPEC.clone()),
                ]) 
            }
        );

}

// StacksPredicate::BlockHeight
#[test_case(
    &StacksPredicate::BlockHeight(BlockIdentifierIndexRule::LowerThan(0)), 
    Some(vec!["invalid predicate for scope 'block_height': 'lower_than' filter must be greater than 0".to_string()]); 
    "invalid lower than"
)]
#[test_case(&StacksPredicate::BlockHeight(BlockIdentifierIndexRule::LowerThan(1)), None; "valid lower than")]
#[test_case(
    &StacksPredicate::BlockHeight(BlockIdentifierIndexRule::Between(10, 5)), 
    Some(vec!["invalid predicate for scope 'block_height': 'between' filter must have left-hand-side valud greater than right-hand-side value".to_string()]); 
    "invalid between"
)]
#[test_case(&StacksPredicate::BlockHeight(BlockIdentifierIndexRule::Between(5, 10)), None; "valid between")]
// StacksPredicate::ContractDeployment
#[test_case(
    &StacksPredicate::ContractDeployment(StacksContractDeploymentPredicate::Deployer(STACKS_ADDRESS_INVALID.clone())), 
    Some(vec![CONTRACT_DEPLOYER_ERR.clone()]); 
    "deployer bad prefix"
)]
#[test_case(
    &StacksPredicate::ContractDeployment(StacksContractDeploymentPredicate::Deployer(STACKS_ADDRESS_VALID_MAINNET.clone())), 
    None; 
    "deployer valid mainnet"
)]
#[test_case(
    &StacksPredicate::ContractDeployment(StacksContractDeploymentPredicate::Deployer(STACKS_ADDRESS_VALID_TESTNET.clone())), 
    None; 
    "deployer valid testnet"
)]
#[test_case(
    &StacksPredicate::ContractDeployment(StacksContractDeploymentPredicate::Deployer(STACKS_ADDRESS_VALID_MULTISIG.clone())), 
    None; 
    "deployer valid multisig"
)]
#[test_case(
    &StacksPredicate::ContractDeployment(StacksContractDeploymentPredicate::Deployer("*".to_string())), 
    None; 
    "deployer valid wildcard"
)]
// StacksPredicate::ContractCall
#[test_case(
    &StacksPredicate::ContractCall(StacksContractCallBasedPredicate { contract_identifier: CONTRACT_ID_INVALID_ADDRESS.clone(), method: INVALID_METHOD.clone()}),
    Some(vec![CONTRACT_ID_ERR.clone(), CONTRACT_METHOD_ERR.clone()]); 
    "invalid id with invalid method"
)]
#[test_case(
    &StacksPredicate::ContractCall(StacksContractCallBasedPredicate { contract_identifier: CONTRACT_ID_VALID.clone(), method: INVALID_METHOD.clone()}),
    Some(vec![CONTRACT_METHOD_ERR.clone()]); 
    "valid id with invalid method"
)]
#[test_case(
    &StacksPredicate::ContractCall(StacksContractCallBasedPredicate { contract_identifier: CONTRACT_ID_NO_PERIOD.clone(), method: "contract-name".to_string()}),
    Some(vec![CONTRACT_ID_NO_PERIOD_ERR.clone()]); 
    "id no period"
)]
#[test_case(
    &StacksPredicate::ContractCall(StacksContractCallBasedPredicate { contract_identifier: CONTRACT_ID_INVALID_NAME.clone(), method: "contract-name".to_string()}),
    Some(vec![CONTRACT_ID_ERR.clone()]); 
    "id invalid contract name"
)]
#[test_case(
    &StacksPredicate::ContractCall(StacksContractCallBasedPredicate { contract_identifier: CONTRACT_ID_VALID.clone(), method: "contract-name".to_string()}),
    None; 
    "id valid"
)]
// StacksPredicate::PrintEvent
#[test_case(
    &StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate::Contains { contract_identifier: CONTRACT_ID_INVALID_ADDRESS.clone(), contains: "string".to_string() }),
    Some(vec![PRINT_EVENT_ID_ERR.clone()]); 
    "contains invalid id"
)]
#[test_case(
    &StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate::Contains { contract_identifier: CONTRACT_ID_VALID.clone(), contains: "string".to_string() }),
    None; 
    "contains valid"
)]
#[test_case(
    &StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate::Contains { contract_identifier: "*".to_string(), contains: "string".to_string() }),
    None; 
    "allows wildcard contract id"
)]
#[test_case(
    &StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate::MatchesRegex { contract_identifier: CONTRACT_ID_INVALID_ADDRESS.clone(), regex: VALID_REGEX.clone() }),
    Some(vec![PRINT_EVENT_ID_ERR.clone()]); 
    "regex invalid id"
)]
#[test_case(
    &StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate::MatchesRegex { contract_identifier: CONTRACT_ID_VALID.clone(), regex:  INVALID_REGEX.clone() }),
    Some(vec![INVALID_REGEX_ERR.clone()]); 
    "regex invalid regex"
)]
#[test_case(
    &StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate::MatchesRegex { contract_identifier: CONTRACT_ID_INVALID_ADDRESS.clone(), regex:  INVALID_REGEX.clone() }),
    Some(vec![PRINT_EVENT_ID_ERR.clone(), INVALID_REGEX_ERR.clone()]); 
    "regex invalid both"
)]
#[test_case(
    &StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate::MatchesRegex { contract_identifier: CONTRACT_ID_VALID.clone(), regex:  VALID_REGEX.clone() }),
    None; 
    "regex valid"
)]
// StacksPredicate::Txid
#[test_case(
    &StacksPredicate::Txid(ExactMatchingRule::Equals(TXID_NO_PREFIX.clone())), 
    Some(vec![TXID_PREDICATE_ERR.clone()]); "txid without 0x"
)]
#[test_case(
    &StacksPredicate::Txid(ExactMatchingRule::Equals(TXID_NOT_HEX.clone())), 
    Some(vec![TXID_PREDICATE_ERR.clone()]); "txid not hex"
)]
#[test_case(
    &StacksPredicate::Txid(ExactMatchingRule::Equals(TXID_SHORT.clone())), 
    Some(vec![TXID_PREDICATE_ERR.clone()]); "txid too short"
)]
#[test_case(
    &StacksPredicate::Txid(ExactMatchingRule::Equals(TXID_LONG.clone())), 
    Some(vec![TXID_PREDICATE_ERR.clone()]); "txid too long"
)]
#[test_case(
    &StacksPredicate::Txid(ExactMatchingRule::Equals(TXID_VALID.clone())), 
    None; "txid just right"
)]
// StacksPredicate::SignerMessage - Pubkey validation
#[cfg(feature = "stacks-signers")]
#[test_case(
    &StacksPredicate::SignerMessage(StacksSignerMessagePredicate::FromSignerPubKey(COMPRESSED_PUBKEY_VALID_WITH_PREFIX.clone())), 
    None; "signer pubkey compressed with prefix"
)]
#[cfg(feature = "stacks-signers")]
#[test_case(
    &StacksPredicate::SignerMessage(StacksSignerMessagePredicate::FromSignerPubKey(COMPRESSED_PUBKEY_VALID_NO_PREFIX.clone())), 
    None; "signer pubkey compressed no prefix"
)]
#[cfg(feature = "stacks-signers")]
#[test_case(
    &StacksPredicate::SignerMessage(StacksSignerMessagePredicate::FromSignerPubKey(UNCOMPRESSED_PUBKEY_VALID.clone())), 
    None; "signer pubkey uncompressed valid"
)]
#[cfg(feature = "stacks-signers")]
#[test_case(
    &StacksPredicate::SignerMessage(StacksSignerMessagePredicate::FromSignerPubKey(PUBKEY_INVALID_LENGTH.clone())), 
    Some(vec![SIGNER_PUBKEY_LENGTH_ERR.clone()]); "signer pubkey invalid length"
)]
#[cfg(feature = "stacks-signers")]
#[test_case(
    &StacksPredicate::SignerMessage(StacksSignerMessagePredicate::FromSignerPubKey(PUBKEY_INVALID_HEX.clone())), 
    Some(vec![SIGNER_PUBKEY_HEX_ERR.clone()]); "signer pubkey invalid hex"
)]
#[cfg(feature = "stacks-signers")]
#[test_case(
    &StacksPredicate::SignerMessage(StacksSignerMessagePredicate::FromSignerPubKey(PUBKEY_INVALID_COMPRESSED_PREFIX.clone())), 
    Some(vec![SIGNER_PUBKEY_COMPRESSED_PREFIX_ERR.clone()]); "signer pubkey invalid compressed prefix"
)]
#[cfg(feature = "stacks-signers")]
#[test_case(
    &StacksPredicate::SignerMessage(StacksSignerMessagePredicate::FromSignerPubKey(PUBKEY_INVALID_UNCOMPRESSED_PREFIX.clone())), 
    Some(vec![SIGNER_PUBKEY_UNCOMPRESSED_PREFIX_ERR.clone()]); "signer pubkey invalid uncompressed prefix"
)]
// StacksPredicate::SignerMessage - Timestamp validation
#[cfg(feature = "stacks-signers")]
#[test_case(
    &StacksPredicate::SignerMessage(StacksSignerMessagePredicate::AfterTimestamp(*TIMESTAMP_VALID)), 
    None; "signer timestamp valid"
)]
#[cfg(feature = "stacks-signers")]
#[test_case(
    &StacksPredicate::SignerMessage(StacksSignerMessagePredicate::AfterTimestamp(*TIMESTAMP_ZERO)), 
    Some(vec![SIGNER_TIMESTAMP_ZERO_ERR.clone()]); "signer timestamp zero"
)]
#[cfg(feature = "stacks-signers")]
#[test_case(
    &StacksPredicate::SignerMessage(StacksSignerMessagePredicate::AfterTimestamp(*TIMESTAMP_TOO_FAR_FUTURE)), 
    Some(vec![SIGNER_TIMESTAMP_FAR_FUTURE_ERR.clone()]); "signer timestamp too far future"
)]
fn it_validates_stacks_predicates(predicate: &StacksPredicate, expected_err: Option<Vec<String>>) {
    if let Err(e) = predicate.validate() {
        if let Some(expected) = expected_err {
            assert_eq!(e, expected);
        } else {
            panic!("Unexpected error in predicate validation: {:?}", predicate);
        }
    } else if expected_err.is_some() {
        panic!(
            "Missing expected error for predicate validation: {:?}",
            predicate
        );
    }
}


#[test_case(&ALL_INVALID_SPEC_NETWORK_MAP, INVALID_SPEC_NETWORK_MAP_ERR.clone())]
fn it_validates_stacks_chainhook_specs(
    predicate: &ChainhookSpecificationNetworkMap,
    expected_err: String,
) {
    if let Err(e) = predicate.validate() {
        assert_eq!(e, expected_err);
    } else {
        panic!(
            "Missing expected error for predicate validation: {:?}",
            predicate
        );
    }
}

use crate::clarity::representations::{ClarityName, ContractName};
use crate::clarity::types::{
    AssetIdentifier, PrincipalData, QualifiedContractIdentifier, Value
};
use crate::clarity::types::serialization::SerializationError;
use crate::clarity::util::StacksAddress;
use crate::clarity::util::hash::{Hash160, hex_bytes, to_hex};
use super::StacksString;
use secp256k1::{Signature, RecoveryId};
use crate::clarity::util::secp256k1::Secp256k1PublicKey;
use serde::{Deserialize, Serialize};
use super::{read_next, read_next_exact, write_next, Error as CodecError, StacksMessageCodec};
use std::convert::TryFrom;
use std::io;
use std::io::{Read, Write};
use std::io::prelude::*;
use crate::clarity::util::retry::BoundReader;
use crate::clarity::codec::{MAX_TRANSACTION_LEN, MAX_MESSAGE_LEN};


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StacksTransaction {
    pub version: TransactionVersion,
    pub chain_id: u32,
    pub auth: TransactionAuth,
    pub anchor_mode: TransactionAnchorMode,
    pub post_condition_mode: TransactionPostConditionMode,
    pub post_conditions: Vec<TransactionPostCondition>,
    pub payload: TransactionPayload,
}

/// Stacks transaction versions
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize)]
pub enum TransactionVersion {
    Mainnet = 0x00,
    Testnet = 0x80,
}

/// Types of transaction authorizations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionAuth {
    Standard(TransactionSpendingCondition),
    Sponsored(TransactionSpendingCondition, TransactionSpendingCondition), // the second account pays on behalf of the first account
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionSpendingCondition {
    Singlesig(SinglesigSpendingCondition),
    Multisig(MultisigSpendingCondition),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SinglesigSpendingCondition {
    pub hash_mode: SinglesigHashMode,
    pub signer: Hash160,
    pub nonce: u64,  // nth authorization from this account
    pub tx_fee: u64, // microSTX/compute rate offerred by this account
    pub key_encoding: TransactionPublicKeyEncoding,
    pub signature: RecoverableSignature,
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SinglesigHashMode {
    P2PKH = 0x00,
    P2WPKH = 0x02,
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MultisigHashMode {
    P2SH = 0x01,
    P2WSH = 0x03,
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize)]
pub enum TransactionPublicKeyEncoding {
    // ways we can encode a public key
    Compressed = 0x00,
    Uncompressed = 0x01,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultisigSpendingCondition {
    pub hash_mode: MultisigHashMode,
    pub signer: Hash160,
    pub nonce: u64,  // nth authorization from this account
    pub tx_fee: u64, // microSTX/compute rate offered by this account
    pub fields: Vec<TransactionAuthField>,
    pub signatures_required: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionAuthField {
    PublicKey(Secp256k1PublicKey),
    Signature(TransactionPublicKeyEncoding, RecoverableSignature),
}

/// A transaction that instantiates a smart contract
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionSmartContract {
    pub name: ContractName,
    pub code_body: StacksString,
}

/// A transaction that calls into a smart contract
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionContractCall {
    pub address: StacksAddress,
    pub contract_name: ContractName,
    pub function_name: ClarityName,
    pub function_args: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionPayload {
    TokenTransfer(PrincipalData, u64, TokenTransferMemo),
    ContractCall(TransactionContractCall),
    SmartContract(TransactionSmartContract),
}

pub struct TokenTransferMemo(pub [u8; 34]); // same length as it is in stacks v1
impl_byte_array_message_codec!(TokenTransferMemo, 34);
impl_array_newtype!(TokenTransferMemo, u8, 34);
impl_array_hexstring_fmt!(TokenTransferMemo);
impl_byte_array_newtype!(TokenTransferMemo, u8, 34);
impl_byte_array_serde!(TokenTransferMemo);

impl_byte_array_message_codec!(Hash160, 20);

/// Post-condition on a transaction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionPostCondition {
    STX(PostConditionPrincipal, FungibleConditionCode, u64),
    Fungible(
        PostConditionPrincipal,
        AssetInfo,
        FungibleConditionCode,
        u64,
    ),
    Nonfungible(
        PostConditionPrincipal,
        AssetInfo,
        Value,
        NonfungibleConditionCode,
    ),
}

/// Post-condition modes for unspecified assets
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize)]
pub enum TransactionPostConditionMode {
    Allow = 0x01, // allow any other changes not specified
    Deny = 0x02,  // deny any other changes not specified
}


#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize)]
pub enum NonfungibleConditionCode {
    Sent = 0x10,
    NotSent = 0x11,
}


#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize)]
pub enum FungibleConditionCode {
    SentEq = 0x01,
    SentGt = 0x02,
    SentGe = 0x03,
    SentLt = 0x04,
    SentLe = 0x05,
}

/// Encoding of an asset type identifier
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetInfo {
    pub contract_address: StacksAddress,
    pub contract_name: ContractName,
    pub asset_name: ClarityName,
}

/// numeric wire-format ID of an asset info type variant
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize)]
pub enum AssetInfoID {
    STX = 0,
    FungibleAsset = 1,
    NonfungibleAsset = 2,
}

/// Post-condition principal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PostConditionPrincipal {
    Origin,
    Standard(StacksAddress),
    Contract(StacksAddress, ContractName),
}

/// How a transaction may be appended to the Stacks blockchain
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize)]
pub enum TransactionAnchorMode {
    OnChainOnly = 1,  // must be included in a StacksBlock
    OffChainOnly = 2, // must be included in a StacksMicroBlock
    Any = 3,          // either
}

pub struct RecoverableSignature(pub [u8; 65]);
impl_array_newtype!(RecoverableSignature, u8, 65);
impl_array_hexstring_fmt!(RecoverableSignature);
impl_byte_array_newtype!(RecoverableSignature, u8, 65);
impl_byte_array_serde!(RecoverableSignature);
impl_byte_array_message_codec!(RecoverableSignature, 65);
pub const MESSAGE_SIGNATURE_ENCODED_SIZE: u32 = 65;

impl RecoverableSignature {
    pub fn empty() -> RecoverableSignature {
        // NOTE: this cannot be a valid signature
        RecoverableSignature([0u8; 65])
    }

    #[cfg(test)]
    // test method for generating place-holder data
    pub fn from_raw(sig: &Vec<u8>) -> RecoverableSignature {
        let mut buf = [0u8; 65];
        if sig.len() < 65 {
            buf.copy_from_slice(&sig[..]);
        } else {
            buf.copy_from_slice(&sig[..65]);
        }
        RecoverableSignature(buf)
    }

    pub fn from_secp256k1_recoverable(sig: &Signature, recid: RecoveryId) -> RecoverableSignature {
        let bytes = sig.serialize();
        let mut ret_bytes = [0u8; 65];
        let recovery_id_byte = recid.serialize(); // recovery ID will be 0, 1, 2, or 3
        ret_bytes[0] = recovery_id_byte;
        for i in 0..64 {
            ret_bytes[i + 1] = bytes[i];
        }
        RecoverableSignature(ret_bytes)
    }
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize)]
pub enum TransactionPayloadID {
    TokenTransfer = 0,
    SmartContract = 1,
    ContractCall = 2,
    PoisonMicroblock = 3,
    Coinbase = 4,
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize)]
pub enum PostConditionPrincipalID {
    Origin = 0x01,
    Standard = 0x02,
    Contract = 0x03,
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize)]
pub enum TransactionAuthFlags {
    // types of auth
    AuthStandard = 0x04,
    AuthSponsored = 0x05,
}

/// Transaction signatures are validated by calculating the public key from the signature, and
/// verifying that all public keys hash to the signing account's hash.  To do so, we must preserve
/// enough information in the auth structure to recover each public key's bytes.
///
/// An auth field can be a public key or a signature.  In both cases, the public key (either given
/// in-the-raw or embedded in a signature) may be encoded as compressed or uncompressed.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize)]
pub enum TransactionAuthFieldID {
    // types of auth fields
    PublicKeyCompressed = 0x00,
    PublicKeyUncompressed = 0x01,
    SignatureCompressed = 0x02,
    SignatureUncompressed = 0x03,
}

/// A container for public keys (compressed secp256k1 public keys)
pub struct StacksPublicKeyBuffer(pub [u8; 33]);
impl_array_newtype!(StacksPublicKeyBuffer, u8, 33);
impl_array_hexstring_fmt!(StacksPublicKeyBuffer);
impl_byte_array_newtype!(StacksPublicKeyBuffer, u8, 33);
impl_byte_array_message_codec!(StacksPublicKeyBuffer, 33);

impl StacksPublicKeyBuffer {
    pub fn from_public_key(pubkey: &Secp256k1PublicKey) -> StacksPublicKeyBuffer {
        let pubkey_bytes_vec = pubkey.to_bytes_compressed();
        let mut pubkey_bytes = [0u8; 33];
        pubkey_bytes.copy_from_slice(&pubkey_bytes_vec[..]);
        StacksPublicKeyBuffer(pubkey_bytes)
    }

    pub fn to_public_key(&self) -> Result<Secp256k1PublicKey, CodecError> {
        Secp256k1PublicKey::from_slice(&self.0).map_err(|_e_str| {
            CodecError::DeserializeError("Failed to decode Stacks public key".to_string())
        })
    }
}

impl StacksTransaction {
    pub fn tx_len(&self) -> u64 {
        let mut tx_bytes = vec![];
        self.consensus_serialize(&mut tx_bytes)
            .expect("BUG: Failed to serialize a transaction object");
        tx_bytes.len() as u64
    }

    pub fn consensus_deserialize_with_len<R: Read>(
        fd: &mut R,
    ) -> Result<(StacksTransaction, u64), CodecError> {
        let mut bound_read = BoundReader::from_reader(fd, MAX_TRANSACTION_LEN.into());
        let fd = &mut bound_read;

        let version_u8: u8 = read_next(fd)?;
        let chain_id: u32 = read_next(fd)?;
        let auth: TransactionAuth = read_next(fd)?;
        let anchor_mode_u8: u8 = read_next(fd)?;
        let post_condition_mode_u8: u8 = read_next(fd)?;
        let post_conditions: Vec<TransactionPostCondition> = read_next(fd)?;

        let payload: TransactionPayload = read_next(fd)?;

        let version = if (version_u8 & 0x80) == 0 {
            TransactionVersion::Mainnet
        } else {
            TransactionVersion::Testnet
        };

        let anchor_mode = match anchor_mode_u8 {
            x if x == TransactionAnchorMode::OffChainOnly as u8 => {
                TransactionAnchorMode::OffChainOnly
            }
            x if x == TransactionAnchorMode::OnChainOnly as u8 => {
                TransactionAnchorMode::OnChainOnly
            }
            x if x == TransactionAnchorMode::Any as u8 => TransactionAnchorMode::Any,
            _ => {
                return Err(CodecError::DeserializeError(format!(
                    "Failed to parse transaction: invalid anchor mode {}",
                    anchor_mode_u8
                )));
            }
        };

        // if the payload is a proof of a poisoned microblock stream, or is a coinbase, then this _must_ be anchored.
        // Otherwise, if the offending leader is the next leader, they can just orphan their proof
        // of malfeasance.
        let post_condition_mode = match post_condition_mode_u8 {
            x if x == TransactionPostConditionMode::Allow as u8 => {
                TransactionPostConditionMode::Allow
            }
            x if x == TransactionPostConditionMode::Deny as u8 => {
                TransactionPostConditionMode::Deny
            }
            _ => {
                return Err(CodecError::DeserializeError(format!(
                    "Failed to parse transaction: invalid post-condition mode {}",
                    post_condition_mode_u8
                )));
            }
        };

        Ok((
            StacksTransaction {
                version,
                chain_id,
                auth,
                anchor_mode,
                post_condition_mode,
                post_conditions,
                payload,
            },
            fd.num_read(),
        ))
    }
}

impl TransactionPublicKeyEncoding {
    pub fn from_u8(n: u8) -> Option<TransactionPublicKeyEncoding> {
        match n {
            x if x == TransactionPublicKeyEncoding::Compressed as u8 => {
                Some(TransactionPublicKeyEncoding::Compressed)
            }
            x if x == TransactionPublicKeyEncoding::Uncompressed as u8 => {
                Some(TransactionPublicKeyEncoding::Uncompressed)
            }
            _ => None,
        }
    }
}

impl SinglesigHashMode {
    pub fn from_u8(n: u8) -> Option<SinglesigHashMode> {
        match n {
            x if x == SinglesigHashMode::P2PKH as u8 => Some(SinglesigHashMode::P2PKH),
            x if x == SinglesigHashMode::P2WPKH as u8 => Some(SinglesigHashMode::P2WPKH),
            _ => None,
        }
    }
}

impl MultisigHashMode {
    pub fn from_u8(n: u8) -> Option<MultisigHashMode> {
        match n {
            x if x == MultisigHashMode::P2SH as u8 => Some(MultisigHashMode::P2SH),
            x if x == MultisigHashMode::P2WSH as u8 => Some(MultisigHashMode::P2WSH),
            _ => None,
        }
    }
}

impl AssetInfoID {
    pub fn from_u8(b: u8) -> Option<AssetInfoID> {
        match b {
            0 => Some(AssetInfoID::STX),
            1 => Some(AssetInfoID::FungibleAsset),
            2 => Some(AssetInfoID::NonfungibleAsset),
            _ => None,
        }
    }
}

impl FungibleConditionCode {
    pub fn from_u8(b: u8) -> Option<FungibleConditionCode> {
        match b {
            0x01 => Some(FungibleConditionCode::SentEq),
            0x02 => Some(FungibleConditionCode::SentGt),
            0x03 => Some(FungibleConditionCode::SentGe),
            0x04 => Some(FungibleConditionCode::SentLt),
            0x05 => Some(FungibleConditionCode::SentLe),
            _ => None,
        }
    }
}

impl NonfungibleConditionCode {
    pub fn from_u8(b: u8) -> Option<NonfungibleConditionCode> {
        match b {
            0x10 => Some(NonfungibleConditionCode::Sent),
            0x11 => Some(NonfungibleConditionCode::NotSent),
            _ => None,
        }
    }
}

impl StacksMessageCodec for Value {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        self.serialize_write(fd).map_err(CodecError::WriteError)
    }

    fn consensus_deserialize<R: Read>(fd: &mut R) -> Result<Value, CodecError> {
        Value::deserialize_read(fd, None).map_err(|e| match e {
            SerializationError::IOError(e) => CodecError::ReadError(e.err),
            _ => CodecError::DeserializeError(format!("Failed to decode clarity value: {:?}", &e)),
        })
    }
}

impl StacksMessageCodec for TransactionContractCall {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        write_next(fd, &self.address)?;
        write_next(fd, &self.contract_name)?;
        write_next(fd, &self.function_name)?;
        write_next(fd, &self.function_args)?;
        Ok(())
    }

    fn consensus_deserialize<R: Read>(fd: &mut R) -> Result<TransactionContractCall, CodecError> {
        let address: StacksAddress = read_next(fd)?;
        let contract_name: ContractName = read_next(fd)?;
        let function_name: ClarityName = read_next(fd)?;
        let function_args: Vec<Value> = {
            let mut bound_read = BoundReader::from_reader(fd, MAX_TRANSACTION_LEN as u64);
            read_next(&mut bound_read)
        }?;

        // function name must be valid Clarity variable
        if !StacksString::from(function_name.clone()).is_clarity_variable() {
            return Err(CodecError::DeserializeError(
                "Failed to parse transaction: invalid function name -- not a Clarity variable"
                    .to_string(),
            ));
        }

        Ok(TransactionContractCall {
            address,
            contract_name,
            function_name,
            function_args,
        })
    }
}

impl StacksMessageCodec for TransactionSmartContract {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        write_next(fd, &self.name)?;
        write_next(fd, &self.code_body)?;
        Ok(())
    }

    fn consensus_deserialize<R: Read>(fd: &mut R) -> Result<TransactionSmartContract, CodecError> {
        let name: ContractName = read_next(fd)?;
        let code_body: StacksString = read_next(fd)?;
        Ok(TransactionSmartContract { name, code_body })
    }
}

impl StacksMessageCodec for TransactionPayload {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        match *self {
            TransactionPayload::TokenTransfer(ref address, ref amount, ref memo) => {
                write_next(fd, &(TransactionPayloadID::TokenTransfer as u8))?;
                write_next(fd, address)?;
                write_next(fd, amount)?;
                write_next(fd, memo)?;
            }
            TransactionPayload::ContractCall(ref cc) => {
                write_next(fd, &(TransactionPayloadID::ContractCall as u8))?;
                cc.consensus_serialize(fd)?;
            }
            TransactionPayload::SmartContract(ref sc) => {
                write_next(fd, &(TransactionPayloadID::SmartContract as u8))?;
                sc.consensus_serialize(fd)?;
            }
        }
        Ok(())
    }

    fn consensus_deserialize<R: Read>(fd: &mut R) -> Result<TransactionPayload, CodecError> {
        let type_id: u8 = read_next(fd)?;
        let payload = match type_id {
            x if x == TransactionPayloadID::TokenTransfer as u8 => {
                let principal = read_next(fd)?;
                let amount = read_next(fd)?;
                let memo = read_next(fd)?;
                TransactionPayload::TokenTransfer(principal, amount, memo)
            }
            x if x == TransactionPayloadID::ContractCall as u8 => {
                let payload: TransactionContractCall = read_next(fd)?;
                TransactionPayload::ContractCall(payload)
            }
            x if x == TransactionPayloadID::SmartContract as u8 => {
                let payload: TransactionSmartContract = read_next(fd)?;
                TransactionPayload::SmartContract(payload)
            }
            _ => {
                return Err(CodecError::DeserializeError(format!(
                    "Failed to parse transaction -- unknown payload ID {}",
                    type_id
                )));
            }
        };

        Ok(payload)
    }
}


impl StacksMessageCodec for AssetInfo {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        write_next(fd, &self.contract_address)?;
        write_next(fd, &self.contract_name)?;
        write_next(fd, &self.asset_name)?;
        Ok(())
    }

    fn consensus_deserialize<R: Read>(fd: &mut R) -> Result<AssetInfo, CodecError> {
        let contract_address: StacksAddress = read_next(fd)?;
        let contract_name: ContractName = read_next(fd)?;
        let asset_name: ClarityName = read_next(fd)?;
        Ok(AssetInfo {
            contract_address,
            contract_name,
            asset_name,
        })
    }
}

impl StacksMessageCodec for PostConditionPrincipal {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        match *self {
            PostConditionPrincipal::Origin => {
                write_next(fd, &(PostConditionPrincipalID::Origin as u8))?;
            }
            PostConditionPrincipal::Standard(ref address) => {
                write_next(fd, &(PostConditionPrincipalID::Standard as u8))?;
                write_next(fd, address)?;
            }
            PostConditionPrincipal::Contract(ref address, ref contract_name) => {
                write_next(fd, &(PostConditionPrincipalID::Contract as u8))?;
                write_next(fd, address)?;
                write_next(fd, contract_name)?;
            }
        }
        Ok(())
    }

    fn consensus_deserialize<R: Read>(fd: &mut R) -> Result<PostConditionPrincipal, CodecError> {
        let principal_id: u8 = read_next(fd)?;
        let principal = match principal_id {
            x if x == PostConditionPrincipalID::Origin as u8 => PostConditionPrincipal::Origin,
            x if x == PostConditionPrincipalID::Standard as u8 => {
                let addr: StacksAddress = read_next(fd)?;
                PostConditionPrincipal::Standard(addr)
            }
            x if x == PostConditionPrincipalID::Contract as u8 => {
                let addr: StacksAddress = read_next(fd)?;
                let contract_name: ContractName = read_next(fd)?;
                PostConditionPrincipal::Contract(addr, contract_name)
            }
            _ => {
                return Err(CodecError::DeserializeError(format!(
                    "Failed to parse transaction: unknown post condition principal ID {}",
                    principal_id
                )));
            }
        };
        Ok(principal)
    }
}

impl StacksMessageCodec for TransactionPostCondition {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        match *self {
            TransactionPostCondition::STX(ref principal, ref fungible_condition, ref amount) => {
                write_next(fd, &(AssetInfoID::STX as u8))?;
                write_next(fd, principal)?;
                write_next(fd, &(*fungible_condition as u8))?;
                write_next(fd, amount)?;
            }
            TransactionPostCondition::Fungible(
                ref principal,
                ref asset_info,
                ref fungible_condition,
                ref amount,
            ) => {
                write_next(fd, &(AssetInfoID::FungibleAsset as u8))?;
                write_next(fd, principal)?;
                write_next(fd, asset_info)?;
                write_next(fd, &(*fungible_condition as u8))?;
                write_next(fd, amount)?;
            }
            TransactionPostCondition::Nonfungible(
                ref principal,
                ref asset_info,
                ref asset_value,
                ref nonfungible_condition,
            ) => {
                write_next(fd, &(AssetInfoID::NonfungibleAsset as u8))?;
                write_next(fd, principal)?;
                write_next(fd, asset_info)?;
                write_next(fd, asset_value)?;
                write_next(fd, &(*nonfungible_condition as u8))?;
            }
        };
        Ok(())
    }

    fn consensus_deserialize<R: Read>(fd: &mut R) -> Result<TransactionPostCondition, CodecError> {
        let asset_info_id: u8 = read_next(fd)?;
        let postcond = match asset_info_id {
            x if x == AssetInfoID::STX as u8 => {
                let principal: PostConditionPrincipal = read_next(fd)?;
                let condition_u8: u8 = read_next(fd)?;
                let amount: u64 = read_next(fd)?;

                let condition_code = FungibleConditionCode::from_u8(condition_u8).ok_or(
                    CodecError::DeserializeError(format!(
                    "Failed to parse transaction: Failed to parse STX fungible condition code {}",
                    condition_u8
                )),
                )?;

                TransactionPostCondition::STX(principal, condition_code, amount)
            }
            x if x == AssetInfoID::FungibleAsset as u8 => {
                let principal: PostConditionPrincipal = read_next(fd)?;
                let asset: AssetInfo = read_next(fd)?;
                let condition_u8: u8 = read_next(fd)?;
                let amount: u64 = read_next(fd)?;

                let condition_code = FungibleConditionCode::from_u8(condition_u8).ok_or(
                    CodecError::DeserializeError(format!(
                    "Failed to parse transaction: Failed to parse FungibleAsset condition code {}",
                    condition_u8
                )),
                )?;

                TransactionPostCondition::Fungible(principal, asset, condition_code, amount)
            }
            x if x == AssetInfoID::NonfungibleAsset as u8 => {
                let principal: PostConditionPrincipal = read_next(fd)?;
                let asset: AssetInfo = read_next(fd)?;
                let asset_value: Value = read_next(fd)?;
                let condition_u8: u8 = read_next(fd)?;

                let condition_code = NonfungibleConditionCode::from_u8(condition_u8)
                    .ok_or(CodecError::DeserializeError(format!("Failed to parse transaction: Failed to parse NonfungibleAsset condition code {}", condition_u8)))?;

                TransactionPostCondition::Nonfungible(principal, asset, asset_value, condition_code)
            }
            _ => {
                return Err(CodecError::DeserializeError(format!(
                    "Failed to aprse transaction: unknown asset info ID {}",
                    asset_info_id
                )));
            }
        };

        Ok(postcond)
    }
}

impl StacksMessageCodec for TransactionAuth {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        match *self {
            TransactionAuth::Standard(ref origin_condition) => {
                write_next(fd, &(TransactionAuthFlags::AuthStandard as u8))?;
                write_next(fd, origin_condition)?;
            }
            TransactionAuth::Sponsored(ref origin_condition, ref sponsor_condition) => {
                write_next(fd, &(TransactionAuthFlags::AuthSponsored as u8))?;
                write_next(fd, origin_condition)?;
                write_next(fd, sponsor_condition)?;
            }
        }
        Ok(())
    }

    fn consensus_deserialize<R: Read>(fd: &mut R) -> Result<TransactionAuth, CodecError> {
        let type_id: u8 = read_next(fd)?;
        let auth = match type_id {
            x if x == TransactionAuthFlags::AuthStandard as u8 => {
                let origin_auth: TransactionSpendingCondition = read_next(fd)?;
                TransactionAuth::Standard(origin_auth)
            }
            x if x == TransactionAuthFlags::AuthSponsored as u8 => {
                let origin_auth: TransactionSpendingCondition = read_next(fd)?;
                let sponsor_auth: TransactionSpendingCondition = read_next(fd)?;
                TransactionAuth::Sponsored(origin_auth, sponsor_auth)
            }
            _ => {
                return Err(CodecError::DeserializeError(format!(
                    "Failed to parse transaction authorization: unrecognized auth flags {}",
                    type_id
                )));
            }
        };
        Ok(auth)
    }
}

impl StacksMessageCodec for TransactionSpendingCondition {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        match *self {
            TransactionSpendingCondition::Singlesig(ref data) => {
                data.consensus_serialize(fd)?;
            }
            TransactionSpendingCondition::Multisig(ref data) => {
                data.consensus_serialize(fd)?;
            }
        }
        Ok(())
    }

    fn consensus_deserialize<R: Read>(
        fd: &mut R,
    ) -> Result<TransactionSpendingCondition, CodecError> {
        // peek the hash mode byte
        let hash_mode_u8: u8 = read_next(fd)?;
        let peek_buf = [hash_mode_u8];
        let mut rrd = peek_buf.chain(fd);
        let cond = {
            if SinglesigHashMode::from_u8(hash_mode_u8).is_some() {
                let cond = SinglesigSpendingCondition::consensus_deserialize(&mut rrd)?;
                TransactionSpendingCondition::Singlesig(cond)
            } else if MultisigHashMode::from_u8(hash_mode_u8).is_some() {
                let cond = MultisigSpendingCondition::consensus_deserialize(&mut rrd)?;
                TransactionSpendingCondition::Multisig(cond)
            } else {
                return Err(CodecError::DeserializeError(format!(
                    "Failed to parse spending condition: invalid hash mode {}",
                    hash_mode_u8
                )));
            }
        };

        Ok(cond)
    }
}

impl StacksMessageCodec for SinglesigSpendingCondition {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        write_next(fd, &(self.hash_mode.clone() as u8))?;
        write_next(fd, &self.signer)?;
        write_next(fd, &self.nonce)?;
        write_next(fd, &self.tx_fee)?;
        write_next(fd, &(self.key_encoding.clone() as u8))?;
        write_next(fd, &self.signature)?;
        Ok(())
    }

    fn consensus_deserialize<R: Read>(fd: &mut R) -> Result<SinglesigSpendingCondition, CodecError> {
        let hash_mode_u8: u8 = read_next(fd)?;
        let hash_mode = SinglesigHashMode::from_u8(hash_mode_u8).ok_or(
            CodecError::DeserializeError(format!(
                "Failed to parse singlesig spending condition: unknown hash mode {}",
                hash_mode_u8
            )),
        )?;

        let bytes: Vec<u8> = read_next_exact::<_, u8>(fd, 20)?;
        let signer = Hash160::from_data(&bytes);
        let nonce: u64 = read_next(fd)?;
        let tx_fee: u64 = read_next(fd)?;

        let key_encoding_u8: u8 = read_next(fd)?;
        let key_encoding = TransactionPublicKeyEncoding::from_u8(key_encoding_u8).ok_or(
            CodecError::DeserializeError(format!(
                "Failed to parse singlesig spending condition: unknown key encoding {}",
                key_encoding_u8
            )),
        )?;

        let signature: RecoverableSignature = read_next(fd)?;

        // sanity check -- must be compressed if we're using p2wpkh
        if hash_mode == SinglesigHashMode::P2WPKH
            && key_encoding != TransactionPublicKeyEncoding::Compressed
        {
            return Err(CodecError::DeserializeError("Failed to parse singlesig spending condition: incomaptible hash mode and key encoding".to_string()));
        }

        Ok(SinglesigSpendingCondition {
            signer: signer,
            nonce: nonce,
            tx_fee: tx_fee,
            hash_mode: hash_mode,
            key_encoding: key_encoding,
            signature: signature,
        })
    }
}

impl StacksMessageCodec for MultisigSpendingCondition {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        write_next(fd, &(self.hash_mode.clone() as u8))?;
        write_next(fd, &self.signer)?;
        write_next(fd, &self.nonce)?;
        write_next(fd, &self.tx_fee)?;
        write_next(fd, &self.fields)?;
        write_next(fd, &self.signatures_required)?;
        Ok(())
    }

    fn consensus_deserialize<R: Read>(fd: &mut R) -> Result<MultisigSpendingCondition, CodecError> {
        let hash_mode_u8: u8 = read_next(fd)?;
        let hash_mode =
            MultisigHashMode::from_u8(hash_mode_u8).ok_or(CodecError::DeserializeError(format!(
                "Failed to parse multisig spending condition: unknown hash mode {}",
                hash_mode_u8
            )))?;

        let bytes: Vec<u8> = read_next_exact::<_, u8>(fd, 20)?;
        let signer = Hash160::from_data(&bytes);
        let nonce: u64 = read_next(fd)?;
        let tx_fee: u64 = read_next(fd)?;
        let fields: Vec<TransactionAuthField> = {
            let mut bound_read = BoundReader::from_reader(fd, MAX_MESSAGE_LEN as u64);
            read_next(&mut bound_read)
        }?;

        let signatures_required: u16 = read_next(fd)?;

        // read and decode _exactly_ num_signatures signature buffers
        let mut num_sigs_given: u16 = 0;
        let mut have_uncompressed = false;
        for f in fields.iter() {
            match *f {
                TransactionAuthField::Signature(ref key_encoding, _) => {
                    num_sigs_given =
                        num_sigs_given
                            .checked_add(1)
                            .ok_or(CodecError::DeserializeError(
                                "Failed to parse multisig spending condition: too many signatures"
                                    .to_string(),
                            ))?;
                    if *key_encoding == TransactionPublicKeyEncoding::Uncompressed {
                        have_uncompressed = true;
                    }
                }
                TransactionAuthField::PublicKey(ref pubk) => {
                    if !pubk.compressed() {
                        have_uncompressed = true;
                    }
                }
            };
        }

        // must be given the right number of signatures
        if num_sigs_given != signatures_required {
            return Err(CodecError::DeserializeError(format!(
                "Failed to parse multisig spending condition: got {} sigs, expected {}",
                num_sigs_given, signatures_required
            )));
        }

        // must all be compressed if we're using P2WSH
        if have_uncompressed && hash_mode == MultisigHashMode::P2WSH {
            return Err(CodecError::DeserializeError(
                "Failed to parse multisig spending condition: expected compressed keys only"
                    .to_string(),
            ));
        }

        Ok(MultisigSpendingCondition {
            signer,
            nonce,
            tx_fee,
            hash_mode,
            fields,
            signatures_required,
        })
    }
}

impl StacksMessageCodec for TransactionAuthField {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        match *self {
            TransactionAuthField::PublicKey(ref pubk) => {
                let field_id = if pubk.compressed() {
                    TransactionAuthFieldID::PublicKeyCompressed
                } else {
                    TransactionAuthFieldID::PublicKeyUncompressed
                };

                let pubkey_buf = StacksPublicKeyBuffer::from_public_key(pubk);

                write_next(fd, &(field_id as u8))?;
                write_next(fd, &pubkey_buf)?;
            }
            TransactionAuthField::Signature(ref key_encoding, ref sig) => {
                let field_id = if *key_encoding == TransactionPublicKeyEncoding::Compressed {
                    TransactionAuthFieldID::SignatureCompressed
                } else {
                    TransactionAuthFieldID::SignatureUncompressed
                };

                write_next(fd, &(field_id as u8))?;
                write_next(fd, sig)?;
            }
        }
        Ok(())
    }

    fn consensus_deserialize<R: Read>(fd: &mut R) -> Result<TransactionAuthField, CodecError> {
        let field_id: u8 = read_next(fd)?;
        let field = match field_id {
            x if x == TransactionAuthFieldID::PublicKeyCompressed as u8 => {
                let pubkey_buf: StacksPublicKeyBuffer = read_next(fd)?;
                let mut pubkey = pubkey_buf.to_public_key()?;
                pubkey.set_compressed(true);

                TransactionAuthField::PublicKey(pubkey)
            }
            x if x == TransactionAuthFieldID::PublicKeyUncompressed as u8 => {
                let pubkey_buf: StacksPublicKeyBuffer = read_next(fd)?;
                let mut pubkey = pubkey_buf.to_public_key()?;
                pubkey.set_compressed(false);

                TransactionAuthField::PublicKey(pubkey)
            }
            x if x == TransactionAuthFieldID::SignatureCompressed as u8 => {
                let sig: RecoverableSignature = read_next(fd)?;
                TransactionAuthField::Signature(TransactionPublicKeyEncoding::Compressed, sig)
            }
            x if x == TransactionAuthFieldID::SignatureUncompressed as u8 => {
                let sig: RecoverableSignature = read_next(fd)?;
                TransactionAuthField::Signature(TransactionPublicKeyEncoding::Uncompressed, sig)
            }
            _ => {
                return Err(CodecError::DeserializeError(format!(
                    "Failed to parse auth field: unkonwn auth field ID {}",
                    field_id
                )));
            }
        };
        Ok(field)
    }
}

impl StacksMessageCodec for StacksTransaction {
    fn consensus_serialize<W: Write>(&self, fd: &mut W) -> Result<(), CodecError> {
        write_next(fd, &(self.version as u8))?;
        write_next(fd, &self.chain_id)?;
        write_next(fd, &self.auth)?;
        write_next(fd, &(self.anchor_mode as u8))?;
        write_next(fd, &(self.post_condition_mode as u8))?;
        write_next(fd, &self.post_conditions)?;
        write_next(fd, &self.payload)?;
        Ok(())
    }

    fn consensus_deserialize<R: Read>(fd: &mut R) -> Result<StacksTransaction, CodecError> {
        StacksTransaction::consensus_deserialize_with_len(fd).map(|(result, _)| result)
    }
}


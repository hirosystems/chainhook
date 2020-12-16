// Copyright (C) 2013-2020 Blockstack PBC, a public benefit corporation
// Copyright (C) 2020 Stacks Open Internet Foundation
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use secp256k1;
use secp256k1::constants as LibSecp256k1Constants;
use secp256k1::recovery::RecoverableSignature as LibSecp256k1RecoverableSignature;
use secp256k1::recovery::RecoveryId as LibSecp256k1RecoveryID;
use secp256k1::Error as LibSecp256k1Error;
use secp256k1::Message as LibSecp256k1Message;
use secp256k1::PublicKey as LibSecp256k1PublicKey;
use secp256k1::Secp256k1;
use secp256k1::SecretKey as LibSecp256k1PrivateKey;
use secp256k1::Signature as LibSecp256k1Signature;

use super::hash::{hex_bytes, to_hex};

use serde::de::Deserialize;
use serde::de::Error as de_Error;
use serde::ser::Error as ser_Error;
use serde::Serialize;

// per-thread Secp256k1 context
thread_local!(static _secp256k1: Secp256k1<secp256k1::All> = Secp256k1::new());

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub struct Secp256k1PublicKey {
    // serde is broken for secp256k1, so do it ourselves
    #[serde(
        serialize_with = "secp256k1_pubkey_serialize",
        deserialize_with = "secp256k1_pubkey_deserialize"
    )]
    key: LibSecp256k1PublicKey,
    compressed: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub struct Secp256k1PrivateKey {
    // serde is broken for secp256k1, so do it ourselves
    #[serde(
        serialize_with = "secp256k1_privkey_serialize",
        deserialize_with = "secp256k1_privkey_deserialize"
    )]
    key: LibSecp256k1PrivateKey,
    compress_public: bool,
}

pub struct MessageSignature(pub [u8; 65]);
impl_array_newtype!(MessageSignature, u8, 65);
impl_array_hexstring_fmt!(MessageSignature);
impl_byte_array_newtype!(MessageSignature, u8, 65);
impl_byte_array_serde!(MessageSignature);
pub const MESSAGE_SIGNATURE_ENCODED_SIZE: u32 = 65;

impl MessageSignature {
    pub fn empty() -> MessageSignature {
        // NOTE: this cannot be a valid signature
        MessageSignature([0u8; 65])
    }

    #[cfg(test)]
    // test method for generating place-holder data
    pub fn from_raw(sig: &Vec<u8>) -> MessageSignature {
        let mut buf = [0u8; 65];
        if sig.len() < 65 {
            buf.copy_from_slice(&sig[..]);
        } else {
            buf.copy_from_slice(&sig[..65]);
        }
        MessageSignature(buf)
    }

    pub fn from_secp256k1_recoverable(sig: &LibSecp256k1RecoverableSignature) -> MessageSignature {
        let (recid, bytes) = sig.serialize_compact();
        let mut ret_bytes = [0u8; 65];
        let recovery_id_byte = recid.to_i32() as u8; // recovery ID will be 0, 1, 2, or 3
        ret_bytes[0] = recovery_id_byte;
        for i in 0..64 {
            ret_bytes[i + 1] = bytes[i];
        }
        MessageSignature(ret_bytes)
    }

    pub fn to_secp256k1_recoverable(&self) -> Option<LibSecp256k1RecoverableSignature> {
        let recid = match LibSecp256k1RecoveryID::from_i32(self.0[0] as i32) {
            Ok(rid) => rid,
            Err(_) => {
                return None;
            }
        };
        let mut sig_bytes = [0u8; 64];
        for i in 0..64 {
            sig_bytes[i] = self.0[i + 1];
        }

        match LibSecp256k1RecoverableSignature::from_compact(&sig_bytes, recid) {
            Ok(sig) => Some(sig),
            Err(_) => None,
        }
    }
}

impl Secp256k1PublicKey {
    pub fn from_hex(hex_string: &str) -> Result<Secp256k1PublicKey, &'static str> {
        let data = hex_bytes(hex_string).map_err(|_e| "Failed to decode hex public key")?;
        Secp256k1PublicKey::from_slice(&data[..]).map_err(|_e| "Invalid public key hex string")
    }

    pub fn from_slice(data: &[u8]) -> Result<Secp256k1PublicKey, &'static str> {
        match LibSecp256k1PublicKey::from_slice(data) {
            Ok(pubkey_res) => Ok(Secp256k1PublicKey {
                key: pubkey_res,
                compressed: data.len() == LibSecp256k1Constants::PUBLIC_KEY_SIZE,
            }),
            Err(_e) => Err("Invalid public key: failed to load"),
        }
    }

    pub fn from_private(privk: &Secp256k1PrivateKey) -> Secp256k1PublicKey {
        _secp256k1.with(|ctx| {
            let pubk = LibSecp256k1PublicKey::from_secret_key(&ctx, &privk.key);
            Secp256k1PublicKey {
                key: pubk,
                compressed: privk.compress_public,
            }
        })
    }

    pub fn to_hex(&self) -> String {
        if self.compressed {
            to_hex(&self.key.serialize().to_vec())
        } else {
            to_hex(&self.key.serialize_uncompressed().to_vec())
        }
    }

    pub fn to_bytes_compressed(&self) -> Vec<u8> {
        self.key.serialize().to_vec()
    }

    pub fn compressed(&self) -> bool {
        self.compressed
    }

    pub fn set_compressed(&mut self, value: bool) {
        self.compressed = value;
    }

    /// recover message and signature to public key (will be compressed)
    pub fn recover_to_pubkey(
        msg: &[u8],
        sig: &MessageSignature,
    ) -> Result<Secp256k1PublicKey, &'static str> {
        _secp256k1.with(|ctx| {
            let msg = LibSecp256k1Message::from_slice(msg).map_err(|_e| {
                "Invalid message: failed to decode data hash: must be a 32-byte hash"
            })?;

            let secp256k1_sig = sig
                .to_secp256k1_recoverable()
                .ok_or("Invalid signature: failed to decode recoverable signature")?;

            let recovered_pubkey = ctx
                .recover(&msg, &secp256k1_sig)
                .map_err(|_e| "Invalid signature: failed to recover public key")?;

            Ok(Secp256k1PublicKey {
                key: recovered_pubkey,
                compressed: true,
            })
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        if self.compressed {
            self.key.serialize().to_vec()
        } else {
            self.key.serialize_uncompressed().to_vec()
        }
    }
}

impl Secp256k1PrivateKey {
    pub fn from_hex(hex_string: &str) -> Result<Secp256k1PrivateKey, &'static str> {
        let data = hex_bytes(hex_string).map_err(|_e| "Failed to decode hex private key")?;
        Secp256k1PrivateKey::from_slice(&data[..]).map_err(|_e| "Invalid private key hex string")
    }

    pub fn from_slice(data: &[u8]) -> Result<Secp256k1PrivateKey, &'static str> {
        if data.len() < 32 {
            return Err("Invalid private key: shorter than 32 bytes");
        }
        if data.len() > 33 {
            return Err("Invalid private key: greater than 33 bytes");
        }
        let compress_public = if data.len() == 33 {
            // compressed byte tag?
            if data[32] != 0x01 {
                return Err("Invalid private key: invalid compressed byte marker");
            }
            true
        } else {
            false
        };
        match LibSecp256k1PrivateKey::from_slice(&data[0..32]) {
            Ok(privkey_res) => Ok(Secp256k1PrivateKey {
                key: privkey_res,
                compress_public: compress_public,
            }),
            Err(_e) => Err("Invalid private key: failed to load"),
        }
    }

    pub fn compress_public(&self) -> bool {
        self.compress_public
    }

    pub fn set_compress_public(&mut self, value: bool) {
        self.compress_public = value;
    }

    pub fn to_hex(&self) -> String {
        let mut bytes = self.key[..].to_vec();
        if self.compress_public {
            bytes.push(1);
        }
        to_hex(&bytes)
    }
}

fn secp256k1_pubkey_serialize<S: serde::Serializer>(
    pubk: &LibSecp256k1PublicKey,
    s: S,
) -> Result<S::Ok, S::Error> {
    let key_hex = to_hex(&pubk.serialize().to_vec());
    s.serialize_str(&key_hex.as_str())
}

fn secp256k1_pubkey_deserialize<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<LibSecp256k1PublicKey, D::Error> {
    let key_hex = String::deserialize(d)?;
    let key_bytes = hex_bytes(&key_hex).map_err(de_Error::custom)?;

    LibSecp256k1PublicKey::from_slice(&key_bytes[..]).map_err(de_Error::custom)
}

fn secp256k1_privkey_serialize<S: serde::Serializer>(
    privk: &LibSecp256k1PrivateKey,
    s: S,
) -> Result<S::Ok, S::Error> {
    let key_hex = to_hex(&privk[..].to_vec());
    s.serialize_str(&key_hex.as_str())
}

fn secp256k1_privkey_deserialize<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<LibSecp256k1PrivateKey, D::Error> {
    let key_hex = String::deserialize(d)?;
    let key_bytes = hex_bytes(&key_hex).map_err(de_Error::custom)?;

    LibSecp256k1PrivateKey::from_slice(&key_bytes[..]).map_err(de_Error::custom)
}

pub fn secp256k1_recover(
    message_arr: &[u8],
    serialized_signature_arr: &[u8],
) -> Result<[u8; 33], LibSecp256k1Error> {
    _secp256k1.with(|ctx| {
        let message = LibSecp256k1Message::from_slice(message_arr)?;

        let rec_id = LibSecp256k1RecoveryID::from_i32(serialized_signature_arr[64] as i32)?;
        let recovered_sig = LibSecp256k1RecoverableSignature::from_compact(
            &serialized_signature_arr[..64],
            rec_id,
        )?;
        let recovered_pub = ctx.recover(&message, &recovered_sig)?;
        let recovered_serialized = recovered_pub.serialize(); // 33 bytes version

        Ok(recovered_serialized)
    })
}

pub fn secp256k1_verify(
    message_arr: &[u8],
    serialized_signature_arr: &[u8],
    pubkey_arr: &[u8],
) -> Result<(), LibSecp256k1Error> {
    _secp256k1.with(|ctx| {
        let message = LibSecp256k1Message::from_slice(message_arr)?;
        let expanded_sig = LibSecp256k1Signature::from_compact(&serialized_signature_arr[..64])?; // ignore 65th byte if present
        let pubkey = LibSecp256k1PublicKey::from_slice(pubkey_arr)?;

        ctx.verify(&message, &expanded_sig, &pubkey)
    })
}

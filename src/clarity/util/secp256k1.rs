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
use secp256k1::Error as LibSecp256k1Error;
use secp256k1::Message as LibSecp256k1Message;
use secp256k1::PublicKey as LibSecp256k1PublicKey;
use secp256k1::SecretKey as LibSecp256k1PrivateKey;
use secp256k1::Signature as LibSecp256k1Signature;

use super::hash::{hex_bytes, to_hex};

use serde::de::Deserialize;
use serde::de::Error as de_Error;
use serde::ser::Error as ser_Error;
use serde::Serialize;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Secp256k1PublicKey {
    key: LibSecp256k1PublicKey,
    compressed: bool,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Secp256k1PrivateKey {
    key: LibSecp256k1PrivateKey,
    compress_public: bool,
}

impl Secp256k1PublicKey {

    pub fn from_slice(data: &[u8]) -> Result<Secp256k1PublicKey, &'static str> {
        match LibSecp256k1PublicKey::parse_slice(data, Some(secp256k1::PublicKeyFormat::Compressed)) {
            Ok(pubkey_res) => Ok(Secp256k1PublicKey {
                key: pubkey_res,
                compressed: true,
            }),
            Err(_e) => Err("Invalid public key: failed to load"),
        }
    }

    pub fn to_hex(&self) -> String {
        if self.compressed {
            to_hex(&self.key.serialize_compressed().to_vec())
        } else {
            to_hex(&self.key.serialize().to_vec())
        }
    }

    pub fn to_bytes_compressed(&self) -> Vec<u8> {
        self.key.serialize_compressed().to_vec()
    }

    pub fn compressed(&self) -> bool {
        self.compressed
    }

    pub fn set_compressed(&mut self, value: bool) {
        self.compressed = value;
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        if self.compressed {
            self.key.serialize_compressed().to_vec()
        } else {
            self.key.serialize().to_vec()
        }
    }
}

fn secp256k1_pubkey_serialize<S: serde::Serializer>(
    pubk: &LibSecp256k1PublicKey,
    s: S,
) -> Result<S::Ok, S::Error> {
    let key_hex = to_hex(&pubk.serialize().to_vec());
    s.serialize_str(&key_hex.as_str())
}

pub fn secp256k1_recover(
    message_arr: &[u8],
    serialized_signature: &[u8],
) -> Result<[u8; 33], LibSecp256k1Error> {
    let recovery_id = secp256k1::RecoveryId::parse(serialized_signature[64] as u8)?;
    let message = LibSecp256k1Message::parse_slice(message_arr)?;
    let signature = LibSecp256k1Signature::parse_slice(&serialized_signature[..64])?;
    let recovered_pub_key = secp256k1::recover(&message, &signature, &recovery_id)?;
    Ok(recovered_pub_key.serialize_compressed())
}

pub fn secp256k1_verify(
    message_arr: &[u8],
    serialized_signature: &[u8],
    pubkey_arr: &[u8],
) -> Result<(), LibSecp256k1Error> {

    let message = LibSecp256k1Message::parse_slice(message_arr)?;
    let signature = LibSecp256k1Signature::parse_slice(&serialized_signature[..64])?; // ignore 65th byte if present
    let pubkey = LibSecp256k1PublicKey::parse_slice(pubkey_arr, Some(secp256k1::PublicKeyFormat::Compressed))?;

    let res = secp256k1::verify(&message, &signature, &pubkey);
    if res { Ok(()) } else { Err(LibSecp256k1Error::InvalidPublicKey) }
}

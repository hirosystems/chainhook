use std::path::PathBuf;

use chainhook_sdk::{
    types::{BlockRejectReasonCode, BlockResponseData, BlockValidationFailedCode},
    utils::Context,
};
use rusqlite::Connection;

use super::sqlite::create_or_open_readwrite_db;

fn get_default_signers_db_file_path(base_dir: &PathBuf) -> PathBuf {
    let mut destination_path = base_dir.clone();
    destination_path.push("stacks_signers.sqlite");
    destination_path
}

pub fn initialize_signers_db(base_dir: &PathBuf, ctx: &Context) -> Result<Connection, String> {
    let conn = create_or_open_readwrite_db(Some(&get_default_signers_db_file_path(base_dir)), ctx)?;

    // Stores message headers
    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pubkey TEXT NOT NULL,
            contract TEXT NOT NULL,
            sig TEXT NOT NULL,
            received_at_ms INTEGER NOT NULL,
            received_at_block_height INTEGER NOT NULL,
            type TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| format!("unable to create table: {e}"))?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS index_messages_on_received_at ON messages(received_at_ms, received_at_block_height)", 
        []
    ).map_err(|e| format!("unable to create index: {e}"))?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS index_messages_on_pubkey ON messages(pubkey)",
        [],
    )
    .map_err(|e| format!("unable to create index: {e}"))?;

    // Stores both `BlockProposal` and `BlockPushed` messages.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS blocks (
            message_id INTEGER NOT NULL,
            proposed BOOLEAN NOT NULL,
            version INTEGER NOT NULL,
            chain_length INTEGER NOT NULL,
            burn_spent INTEGER NOT NULL,
            consensus_hash TEXT NOT NULL,
            parent_block_id TEXT NOT NULL,
            tx_merkle_root TEXT NOT NULL,
            state_index_root TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            miner_signature TEXT NOT NULL,
            signer_signature TEXT NOT NULL,
            pox_treatment TEXT NOT NULL,
            block_hash TEXT NOT NULL,
            index_block_hash TEXT NOT NULL,
            proposal_burn_height INTEGER NOT NULL,
            proposal_reward_cycle INTEGER NOT NULL,
            UNIQUE(message_id),
            FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| format!("unable to create table: {e}"))?;

    // Stores `BlockResponse` messages.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS block_responses (
            message_id INTEGER NOT NULL,
            accepted BOOLEAN NOT NULL,
            signer_signature_hash TEXT NOT NULL,
            signature TEXT NOT NULL,
            rejected_reason TEXT,
            rejected_reason_code TEXT,
            rejected_validation_failed_code TEXT,
            rejected_chain_id INTEGER,
            UNIQUE(message_id),
            FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| format!("unable to create table: {e}"))?;

    Ok(conn)
}

pub fn store_signer_db_messages(
    base_dir: &PathBuf,
    events: &Vec<chainhook_sdk::types::StacksNonConsensusEventData>,
    ctx: &Context,
) -> Result<(), String> {
    use chainhook_sdk::types::{StacksNonConsensusEventPayloadData, StacksSignerMessage};

    if events.len() == 0 {
        return Ok(());
    }
    let mut conn =
        create_or_open_readwrite_db(Some(&get_default_signers_db_file_path(base_dir)), ctx)?;
    let db_tx = conn
        .transaction()
        .map_err(|e| format!("unable to open db transaction: {e}"))?;
    {
        let mut message_stmt = db_tx
            .prepare_cached(
                "INSERT INTO messages
                (pubkey, contract, sig, received_at_ms, received_at_block_height, type)
                VALUES (?,?,?,?,?,?)
                RETURNING id",
            )
            .map_err(|e| format!("unable to prepare statement: {e}"))?;
        for event in events.iter() {
            match &event.payload {
                StacksNonConsensusEventPayloadData::SignerMessage(chunk) => {
                    // Write message header.
                    let type_str = match chunk.message {
                        StacksSignerMessage::BlockProposal(_) => "block_proposal",
                        StacksSignerMessage::BlockResponse(_) => "block_response",
                        StacksSignerMessage::BlockPushed(_) => "block_pushed",
                    };
                    let message_id: u64 = message_stmt
                        .query(rusqlite::params![
                            &chunk.pubkey,
                            &chunk.contract,
                            &chunk.sig,
                            &event.received_at_ms,
                            &event.received_at_block.index,
                            &type_str,
                        ])
                        .map_err(|e| format!("unable to write message: {e}"))?
                        .next()
                        .map_err(|e| format!("unable to retrieve new message id: {e}"))?
                        .ok_or("message id is empty")?
                        .get(0)
                        .map_err(|e| format!("unable to convert message id: {e}"))?;

                    // Write payload specifics.
                    match &chunk.message {
                        StacksSignerMessage::BlockProposal(data) => {
                            let mut stmt = db_tx
                            .prepare("INSERT INTO blocks
                                (message_id, proposed, version, chain_length, burn_spent, consensus_hash, parent_block_id,
                                    tx_merkle_root, state_index_root, timestamp, miner_signature, signer_signature, pox_treatment,
                                    block_hash, index_block_hash, proposal_burn_height, proposal_reward_cycle)
                                VALUES (?,TRUE,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                            .map_err(|e| format!("unable to prepare statement: {e}"))?;
                            stmt.execute(rusqlite::params![
                                &message_id,
                                &data.block.header.version,
                                &data.block.header.chain_length,
                                &data.block.header.burn_spent,
                                &data.block.header.consensus_hash,
                                &data.block.header.parent_block_id,
                                &data.block.header.tx_merkle_root,
                                &data.block.header.state_index_root,
                                &data.block.header.timestamp,
                                &data.block.header.miner_signature,
                                &data.block.header.signer_signature.join(","),
                                &data.block.header.pox_treatment,
                                &data.block.block_hash,
                                &data.block.index_block_hash,
                                &data.burn_height,
                                &data.reward_cycle,
                            ])
                            .map_err(|e| format!("unable to write block proposal: {e}"))?;
                        }
                        StacksSignerMessage::BlockPushed(data) => {
                            let mut stmt = db_tx
                            .prepare("INSERT INTO blocks
                                (message_id, proposed, version, chain_length, burn_spent, consensus_hash, parent_block_id,
                                    tx_merkle_root, state_index_root, timestamp, miner_signature, signer_signature, pox_treatment,
                                    block_hash, index_block_hash)
                                VALUES (?,FALSE,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                            .map_err(|e| format!("unable to prepare statement: {e}"))?;
                            stmt.execute(rusqlite::params![
                                &message_id,
                                &data.block.header.version,
                                &data.block.header.chain_length,
                                &data.block.header.burn_spent,
                                &data.block.header.consensus_hash,
                                &data.block.header.parent_block_id,
                                &data.block.header.tx_merkle_root,
                                &data.block.header.state_index_root,
                                &data.block.header.timestamp,
                                &data.block.header.miner_signature,
                                &data.block.header.signer_signature.join(","),
                                &data.block.header.pox_treatment,
                                &data.block.block_hash,
                                &data.block.index_block_hash,
                            ])
                            .map_err(|e| format!("unable to write block pushed: {e}"))?;
                        }
                        StacksSignerMessage::BlockResponse(data) => {
                            match data {
                                BlockResponseData::Accepted(response) => {
                                    let mut stmt = db_tx
                                        .prepare(
                                            "INSERT INTO block_responses
                                        (message_id, accepted, signer_signature_hash, signature)
                                        VALUES (?,TRUE,?,?)",
                                        )
                                        .map_err(|e| format!("unable to prepare statement: {e}"))?;
                                    stmt.execute(rusqlite::params![
                                        &message_id,
                                        &response.signer_signature_hash,
                                        &response.signature,
                                    ])
                                    .map_err(|e| format!("unable to write block pushed: {e}"))?;
                                }
                                BlockResponseData::Rejected(response) => {
                                    let mut validation_code: Option<&str> = None;
                                    let reason_code = match &response.reason_code {
                                        BlockRejectReasonCode::ValidationFailed{ validation_failed} => {
                                            validation_code = match validation_failed {
                                                BlockValidationFailedCode::BadBlockHash => {
                                                    Some("bad_block_hash")
                                                }
                                                BlockValidationFailedCode::BadTransaction => {
                                                    Some("bad_transaction")
                                                }
                                                BlockValidationFailedCode::InvalidBlock => {
                                                    Some("invalid_block")
                                                }
                                                BlockValidationFailedCode::ChainstateError => {
                                                    Some("chainstate_error")
                                                }
                                                BlockValidationFailedCode::UnknownParent => {
                                                    Some("unknown_parent")
                                                }
                                                BlockValidationFailedCode::NonCanonicalTenure => {
                                                    Some("no_canonical_tenure")
                                                }
                                                BlockValidationFailedCode::NoSuchTenure => {
                                                    Some("no_such_tenure")
                                                }
                                            };
                                            "validation_failed"
                                        }
                                        BlockRejectReasonCode::ConnectivityIssues => {
                                            "connectivity_issues"
                                        }
                                        BlockRejectReasonCode::RejectedInPriorRound => {
                                            "rejected_in_prior_round"
                                        }
                                        BlockRejectReasonCode::NoSortitionView => {
                                            "no_sortition_view"
                                        }
                                        BlockRejectReasonCode::SortitionViewMismatch => {
                                            "sortition_view_mismatch"
                                        }
                                        BlockRejectReasonCode::TestingDirective => {
                                            "testing_directive"
                                        }
                                    };
                                    let mut stmt = db_tx
                                    .prepare("INSERT INTO block_responses
                                        (message_id, accepted, signer_signature_hash, signature, rejected_reason,
                                            rejected_reason_code, rejected_validation_failed_code, rejected_chain_id)
                                        VALUES (?,FALSE,?,?,?,?,?,?)")
                                    .map_err(|e| format!("unable to prepare statement: {e}"))?;
                                    stmt.execute(rusqlite::params![
                                        &message_id,
                                        &response.signer_signature_hash,
                                        &response.signature,
                                        &response.reason,
                                        &reason_code,
                                        &validation_code,
                                        &response.chain_id,
                                    ])
                                    .map_err(|e| format!("unable to write block pushed: {e}"))?;
                                }
                            };
                        }
                    }
                }
            }
        }
    }
    db_tx
        .commit()
        .map_err(|e| format!("unable to commit db transaction: {e}"))?;
    Ok(())
}
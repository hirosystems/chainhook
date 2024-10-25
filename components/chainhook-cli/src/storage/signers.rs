use std::path::PathBuf;

use chainhook_sdk::{
    try_info,
    types::{
        BlockAcceptedResponse, BlockIdentifier, BlockProposalData, BlockPushedData,
        BlockRejectReasonCode, BlockRejectedResponse, BlockResponseData, BlockValidationFailedCode,
        MockBlockData, MockProposalData, MockSignatureData, NakamotoBlockData,
        NakamotoBlockHeaderData, PeerInfoData, SignerMessageMetadata, StacksNonConsensusEventData,
        StacksNonConsensusEventPayloadData, StacksSignerMessage, StacksStackerDbChunk,
    },
    utils::Context,
};
use rusqlite::{Connection, Transaction};

use super::sqlite::{create_or_open_readwrite_db, open_existing_readonly_db};

fn get_default_signers_db_file_path(base_dir: &PathBuf) -> PathBuf {
    let mut destination_path = base_dir.clone();
    destination_path.push("stacks_signers.sqlite");
    destination_path
}

pub fn open_readonly_signers_db_conn(
    base_dir: &PathBuf,
    ctx: &Context,
) -> Result<Connection, String> {
    let path = get_default_signers_db_file_path(&base_dir);
    let conn = open_existing_readonly_db(&path, ctx)?;
    Ok(conn)
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
            received_at_index_block_hash INTEGER NOT NULL,
            type TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| format!("unable to create table: {e}"))?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS index_messages_on_received_at ON messages(received_at_block_height)", 
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
            proposal_burn_height INTEGER,
            proposal_reward_cycle INTEGER,
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
            server_version TEXT NOT NULL,
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

    // `MockProposal` or `PeerInfo` data
    conn.execute(
        "CREATE TABLE IF NOT EXISTS mock_proposals (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            message_id INTEGER,
            burn_block_height INTEGER NOT NULL,
            stacks_tip_consensus_hash TEXT NOT NULL,
            stacks_tip TEXT NOT NULL,
            stacks_tip_height INTEGER NOT NULL,
            pox_consensus TEXT NOT NULL,
            server_version TEXT NOT NULL,
            network_id INTEGER NOT NULL,
            index_block_hash TEXT NOT NULL,
            UNIQUE(message_id),
            FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| format!("unable to create table: {e}"))?;

    // `MockBlock` data
    conn.execute(
        "CREATE TABLE IF NOT EXISTS mock_blocks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            message_id INTEGER NOT NULL,
            mock_proposal_id INTEGER NOT NULL,
            UNIQUE(message_id),
            FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE,
            FOREIGN KEY (mock_proposal_id) REFERENCES mock_proposals(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| format!("unable to create table: {e}"))?;

    // `MockSignature` data
    conn.execute(
        "CREATE TABLE IF NOT EXISTS mock_signatures (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            mock_proposal_id INTEGER NOT NULL,
            message_id INTEGER,
            mock_block_id INTEGER,
            server_version TEXT NOT NULL,
            signature TEXT NOT NULL,
            pubkey TEXT NOT NULL,
            UNIQUE(message_id),
            FOREIGN KEY (mock_proposal_id) REFERENCES mock_proposals(id) ON DELETE CASCADE,
            FOREIGN KEY (mock_block_id) REFERENCES mock_blocks(id) ON DELETE CASCADE,
            FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| format!("unable to create table: {e}"))?;

    Ok(conn)
}

fn store_mock_proposal_peer_info(
    db_tx: &Transaction<'_>,
    peer_info: &PeerInfoData,
    message_id: Option<u64>,
) -> Result<u64, String> {
    let mut proposal_stmt = db_tx
        .prepare(
            "INSERT INTO mock_proposals
            (message_id, burn_block_height, stacks_tip_consensus_hash, stacks_tip, stacks_tip_height, pox_consensus,
                server_version, network_id, index_block_hash)
            VALUES (?,?,?,?,?,?,?,?,?)
            RETURNING id",
        )
        .map_err(|e| format!("unable to prepare statement: {e}"))?;
    let mock_proposal_id: u64 = proposal_stmt
        .query(rusqlite::params![
            &message_id,
            &peer_info.burn_block_height,
            &peer_info.stacks_tip_consensus_hash,
            &peer_info.stacks_tip,
            &peer_info.stacks_tip_height,
            &peer_info.pox_consensus,
            &peer_info.server_version,
            &peer_info.network_id,
            &peer_info.index_block_hash,
        ])
        .map_err(|e| format!("unable to write mock proposal: {e}"))?
        .next()
        .map_err(|e| format!("unable to retrieve mock proposal id: {e}"))?
        .ok_or("mock proposal id is empty")?
        .get(0)
        .map_err(|e| format!("unable to convert message id: {e}"))?;
    Ok(mock_proposal_id)
}

fn store_mock_signature(
    db_tx: &Transaction<'_>,
    mock_signature: &MockSignatureData,
    message_id: Option<u64>,
    mock_block_id: Option<u64>,
) -> Result<(), String> {
    let mock_proposal_id =
        store_mock_proposal_peer_info(&db_tx, &mock_signature.mock_proposal.peer_info, None)?;
    let mut signature_stmt = db_tx
        .prepare(
            "INSERT INTO mock_signatures
            (message_id, mock_proposal_id, mock_block_id, server_version, signature, pubkey)
            VALUES (?,?,?,?,?,?)",
        )
        .map_err(|e| format!("unable to prepare statement: {e}"))?;
    signature_stmt
        .execute(rusqlite::params![
            &message_id,
            &mock_proposal_id,
            &mock_block_id,
            &mock_signature.metadata.server_version,
            &mock_signature.signature,
            &mock_signature.pubkey,
        ])
        .map_err(|e| format!("unable to write mock signature: {e}"))?;
    Ok(())
}

pub fn store_signer_db_messages(
    base_dir: &PathBuf,
    events: &Vec<StacksNonConsensusEventData>,
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
                (pubkey, contract, sig, received_at_ms, received_at_block_height, received_at_index_block_hash, type)
                VALUES (?,?,?,?,?,?,?)
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
                        StacksSignerMessage::MockBlock(_) => "mock_block",
                        StacksSignerMessage::MockSignature(_) => "mock_signature",
                        StacksSignerMessage::MockProposal(_) => "mock_proposal",
                    };
                    let message_id: u64 = message_stmt
                        .query(rusqlite::params![
                            &chunk.pubkey,
                            &chunk.contract,
                            &chunk.sig,
                            &event.received_at_ms,
                            &event.received_at_block.index,
                            &event.received_at_block.hash,
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
                            try_info!(
                                ctx,
                                "Storing stacks BlockProposal by signer {}",
                                chunk.pubkey
                            );
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
                            try_info!(ctx, "Storing stacks BlockPushed by signer {}", chunk.pubkey);
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
                                    try_info!(
                                        ctx,
                                        "Storing stacks BlockResponse (Accepted) by signer {}",
                                        chunk.pubkey
                                    );
                                    let mut stmt = db_tx
                                        .prepare(
                                            "INSERT INTO block_responses
                                        (message_id, accepted, signer_signature_hash, signature, server_version)
                                        VALUES (?,TRUE,?,?,?)",
                                        )
                                        .map_err(|e| format!("unable to prepare statement: {e}"))?;
                                    stmt.execute(rusqlite::params![
                                        &message_id,
                                        &response.signer_signature_hash,
                                        &response.signature,
                                        &response.metadata.server_version,
                                    ])
                                    .map_err(|e| format!("unable to write block pushed: {e}"))?;
                                }
                                BlockResponseData::Rejected(response) => {
                                    try_info!(
                                        ctx,
                                        "Storing stacks BlockResponse (Rejected) by signer {}",
                                        chunk.pubkey
                                    );
                                    let mut validation_code: Option<&str> = None;
                                    let reason_code = match &response.reason_code {
                                        BlockRejectReasonCode::ValidationFailed {
                                            validation_failed,
                                        } => {
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
                                        (message_id, accepted, signer_signature_hash, signature, server_version, rejected_reason,
                                            rejected_reason_code, rejected_validation_failed_code, rejected_chain_id)
                                        VALUES (?,FALSE,?,?,?,?,?,?,?)")
                                    .map_err(|e| format!("unable to prepare statement: {e}"))?;
                                    stmt.execute(rusqlite::params![
                                        &message_id,
                                        &response.signer_signature_hash,
                                        &response.signature,
                                        &response.metadata.server_version,
                                        &response.reason,
                                        &reason_code,
                                        &validation_code,
                                        &response.chain_id,
                                    ])
                                    .map_err(|e| format!("unable to write block pushed: {e}"))?;
                                }
                            };
                        }
                        StacksSignerMessage::MockSignature(data) => {
                            try_info!(
                                ctx,
                                "Storing stacks MockSignature by signer {}",
                                chunk.pubkey
                            );
                            store_mock_signature(&db_tx, &data, Some(message_id), None)?;
                        }
                        StacksSignerMessage::MockProposal(data) => {
                            try_info!(
                                ctx,
                                "Storing stacks MockProposal by signer {}",
                                chunk.pubkey
                            );
                            let _ = store_mock_proposal_peer_info(&db_tx, data, Some(message_id));
                        }
                        StacksSignerMessage::MockBlock(data) => {
                            try_info!(ctx, "Storing stacks MockBlock by signer {}", chunk.pubkey);
                            let mock_proposal_id = store_mock_proposal_peer_info(
                                &db_tx,
                                &data.mock_proposal.peer_info,
                                None,
                            )?;
                            let mut block_stmt = db_tx
                                .prepare(
                                    "INSERT INTO mock_blocks
                                    (message_id, mock_proposal_id)
                                    VALUES (?,?)
                                    RETURNING id",
                                )
                                .map_err(|e| format!("unable to prepare statement: {e}"))?;
                            let mock_block_id: u64 = block_stmt
                                .query(rusqlite::params![&message_id, &mock_proposal_id,])
                                .map_err(|e| format!("unable to write mock block: {e}"))?
                                .next()
                                .map_err(|e| format!("unable to retrieve mock block id: {e}"))?
                                .ok_or("mock block id is empty")?
                                .get(0)
                                .map_err(|e| format!("unable to convert message id: {e}"))?;
                            for signature in data.mock_signatures.iter() {
                                store_mock_signature(
                                    &db_tx,
                                    &signature,
                                    None,
                                    Some(mock_block_id),
                                )?;
                            }
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

fn event_data_from_message_row(
    pubkey: String,
    contract: String,
    sig: String,
    received_at_ms: u64,
    received_at_block_height: u64,
    received_at_index_block_hash: String,
    message: StacksSignerMessage,
) -> StacksNonConsensusEventData {
    StacksNonConsensusEventData {
        payload: StacksNonConsensusEventPayloadData::SignerMessage(StacksStackerDbChunk {
            contract,
            sig,
            pubkey,
            message,
        }),
        received_at_ms,
        received_at_block: BlockIdentifier {
            index: received_at_block_height,
            hash: received_at_index_block_hash,
        },
    }
}

pub fn get_signer_db_messages_received_at_block(
    db_conn: &mut Connection,
    block_identifier: &BlockIdentifier,
) -> Result<Vec<StacksNonConsensusEventData>, String> {
    let mut events = vec![];
    let db_tx = db_conn
        .transaction()
        .map_err(|e| format!("unable to open db transaction: {e}"))?;
    {
        let mut messages_stmt = db_tx
            .prepare(
                "SELECT id, pubkey, contract, sig, received_at_ms, received_at_block_height, received_at_index_block_hash,
                    type
                FROM messages
                WHERE received_at_block_height = ?
                ORDER BY id ASC",
            )
            .map_err(|e| format!("unable to prepare query: {e}"))?;
        let mut messages_iter = messages_stmt
            .query(rusqlite::params![&block_identifier.index])
            .map_err(|e| format!("unable to query messages: {e}"))?;
        while let Some(row) = messages_iter
            .next()
            .map_err(|e| format!("row error: {e}"))?
        {
            let message_id: u64 = row.get(0).unwrap();
            let pubkey: String = row.get(1).unwrap();
            let contract: String = row.get(2).unwrap();
            let sig: String = row.get(3).unwrap();
            let received_at_ms: u64 = row.get(4).unwrap();
            let received_at_block_height: u64 = row.get(5).unwrap();
            let received_at_index_block_hash: String = row.get(6).unwrap();
            let type_str: String = row.get(7).unwrap();
            let message = match type_str.as_str() {
                "block_proposal"
                | "block_pushed" => db_tx
                    .query_row(
                        "SELECT version, chain_length, burn_spent, consensus_hash, parent_block_id, tx_merkle_root,
                            state_index_root, timestamp, miner_signature, signer_signature, pox_treatment, block_hash,
                            index_block_hash, proposal_burn_height, proposal_reward_cycle
                        FROM blocks
                        WHERE message_id = ?",
                        rusqlite::params![&message_id],
                        |block_row| {
                            let signer_signature_str: String = block_row.get(9).unwrap();
                            let header = NakamotoBlockHeaderData {
                                version: block_row.get(0).unwrap(),
                                chain_length: block_row.get(1).unwrap(),
                                burn_spent: block_row.get(2).unwrap(),
                                consensus_hash: block_row.get(3).unwrap(),
                                parent_block_id: block_row.get(4).unwrap(),
                                tx_merkle_root: block_row.get(5).unwrap(),
                                state_index_root: block_row.get(6).unwrap(),
                                timestamp: block_row.get(7).unwrap(),
                                miner_signature: block_row.get(8).unwrap(),
                                signer_signature: signer_signature_str.split(",").map(String::from).collect(),
                                pox_treatment: block_row.get(10).unwrap(),
                            };
                            let block = NakamotoBlockData {
                                header,
                                block_hash: block_row.get(11).unwrap(),
                                index_block_hash: block_row.get(12).unwrap(),
                                transactions: vec![],
                            };
                            if type_str == "block_proposal" {
                                Ok(StacksSignerMessage::BlockProposal(BlockProposalData {
                                    block,
                                    burn_height: block_row.get(13).unwrap(),
                                    reward_cycle: block_row.get(14).unwrap(),
                                }))
                            } else {
                                Ok(StacksSignerMessage::BlockPushed(BlockPushedData { block }))
                            }
                        },
                    )
                    .map_err(|e| format!("unable to query block proposal: {e}"))?,
                "block_response" => db_tx
                    .query_row(
                        "SELECT accepted, signer_signature_hash, signature, server_version, rejected_reason,
                            rejected_reason_code, rejected_validation_failed_code, rejected_chain_id
                        FROM block_responses
                        WHERE message_id = ?",
                        rusqlite::params![&message_id],
                        |response_row| {
                            let accepted: bool = response_row.get(0).unwrap();
                            let signer_signature_hash: String = response_row.get(1).unwrap();
                            let signature: String = response_row.get(2).unwrap();
                            let metadata = SignerMessageMetadata {
                                server_version: response_row.get(3).unwrap()
                            };
                            if accepted {
                                Ok(StacksSignerMessage::BlockResponse(BlockResponseData::Accepted(BlockAcceptedResponse {
                                    signer_signature_hash,
                                    signature,
                                    metadata,
                                })))
                            } else {
                                let rejected_reason_code: String = response_row.get(5).unwrap();
                                Ok(StacksSignerMessage::BlockResponse(BlockResponseData::Rejected(BlockRejectedResponse {
                                    signer_signature_hash,
                                    signature,
                                    metadata,
                                    reason: response_row.get(4).unwrap(),
                                    reason_code: match rejected_reason_code.as_str() {
                                        "validation_failed" => {
                                            let validation_code: String = response_row.get(6).unwrap();
                                            BlockRejectReasonCode::ValidationFailed {
                                                validation_failed: match validation_code.as_str() {
                                                    "bad_block_hash" => BlockValidationFailedCode::BadBlockHash,
                                                    "bad_transaction" => BlockValidationFailedCode::BadTransaction,
                                                    "invalid_block" => BlockValidationFailedCode::InvalidBlock,
                                                    "chainstate_error" => BlockValidationFailedCode::ChainstateError,
                                                    "unknown_parent" => BlockValidationFailedCode::UnknownParent,
                                                    "no_canonical_tenure" => BlockValidationFailedCode::NonCanonicalTenure,
                                                    "no_such_tenure" => BlockValidationFailedCode::NoSuchTenure,
                                                    _ => unreachable!(),
                                                }
                                            }
                                        },
                                        "connectivity_issues" => BlockRejectReasonCode::ConnectivityIssues,
                                        "rejected_in_prior_round" => BlockRejectReasonCode::RejectedInPriorRound,
                                        "no_sortition_view" => BlockRejectReasonCode::NoSortitionView,
                                        "sortition_view_mismatch" => BlockRejectReasonCode::SortitionViewMismatch,
                                        "testing_directive" => BlockRejectReasonCode::TestingDirective,
                                        _ => unreachable!(),
                                    },
                                    chain_id: response_row.get(7).unwrap(),
                                })))
                            }
                        },
                    )
                    .map_err(|e| format!("unable to query block response: {e}"))?,
                "mock_signature" => db_tx
                    .query_row(
                        "SELECT p.burn_block_height, p.stacks_tip_consensus_hash, p.stacks_tip, p.stacks_tip_height,
                            p.pox_consensus, p.server_version AS peer_version, p.network_id, s.server_version, s.signature,
                            s.pubkey
                        FROM mock_signatures AS s
                        INNER JOIN mock_proposals AS p ON p.id = s.mock_proposal_id
                        WHERE s.message_id = ?",
                        rusqlite::params![&message_id],
                        |signature_row| {
                            Ok(StacksSignerMessage::MockSignature(MockSignatureData {
                                mock_proposal: MockProposalData {
                                    peer_info: PeerInfoData {
                                        burn_block_height: signature_row.get(0).unwrap(),
                                        stacks_tip_consensus_hash: signature_row.get(1).unwrap(),
                                        stacks_tip: signature_row.get(2).unwrap(),
                                        stacks_tip_height: signature_row.get(3).unwrap(),
                                        pox_consensus: signature_row.get(4).unwrap(),
                                        server_version: signature_row.get(5).unwrap(),
                                        network_id: signature_row.get(6).unwrap(),
                                        index_block_hash: signature_row.get(7).unwrap(),
                                    }
                                },
                                metadata: SignerMessageMetadata {
                                    server_version: signature_row.get(8).unwrap()
                                },
                                signature: signature_row.get(9).unwrap(),
                                pubkey: signature_row.get(10).unwrap()
                            }))
                        },
                    )
                    .map_err(|e| format!("unable to query mock signature: {e}"))?,
                "mock_proposal" => db_tx
                    .query_row(
                        "SELECT burn_block_height, stacks_tip_consensus_hash, stacks_tip, stacks_tip_height,
                            pox_consensus, server_version, network_id, index_block_hash
                        FROM mock_proposals
                        WHERE message_id = ?",
                        rusqlite::params![&message_id],
                        |proposal_row| {
                            Ok(StacksSignerMessage::MockProposal(PeerInfoData {
                                burn_block_height: proposal_row.get(0).unwrap(),
                                stacks_tip_consensus_hash: proposal_row.get(1).unwrap(),
                                stacks_tip: proposal_row.get(2).unwrap(),
                                stacks_tip_height: proposal_row.get(3).unwrap(),
                                pox_consensus: proposal_row.get(4).unwrap(),
                                server_version: proposal_row.get(5).unwrap(),
                                network_id: proposal_row.get(6).unwrap(),
                                index_block_hash: proposal_row.get(7).unwrap(),
                            }))
                        },
                    )
                    .map_err(|e| format!("unable to query mock proposal: {e}"))?,
                "mock_block" => db_tx
                    .query_row(
                        "SELECT b.id, p.burn_block_height, p.stacks_tip_consensus_hash, p.stacks_tip, p.stacks_tip_height,
                            p.pox_consensus, p.server_version, p.network_id, p.index_block_hash
                        FROM mock_blocks AS b
                        INNER JOIN mock_proposals AS p ON p.id = b.mock_proposal_id
                        WHERE b.message_id = ?",
                        rusqlite::params![&message_id],
                        |block_row| {
                            let mock_block_id: u64 = block_row.get(0).unwrap();
                            let mut sig_stmt = db_tx
                                .prepare(
                                    "SELECT p.burn_block_height, p.stacks_tip_consensus_hash, p.stacks_tip,
                                        p.stacks_tip_height, p.pox_consensus, p.server_version AS peer_version,
                                        p.network_id, p.index_block_hash, s.server_version, s.signature, s.pubkey
                                    FROM mock_signatures AS s
                                    INNER JOIN mock_proposals AS p ON p.id = s.mock_proposal_id
                                    WHERE s.mock_block_id = ?")?;
                            let mut signatures_iter = sig_stmt.query(rusqlite::params![&mock_block_id])?;
                            let mut mock_signatures = vec![];
                            while let Some(signature_row) = signatures_iter.next()? {
                                mock_signatures.push(MockSignatureData {
                                    mock_proposal: MockProposalData {
                                        peer_info: PeerInfoData {
                                            burn_block_height: signature_row.get(0).unwrap(),
                                            stacks_tip_consensus_hash: signature_row.get(1).unwrap(),
                                            stacks_tip: signature_row.get(2).unwrap(),
                                            stacks_tip_height: signature_row.get(3).unwrap(),
                                            pox_consensus: signature_row.get(4).unwrap(),
                                            server_version: signature_row.get(5).unwrap(),
                                            network_id: signature_row.get(6).unwrap(),
                                            index_block_hash: signature_row.get(7).unwrap(),
                                        }
                                    },
                                    metadata: SignerMessageMetadata {
                                        server_version: signature_row.get(8).unwrap()
                                    },
                                    signature: signature_row.get(9).unwrap(),
                                    pubkey: signature_row.get(10).unwrap()
                                });
                            }
                            Ok(StacksSignerMessage::MockBlock(MockBlockData {
                                mock_proposal: MockProposalData {
                                    peer_info: PeerInfoData {
                                        burn_block_height: block_row.get(1).unwrap(),
                                        stacks_tip_consensus_hash: block_row.get(2).unwrap(),
                                        stacks_tip: block_row.get(3).unwrap(),
                                        stacks_tip_height: block_row.get(4).unwrap(),
                                        pox_consensus: block_row.get(5).unwrap(),
                                        server_version: block_row.get(6).unwrap(),
                                        network_id: block_row.get(7).unwrap(),
                                        index_block_hash: block_row.get(8).unwrap(),
                                    }
                                },
                                mock_signatures
                            }))
                        },
                    )
                    .map_err(|e| format!("unable to query mock block: {e}"))?,
                _ => return Err(format!("invalid message type: {type_str}")),
            };
            events.push(event_data_from_message_row(
                pubkey,
                contract,
                sig,
                received_at_ms,
                received_at_block_height,
                received_at_index_block_hash,
                message,
            ));
        }
    }
    Ok(events)
}

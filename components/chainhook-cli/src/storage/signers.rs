use std::path::PathBuf;

use chainhook_sdk::{try_warn, utils::Context};
use rusqlite::Connection;

use super::sqlite::create_or_open_readwrite_db;

fn get_default_signers_db_file_path(base_dir: &PathBuf) -> PathBuf {
    let mut destination_path = base_dir.clone();
    destination_path.push("stacks_signers.sqlite");
    destination_path
}

pub fn initialize_signers_db(base_dir: Option<&PathBuf>, ctx: &Context) -> Connection {
    let db_path = base_dir.map(|dir| get_default_signers_db_file_path(dir));
    let conn = create_or_open_readwrite_db(db_path.as_ref(), ctx);
    if let Err(e) = conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            signer_pubkey TEXT NOT NULL,
            received_at INTEGER NOT NULL,
            received_at_block INTEGER NOT NULL,
            contract TEXT NOT NULL,
            sig TEXT NOT NULL,

            inscription_id TEXT NOT NULL PRIMARY KEY,
            inscription_number INTEGER NOT NULL,
            block_height INTEGER NOT NULL,
            tick TEXT NOT NULL,
            max REAL NOT NULL,
            lim REAL NOT NULL,
            dec INTEGER NOT NULL,
            address TEXT NOT NULL,
            self_mint BOOL NOT NULL,
            UNIQUE (inscription_id),
            UNIQUE (inscription_number),
            UNIQUE (tick)
        )",
        [],
    ) {
        try_warn!(ctx, "Unable to create table tokens: {}", e.to_string());
    } else {
        if let Err(e) = conn.execute(
            "CREATE INDEX IF NOT EXISTS index_tokens_on_block_height ON tokens(block_height);",
            [],
        ) {
            try_warn!(ctx, "unable to create brc20.sqlite: {}", e.to_string());
        }
    }
    if let Err(e) = conn.execute(
        "CREATE TABLE IF NOT EXISTS ledger (
            inscription_id TEXT NOT NULL,
            inscription_number INTEGER NOT NULL,
            ordinal_number INTEGER NOT NULL,
            block_height INTEGER NOT NULL,
            tx_index INTEGER NOT NULL,
            tick TEXT NOT NULL,
            address TEXT NOT NULL,
            avail_balance REAL NOT NULL,
            trans_balance REAL NOT NULL,
            operation TEXT NOT NULL CHECK(operation IN ('deploy', 'mint', 'transfer', 'transfer_send', 'transfer_receive'))
        )",
        [],
    ) {
        try_warn!(ctx, "Unable to create table ledger: {}", e.to_string());
    } else {
        if let Err(e) = conn.execute(
            "CREATE INDEX IF NOT EXISTS index_ledger_on_tick_address ON ledger(tick, address);",
            [],
        ) {
            try_warn!(ctx, "unable to create brc20.sqlite: {}", e.to_string());
        }
        if let Err(e) = conn.execute(
            "CREATE INDEX IF NOT EXISTS index_ledger_on_ordinal_number_operation ON ledger(ordinal_number, operation);",
            [],
        ) {
            try_warn!(ctx, "unable to create brc20.sqlite: {}", e.to_string());
        }
        if let Err(e) = conn.execute(
            "CREATE INDEX IF NOT EXISTS index_ledger_on_block_height_operation ON ledger(block_height, operation);",
            [],
        ) {
            try_warn!(ctx, "unable to create brc20.sqlite: {}", e.to_string());
        }
        if let Err(e) = conn.execute(
            "CREATE INDEX IF NOT EXISTS index_ledger_on_inscription_id ON ledger(inscription_id);",
            [],
        ) {
            try_warn!(ctx, "unable to create brc20.sqlite: {}", e.to_string());
        }
        if let Err(e) = conn.execute(
            "CREATE INDEX IF NOT EXISTS index_ledger_on_inscription_number ON ledger(inscription_number);",
            [],
        ) {
            try_warn!(ctx, "unable to create brc20.sqlite: {}", e.to_string());
        }
    }

    conn
}

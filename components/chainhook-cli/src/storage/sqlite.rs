use std::path::PathBuf;

use chainhook_sdk::{try_error, utils::Context};
use rusqlite::{Connection, OpenFlags};

/// Configures the SQLite connection with common settings.
fn connection_with_defaults_pragma(conn: Connection) -> Result<Connection, String> {
    conn.busy_timeout(std::time::Duration::from_secs(300))
        .map_err(|e| format!("unable to set db timeout: {e}"))?;
    conn.pragma_update(None, "mmap_size", 512 * 1024 * 1024)
        .map_err(|e| format!("unable to set db mmap_size: {e}"))?;
    conn.pragma_update(None, "cache_size", 512 * 1024 * 1024)
        .map_err(|e| format!("unable to set db cache_size: {e}"))?;
    conn.pragma_update(None, "journal_mode", &"WAL")
        .map_err(|e| format!("unable to enable db wal: {e}"))?;
    Ok(conn)
}

pub fn open_existing_readonly_db(db_path: &PathBuf, ctx: &Context) -> Result<Connection, String> {
    let open_flags = match std::fs::metadata(db_path) {
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Err(format!("could not find {}", db_path.display()));
            } else {
                return Err(format!("could not stat {}", db_path.display()));
            }
        }
        Ok(_md) => {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        }
    };
    let conn = loop {
        match Connection::open_with_flags(db_path, open_flags) {
            Ok(conn) => break conn,
            Err(e) => {
                try_error!(ctx, "unable to open hord.rocksdb: {}", e.to_string());
            }
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
    };
    Ok(connection_with_defaults_pragma(conn)?)
}

pub fn create_or_open_readwrite_db(
    db_path: Option<&PathBuf>,
    ctx: &Context,
) -> Result<Connection, String> {
    let open_flags = if let Some(db_path) = db_path {
        match std::fs::metadata(&db_path) {
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    // Create the directory path that leads to the DB file
                    if let Some(dirp) = PathBuf::from(&db_path).parent() {
                        std::fs::create_dir_all(dirp)
                            .map_err(|e| format!("unable to create db directory path: {e}"))?;
                    }
                    OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
                } else {
                    return Err(format!(
                        "could not stat db directory {}: {e}",
                        db_path.display()
                    ));
                }
            }
            Ok(_) => OpenFlags::SQLITE_OPEN_READ_WRITE,
        }
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };

    let path = match db_path {
        Some(path) => path.to_str().unwrap(),
        None => ":memory:",
    };
    let conn = loop {
        // Connect with retry.
        match Connection::open_with_flags(&path, open_flags) {
            Ok(conn) => break conn,
            Err(e) => {
                try_error!(ctx, "unable to open sqlite db: {e}");
            }
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
    };

    Ok(connection_with_defaults_pragma(conn)?)
}

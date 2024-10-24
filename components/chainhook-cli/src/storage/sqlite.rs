use std::path::PathBuf;

use chainhook_sdk::{try_error, utils::Context};
use rusqlite::{Connection, OpenFlags};

fn connection_with_defaults_pragma(conn: Connection) -> Connection {
    conn.busy_timeout(std::time::Duration::from_secs(300))
        .expect("unable to set db timeout");
    conn.pragma_update(None, "mmap_size", 512 * 1024 * 1024)
        .expect("unable to enable mmap_size");
    conn.pragma_update(None, "cache_size", 512 * 1024 * 1024)
        .expect("unable to enable cache_size");
    conn.pragma_update(None, "journal_mode", &"WAL")
        .expect("unable to enable wal");
    conn
}

pub fn create_or_open_readwrite_db(db_path: Option<&PathBuf>, ctx: &Context) -> Connection {
    let open_flags = if let Some(db_path) = db_path {
        match std::fs::metadata(&db_path) {
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    // need to create
                    if let Some(dirp) = PathBuf::from(&db_path).parent() {
                        std::fs::create_dir_all(dirp).unwrap_or_else(|e| {
                            try_error!(ctx, "{}", e.to_string());
                        });
                    }
                    OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
                } else {
                    panic!("FATAL: could not stat {}", db_path.display());
                }
            }
            Ok(_md) => {
                // can just open
                OpenFlags::SQLITE_OPEN_READ_WRITE
            }
        }
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };

    let path = match db_path {
        Some(path) => path.to_str().unwrap(),
        None => ":memory:",
    };
    let conn = loop {
        match Connection::open_with_flags(&path, open_flags) {
            Ok(conn) => break conn,
            Err(e) => {
                try_error!(ctx, "{}", e.to_string());
            }
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
    };
    connection_with_defaults_pragma(conn)
}

// pub fn open_existing_readonly_db(db_path: &PathBuf, ctx: &Context) -> Connection {
//     let open_flags = match std::fs::metadata(db_path) {
//         Err(e) => {
//             if e.kind() == std::io::ErrorKind::NotFound {
//                 panic!("FATAL: could not find {}", db_path.display());
//             } else {
//                 panic!("FATAL: could not stat {}", db_path.display());
//             }
//         }
//         Ok(_md) => {
//             // can just open
//             OpenFlags::SQLITE_OPEN_READ_ONLY
//         }
//     };

//     let conn = loop {
//         match Connection::open_with_flags(db_path, open_flags) {
//             Ok(conn) => break conn,
//             Err(e) => {
//                 try_warn!(ctx, "unable to open hord.rocksdb: {}", e.to_string());
//             }
//         };
//         std::thread::sleep(std::time::Duration::from_secs(1));
//     };
//     connection_with_defaults_pragma(conn)
// }

// pub fn perform_query_exists(
//     query: &str,
//     args: &[&dyn ToSql],
//     db_conn: &Connection,
//     ctx: &Context,
// ) -> bool {
//     let res = perform_query(query, args, db_conn, ctx, |_| true, true);
//     !res.is_empty()
// }

// pub fn perform_query_one<F, T>(
//     query: &str,
//     args: &[&dyn ToSql],
//     db_conn: &Connection,
//     ctx: &Context,
//     mapping_func: F,
// ) -> Option<T>
// where
//     F: Fn(&rusqlite::Row<'_>) -> T,
// {
//     let mut res = perform_query(query, args, db_conn, ctx, mapping_func, true);
//     match res.is_empty() {
//         true => None,
//         false => Some(res.remove(0)),
//     }
// }

// pub fn perform_query_set<F, T>(
//     query: &str,
//     args: &[&dyn ToSql],
//     db_conn: &Connection,
//     ctx: &Context,
//     mapping_func: F,
// ) -> Vec<T>
// where
//     F: Fn(&rusqlite::Row<'_>) -> T,
// {
//     perform_query(query, args, db_conn, ctx, mapping_func, false)
// }

// fn perform_query<F, T>(
//     query: &str,
//     args: &[&dyn ToSql],
//     db_conn: &Connection,
//     ctx: &Context,
//     mapping_func: F,
//     stop_at_first: bool,
// ) -> Vec<T>
// where
//     F: Fn(&rusqlite::Row<'_>) -> T,
// {
//     let mut results = vec![];
//     loop {
//         let mut stmt = match db_conn.prepare(query) {
//             Ok(stmt) => stmt,
//             Err(e) => {
//                 try_warn!(ctx, "unable to prepare query {query}: {}", e.to_string());
//                 std::thread::sleep(std::time::Duration::from_secs(5));
//                 continue;
//             }
//         };

//         match stmt.query(args) {
//             Ok(mut rows) => loop {
//                 match rows.next() {
//                     Ok(Some(row)) => {
//                         let r = mapping_func(row);
//                         results.push(r);
//                         if stop_at_first {
//                             break;
//                         }
//                     }
//                     Ok(None) => break,
//                     Err(e) => {
//                         try_warn!(
//                             ctx,
//                             "unable to iterate over results from {query}: {}",
//                             e.to_string()
//                         );
//                         std::thread::sleep(std::time::Duration::from_secs(5));
//                         continue;
//                     }
//                 }
//             },
//             Err(e) => {
//                 try_warn!(ctx, "unable to execute query {query}: {}", e.to_string());
//                 std::thread::sleep(std::time::Duration::from_secs(5));
//                 continue;
//             }
//         };
//         break;
//     }
//     results
// }

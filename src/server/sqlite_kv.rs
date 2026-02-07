use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bc_components::ARID;
use bc_envelope::Envelope;
use bc_ur::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use tokio::time::sleep;

use super::Error as ServerError;
use crate::{Error, KvStore, Result};

/// SQLite-backed key-value store for Gordian Envelopes.
///
/// Provides persistent storage with TTL support and automatic cleanup of
/// expired entries.
#[derive(Clone)]
pub struct SqliteKv {
    db_path: PathBuf,
    connection: Arc<Mutex<Connection>>,
}

impl SqliteKv {
    /// Create a new SQLite-backed key-value store.
    ///
    /// # Parameters
    ///
    /// - `path`: Path to the SQLite database file. Will be created if it
    ///   doesn't exist.
    ///
    /// # Returns
    ///
    /// A new `SqliteKv` instance with the database initialized.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db_path = path.as_ref().to_path_buf();

        // Create parent directory if it doesn't exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(ServerError::from)?;
        }

        let connection = Connection::open(&db_path)
            .map_err(ServerError::from)?;

        // Create table if it doesn't exist
        let schema = "
            CREATE TABLE IF NOT EXISTS hubert_store (
                arid TEXT PRIMARY KEY,
                envelope TEXT NOT NULL,
                expires_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_expires_at ON hubert_store(expires_at);
        ";
        connection
            .execute_batch(schema)
            .map_err(ServerError::from)?;

        let kv = Self {
            db_path,
            connection: Arc::new(Mutex::new(connection)),
        };

        // Start background cleanup task
        kv.start_cleanup_task();

        Ok(kv)
    }

    /// Start a background task that prunes expired entries every minute.
    fn start_cleanup_task(&self) {
        let connection = Arc::clone(&self.connection);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(60)).await;

                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;

                if let Ok(conn) = connection.lock() {
                    // First collect the ARIDs that will be deleted
                    let select_query = "SELECT arid FROM hubert_store WHERE expires_at IS NOT NULL AND expires_at <= ?1";
                    let arids: Vec<String> = conn
                        .prepare(select_query)
                        .and_then(|mut stmt| {
                            let rows = stmt.query_map(
                                params![now],
                                |row| row.get(0),
                            )?;
                            Ok(rows.filter_map(|r| r.ok()).collect())
                        })
                        .unwrap_or_default();

                    if !arids.is_empty() {
                        // Now delete them
                        let delete_query = "DELETE FROM hubert_store WHERE expires_at IS NOT NULL AND expires_at <= ?1";
                        if conn
                            .execute(delete_query, params![now])
                            .is_ok()
                        {
                            use crate::logging::verbose_println;
                            let count = arids.len();
                            let arid_list = arids.join(" ");
                            verbose_println(&format!(
                                "Pruned {} expired {}: {}",
                                count,
                                if count == 1 {
                                    "entry"
                                } else {
                                    "entries"
                                },
                                arid_list
                            ));
                        }
                    }
                }
            }
        });
    }

    /// Check if an ARID exists and is not expired.
    fn check_exists(&self, arid: &ARID) -> Result<bool> {
        let arid_str = arid.ur_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(ServerError::from)?
            .as_secs() as i64;

        let conn = self.connection.lock().unwrap();
        let query =
            "SELECT expires_at FROM hubert_store WHERE arid = ?1";
        let row: Option<Option<i64>> = conn
            .query_row(query, params![arid_str], |row| row.get(0))
            .optional()
            .map_err(ServerError::from)?;

        match row {
            Some(expires_at) => {
                // Check if expired
                if let Some(expiry) = expires_at {
                    if now >= expiry {
                        // Entry is expired, remove it
                        let delete_query =
                            "DELETE FROM hubert_store \
                             WHERE arid = ?1";
                        conn.execute(delete_query, params![arid_str])
                            .map_err(ServerError::from)?;
                        Ok(false)
                    } else {
                        Ok(true)
                    }
                } else {
                    Ok(true)
                }
            }
            None => Ok(false),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl KvStore for SqliteKv {
    async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        ttl_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<String> {
        use crate::logging::verbose_println;

        // Check if already exists
        if self.check_exists(arid)? {
            if verbose {
                verbose_println(&format!(
                    "PUT {} ALREADY_EXISTS",
                    arid.ur_string()
                ));
            }
            return Err(Error::AlreadyExists {
                arid: arid.ur_string(),
            });
        }

        let arid_str = arid.ur_string();
        let envelope_str = envelope.ur_string();

        let expires_at = ttl_seconds.map(|ttl| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_add(ttl) as i64
        });

        let conn = self.connection.lock().unwrap();
        let query = "INSERT INTO hubert_store \
                     (arid, envelope, expires_at) \
                     VALUES (?1, ?2, ?3)";
        conn.execute(
            query,
            params![arid_str, envelope_str, expires_at],
        )
        .map_err(ServerError::from)?;

        if verbose {
            let ttl_msg = ttl_seconds
                .map(|ttl| format!(" (TTL {}s)", ttl))
                .unwrap_or_default();
            verbose_println(&format!(
                "PUT {}{} OK (SQLite: {})",
                arid.ur_string(),
                ttl_msg,
                self.db_path.display()
            ));
        }

        Ok(format!("Stored in SQLite: {}", self.db_path.display()))
    }

    async fn get(
        &self,
        arid: &ARID,
        timeout_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<Option<Envelope>> {
        use crate::logging::verbose_println;

        let timeout = timeout_seconds.unwrap_or(30);
        let start = std::time::Instant::now();
        let mut first_attempt = true;

        loop {
            let arid_str = arid.ur_string();
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(ServerError::from)?
                .as_secs() as i64;

            let result = {
                let conn = self.connection.lock().unwrap();
                let query = "SELECT envelope, expires_at \
                             FROM hubert_store WHERE arid = ?1";
                let row: Option<(String, Option<i64>)> = conn
                    .query_row(query, params![arid_str], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })
                    .optional()
                    .map_err(ServerError::from)?;

                match row {
                    Some((envelope_str, expires_at)) => {
                        // Check if expired
                        if let Some(expiry) = expires_at {
                            if now >= expiry {
                                Some((None, true)) // expired
                            } else {
                                Some((
                                    Some(envelope_str),
                                    false,
                                )) // valid
                            }
                        } else {
                            Some((Some(envelope_str), false)) // no expiry
                        }
                    }
                    None => None, // not found
                }
            };

            match result {
                Some((Some(envelope_str), false)) => {
                    // Entry found and not expired
                    let envelope =
                        Envelope::from_ur_string(&envelope_str)?;

                    if verbose {
                        verbose_println(&format!(
                            "GET {} OK (SQLite: {})",
                            arid.ur_string(),
                            self.db_path.display()
                        ));
                    }

                    return Ok(Some(envelope));
                }
                Some((None, true)) => {
                    // Entry is expired, remove it
                    let conn = self.connection.lock().unwrap();
                    let delete_query =
                        "DELETE FROM hubert_store \
                         WHERE arid = ?1";
                    conn.execute(
                        delete_query,
                        params![arid_str],
                    )
                    .map_err(ServerError::from)?;

                    if verbose {
                        verbose_println(&format!(
                            "GET {} EXPIRED",
                            arid.ur_string()
                        ));
                    }
                    return Ok(None);
                }
                None => {
                    // Not found yet
                    if start.elapsed().as_secs() >= timeout {
                        if verbose {
                            verbose_println(&format!(
                                "GET {} NOT_FOUND \
                                 (timeout after {}s)",
                                arid.ur_string(),
                                timeout
                            ));
                        }
                        return Ok(None);
                    }

                    if first_attempt && verbose {
                        verbose_println(&format!(
                            "Polling for {} (timeout: {}s)",
                            arid.ur_string(),
                            timeout
                        ));
                        first_attempt = false;
                    } else if verbose {
                        print!(".");
                        use std::io::Write;
                        std::io::stdout().flush().ok();
                    }

                    sleep(Duration::from_millis(500)).await;
                }
                _ => unreachable!(), // Invalid states
            }
        }
    }

    async fn exists(&self, arid: &ARID) -> Result<bool> {
        self.check_exists(arid)
    }
}

use bc_components::ARID;
use bc_envelope::Envelope;

use super::{MemoryKv, SqliteKv};
use crate::KvStore;

/// Server-side key-value storage backend.
///
/// This enum allows selecting between in-memory and SQLite storage
/// at server setup time.
#[derive(Clone)]
pub enum ServerKv {
    Memory(MemoryKv),
    Sqlite(SqliteKv),
}

impl ServerKv {
    /// Create a new in-memory server KV store.
    pub fn memory() -> Self { Self::Memory(MemoryKv::new()) }

    /// Create a new SQLite-backed server KV store.
    pub fn sqlite(store: SqliteKv) -> Self { Self::Sqlite(store) }

    /// Synchronously put an envelope into the store.
    ///
    /// This method wraps the async KvStore trait implementation.
    pub(super) fn put_sync(
        &self,
        arid: ARID,
        envelope: Envelope,
        ttl_seconds: u64,
    ) -> Result<(), String> {
        match self {
            ServerKv::Memory(store) => tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    store
                        .put(&arid, &envelope, Some(ttl_seconds), false)
                        .await
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                })
            }),
            ServerKv::Sqlite(store) => tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    store
                        .put(&arid, &envelope, Some(ttl_seconds), false)
                        .await
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                })
            }),
        }
    }

    /// Synchronously get an envelope from the store.
    ///
    /// This method wraps the async KvStore trait implementation.
    pub(super) fn get_sync(&self, arid: &ARID) -> Option<Envelope> {
        match self {
            ServerKv::Memory(store) => tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    store.get(arid, Some(0), false).await.ok().flatten()
                })
            }),
            ServerKv::Sqlite(store) => tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    store.get(arid, Some(0), false).await.ok().flatten()
                })
            }),
        }
    }
}

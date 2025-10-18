mod error;
mod kv;
#[allow(clippy::module_inception)]
mod server;
mod server_kv;

pub use error::Error;
pub use kv::ServerKvClient;
pub use server::{Server, ServerConfig};

mod memory_kv;
pub use memory_kv::MemoryKv;
mod sqlite_kv;
pub use server_kv::ServerKv;
pub use sqlite_kv::SqliteKv;

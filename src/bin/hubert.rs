//! Hubert: Secure Distributed Substrate for Multiparty Transactions
//!
//! A command-line tool for storing and retrieving Gordian Envelopes using
//! distributed storage backends (BitTorrent Mainline DHT or IPFS).

use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use bc_components::ARID;
use bc_envelope::Envelope;
use bc_ur::prelude::*;
use clap::{Parser, Subcommand, ValueEnum};
use hubert::{KvStore, SqliteKv, ipfs::IpfsKv, mainline::MainlineDhtKv};

/// Hubert: Secure distributed key-value store for Gordian Envelopes
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Storage backend to use
    #[arg(long, short, global = true, default_value = "mainline")]
    storage: StorageBackend,

    /// Server/IPFS host (for --storage server or --storage ipfs)
    #[arg(long, global = true)]
    host: Option<String>,

    /// Port (for --storage server, --storage ipfs, or server command)
    #[arg(long, global = true)]
    port: Option<u16>,

    /// Enable verbose output with timestamps
    #[arg(long, short, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum StorageBackend {
    /// BitTorrent Mainline DHT (fast, ≤1 KB messages)
    Mainline,
    /// IPFS (large capacity, up to 10 MB messages)
    Ipfs,
    /// Hubert HTTP server (centralized coordination)
    Server,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Generate a new ARID
    Generate {
        #[command(subcommand)]
        generate_type: GenerateType,
    },

    /// Store an envelope at an ARID
    Put {
        /// ARID key (ur:arid format)
        #[arg(value_name = "ARID")]
        arid: String,

        /// Envelope value (ur:envelope format)
        #[arg(value_name = "ENVELOPE")]
        envelope: String,
    },

    /// Retrieve an envelope by ARID
    Get {
        /// ARID key (ur:arid format)
        #[arg(value_name = "ARID")]
        arid: String,

        /// Maximum time to wait in seconds (default: 30)
        #[arg(long, short, default_value = "30")]
        timeout: u64,
    },

    /// Check if storage backend is available
    Check,

    /// Start the Hubert HTTP server
    Server {
        /// SQLite database file path for persistent storage.
        /// If a directory is provided, uses 'hubert.sqlite' in that directory.
        /// If not provided, uses in-memory storage.
        #[arg(long)]
        sqlite: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum GenerateType {
    /// Generate a new ARID
    Arid,
}

fn parse_arid(s: &str) -> Result<ARID> {
    ARID::from_ur_string(s)
        .map_err(|_| anyhow!("Invalid ARID format. Expected ur:arid"))
}

fn parse_envelope(s: &str) -> Result<Envelope> {
    if let Ok(envelope) = Envelope::from_ur_string(s) {
        Ok(envelope)
    } else {
        bail!("Invalid envelope format. Expected ur:envelope")
    }
}

async fn check_mainline() -> Result<()> {
    use mainline::Testnet;

    // Try to connect to mainline DHT using testnet
    match Testnet::new_async(5).await {
        Ok(_) => {
            println!("✓ Mainline DHT is available");
            Ok(())
        }
        Err(e) => {
            bail!("✗ Mainline DHT is not available: {}", e)
        }
    }
}

async fn check_ipfs(port: u16) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/api/v0/version", port);
    match client
        .post(&url)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                println!("✓ IPFS is available at 127.0.0.1:{}", port);
                Ok(())
            } else {
                bail!("✗ IPFS daemon returned error: {}", response.status())
            }
        }
        Err(e) => {
            bail!("✗ IPFS is not available at 127.0.0.1:{}: {}", port, e)
        }
    }
}

async fn put_mainline(
    arid: &ARID,
    envelope: &Envelope,
    verbose: bool,
) -> Result<()> {
    let store = MainlineDhtKv::new().await.map_err(|e| anyhow!("{}", e))?;
    store
        .put(arid, envelope, None, verbose) // No TTL for mainline (not supported)
        .await
        .map_err(|e| anyhow!("{}", e))?;
    println!("✓ Stored envelope at ARID");
    Ok(())
}

async fn put_ipfs(
    arid: &ARID,
    envelope: &Envelope,
    port: u16,
    verbose: bool,
) -> Result<()> {
    let url = format!("http://127.0.0.1:{}", port);
    let store = IpfsKv::new(&url);
    store
        .put(arid, envelope, None, verbose) // No TTL (use IPFS default of 24h)
        .await
        .map_err(|e| anyhow!("{}", e))?;
    println!("✓ Stored envelope at ARID");
    Ok(())
}

async fn get_mainline(
    arid: &ARID,
    timeout: u64,
    verbose: bool,
) -> Result<Option<Envelope>> {
    let store = MainlineDhtKv::new().await.map_err(|e| anyhow!("{}", e))?;
    store
        .get(arid, Some(timeout), verbose)
        .await
        .map_err(|e| anyhow!("{}", e))
}

async fn get_ipfs(
    arid: &ARID,
    timeout: u64,
    port: u16,
    verbose: bool,
) -> Result<Option<Envelope>> {
    let url = format!("http://127.0.0.1:{}", port);
    let store = IpfsKv::new(&url);
    store
        .get(arid, Some(timeout), verbose)
        .await
        .map_err(|e| anyhow!("{}", e))
}

async fn put_server(
    host: &str,
    port: u16,
    arid: &ARID,
    envelope: &Envelope,
    verbose: bool,
) -> Result<()> {
    use hubert::server::ServerKv;

    let url = format!("http://{}:{}", host, port);
    let store = ServerKv::new(&url);
    store
        .put(arid, envelope, None, verbose) // No TTL (use server default)
        .await
        .map_err(|e| anyhow!("{}", e))?;
    println!("✓ Stored envelope at ARID");
    Ok(())
}

async fn get_server(
    host: &str,
    port: u16,
    arid: &ARID,
    timeout: u64,
    verbose: bool,
) -> Result<Option<Envelope>> {
    use hubert::server::ServerKv;

    let url = format!("http://{}:{}", host, port);
    let store = ServerKv::new(&url);
    store
        .get(arid, Some(timeout), verbose)
        .await
        .map_err(|e| anyhow!("{}", e))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Register CBOR tags for URs
    bc_components::register_tags();

    let cli = Cli::parse();

    // Validate port/host usage based on storage backend (skip for Server
    // command)
    if !matches!(cli.command, Commands::Server { .. }) {
        match cli.storage {
            StorageBackend::Mainline => {
                if cli.port.is_some() {
                    bail!(
                        "--port option is not supported for --storage mainline"
                    );
                }
                if cli.host.is_some() {
                    bail!(
                        "--host option is not supported for --storage mainline"
                    );
                }
            }
            StorageBackend::Ipfs => {
                if cli.host.is_some() {
                    bail!(
                        "--host option is not supported for --storage ipfs (always uses 127.0.0.1)"
                    );
                }
            }
            StorageBackend::Server => {
                // host and port are allowed
            }
        }
    }

    match cli.command {
        Commands::Generate { generate_type } => match generate_type {
            GenerateType::Arid => {
                let arid = ARID::new();
                println!("{}", arid.ur_string());
            }
        },

        Commands::Put { arid, envelope } => {
            let arid = parse_arid(&arid)?;
            let envelope = parse_envelope(&envelope)?;

            match cli.storage {
                StorageBackend::Mainline => {
                    put_mainline(&arid, &envelope, cli.verbose).await?
                }
                StorageBackend::Ipfs => {
                    let port = cli.port.unwrap_or(5001);
                    put_ipfs(&arid, &envelope, port, cli.verbose).await?
                }
                StorageBackend::Server => {
                    let host = cli.host.as_deref().unwrap_or("127.0.0.1");
                    let port = cli.port.unwrap_or(45678);
                    put_server(host, port, &arid, &envelope, cli.verbose)
                        .await?
                }
            }
        }

        Commands::Get { arid, timeout } => {
            let arid = parse_arid(&arid)?;

            let envelope = match cli.storage {
                StorageBackend::Mainline => {
                    get_mainline(&arid, timeout, cli.verbose).await?
                }
                StorageBackend::Ipfs => {
                    let port = cli.port.unwrap_or(5001);
                    get_ipfs(&arid, timeout, port, cli.verbose).await?
                }
                StorageBackend::Server => {
                    let host = cli.host.as_deref().unwrap_or("127.0.0.1");
                    let port = cli.port.unwrap_or(45678);
                    get_server(host, port, &arid, timeout, cli.verbose).await?
                }
            };

            match envelope {
                Some(env) => {
                    println!("{}", env.ur_string());
                }
                None => {
                    bail!("Envelope not found within {} seconds", timeout);
                }
            }
        }

        Commands::Check => match cli.storage {
            StorageBackend::Mainline => check_mainline().await?,
            StorageBackend::Ipfs => {
                let port = cli.port.unwrap_or(5001);
                check_ipfs(port).await?
            }
            StorageBackend::Server => {
                // Check if server is reachable
                use hubert::server::ServerKv;
                use tokio::time::{Duration, timeout};

                let host = cli.host.as_deref().unwrap_or("127.0.0.1");
                let port = cli.port.unwrap_or(45678);
                let url = format!("http://{}:{}", host, port);
                let store = ServerKv::new(&url);
                // Try to get a non-existent ARID to check connectivity
                let test_arid = ARID::new();

                // Wrap the entire check in a 2-second timeout
                match timeout(
                    Duration::from_secs(2),
                    store.get(&test_arid, Some(1), false),
                )
                .await
                {
                    Ok(Ok(_)) => {
                        println!("✓ Server is available at {}:{}", host, port);
                    }
                    Ok(Err(e)) => {
                        bail!(
                            "✗ Server is not available at {}:{}: {}",
                            host,
                            port,
                            e
                        )
                    }
                    Err(_) => {
                        bail!(
                            "✗ Server is not available at {}:{}: connection timeout",
                            host,
                            port
                        )
                    }
                }
            }
        },

        Commands::Server { sqlite } => {
            use hubert::server::{Server, ServerConfig};

            // Validate that --storage is not used with server command
            if matches!(cli.storage, StorageBackend::Server) {
                bail!(
                    "--storage server is for clients using the server, not for running the server itself. Use: hubert server"
                );
            }

            let port = cli.port.unwrap_or(45678);
            let config = ServerConfig {
                port,
                max_ttl: 86400, // 24 hours
                verbose: cli.verbose,
            };

            // Determine storage backend
            if let Some(sqlite_path) = sqlite {
                // Use SQLite storage
                let path = if PathBuf::from(&sqlite_path).is_dir() {
                    PathBuf::from(&sqlite_path).join("hubert.sqlite")
                } else {
                    PathBuf::from(&sqlite_path)
                };

                let store =
                    SqliteKv::new(&path).map_err(|e| anyhow!("{}", e))?;
                let server = Server::new_sqlite(config, store);
                println!(
                    "Starting Hubert server on port {} with SQLite storage: {}",
                    port,
                    path.display()
                );
                server.run().await.map_err(|e| anyhow!("{}", e))?;
            } else {
                // Use in-memory storage
                let server = Server::new_memory(config);
                println!(
                    "Starting Hubert server on port {} with in-memory storage",
                    port
                );
                server.run().await.map_err(|e| anyhow!("{}", e))?;
            }
        }
    }

    Ok(())
}

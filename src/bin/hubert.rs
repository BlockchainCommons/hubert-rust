//! Hubert: Secure Distributed Substrate for Multiparty Transactions
//!
//! A command-line tool for storing and retrieving Gordian Envelopes using
//! distributed storage backends (BitTorrent Mainline DHT or IPFS).

use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use bc_components::ARID;
use bc_envelope::Envelope;
use bc_rand::random_data;
use bc_ur::prelude::*;
use clap::{Parser, Subcommand, ValueEnum};
use hubert::{
    KvStore, SqliteKv, hybrid::HybridKv, ipfs::IpfsKv,
    logging::verbose_println, mainline::MainlineDhtKv,
};

/// Hubert: Distributed substrate for multiparty transactions
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
#[command(infer_subcommands = true)]
struct Cli {
    /// Enable verbose logging
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
    /// Hybrid (automatic: DHT for small, IPFS for large)
    Hybrid,
    /// Hubert HTTP server (centralized coordination)
    Server,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Generate a new ARID or example Envelope
    Generate {
        #[command(subcommand)]
        generate_type: GenerateType,
    },

    /// Store an envelope at an ARID
    Put {
        /// Storage backend to use
        #[arg(long, short, default_value = "mainline")]
        storage: StorageBackend,

        /// Server/IPFS host (for --storage server)
        #[arg(long)]
        host: Option<String>,

        /// Port (for --storage server, --storage ipfs, or --storage hybrid)
        #[arg(long)]
        port: Option<u16>,

        /// ARID key (ur:arid format)
        #[arg(value_name = "ARID")]
        arid: String,

        /// Envelope value (ur:envelope format)
        #[arg(value_name = "ENVELOPE")]
        envelope: String,

        /// Time-to-live in seconds (for --storage server or --storage
        /// ipfs/hybrid). Server: controls data retention (default: 24
        /// hours). IPFS: controls IPNS record lifetime (default: 24
        /// hours).
        #[arg(long)]
        ttl: Option<u64>,

        /// Pin content in IPFS (only for --storage ipfs or --storage hybrid)
        #[arg(long)]
        pin: bool,
    },

    /// Retrieve an envelope by ARID
    Get {
        /// Storage backend to use
        #[arg(long, short, default_value = "mainline")]
        storage: StorageBackend,

        /// Server/IPFS host (for --storage server)
        #[arg(long)]
        host: Option<String>,

        /// Port (for --storage server, --storage ipfs, or --storage hybrid)
        #[arg(long)]
        port: Option<u16>,

        /// ARID key (ur:arid format)
        #[arg(value_name = "ARID")]
        arid: String,

        /// Maximum time to wait in seconds (default: 30)
        #[arg(long, short, default_value = "30")]
        timeout: u64,
    },

    /// Check if storage backend is available
    Check {
        /// Storage backend to use
        #[arg(long, short, default_value = "mainline")]
        storage: StorageBackend,

        /// Server/IPFS host (for --storage server)
        #[arg(long)]
        host: Option<String>,

        /// Port (for --storage server, --storage ipfs, or --storage hybrid)
        #[arg(long)]
        port: Option<u16>,
    },

    /// Start the Hubert HTTP server
    Server {
        /// Port for the server to listen on (default: 45678)
        #[arg(long)]
        port: Option<u16>,

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
    /// Generate a test envelope with random data
    Envelope {
        /// Number of random bytes to include in the envelope's subject
        #[arg(value_name = "SIZE")]
        size: usize,
    },
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

fn generate_random_envelope(size: usize) -> Envelope {
    let random_bytes = random_data(size);
    let byte_string = ByteString::new(random_bytes);
    Envelope::new(byte_string)
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
    if verbose {
        verbose_println("✓ Stored envelope at ARID");
    }
    Ok(())
}

async fn put_ipfs(
    arid: &ARID,
    envelope: &Envelope,
    port: u16,
    pin: bool,
    verbose: bool,
) -> Result<()> {
    let url = format!("http://127.0.0.1:{}", port);
    let store = IpfsKv::new(&url).with_pin_content(pin);
    let result = store
        .put(arid, envelope, None, verbose) // No TTL (use IPFS default of 24h)
        .await
        .map_err(|e| anyhow!("{}", e))?;

    if verbose {
        verbose_println("✓ Stored envelope at ARID");
    }

    // Extract and print CID if pinning was requested
    if pin {
        // Result format is "ipns://{peer_id} -> ipfs://{cid}"
        if let Some(cid_part) = result.split("ipfs://").nth(1) {
            println!("CID: {}", cid_part);
        }
    }

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

async fn put_hybrid(
    arid: &ARID,
    envelope: &Envelope,
    port: u16,
    pin: bool,
    verbose: bool,
) -> Result<()> {
    let url = format!("http://127.0.0.1:{}", port);
    let store = HybridKv::new(&url)
        .await
        .map_err(|e| anyhow!("{}", e))?
        .with_pin_content(pin);
    let result = store
        .put(arid, envelope, None, verbose)
        .await
        .map_err(|e| anyhow!("{}", e))?;

    if verbose {
        verbose_println("✓ Stored envelope at ARID");
    }

    // Extract and print CID if pinning was requested and IPFS was used
    if pin
        && result.contains("ipfs://")
        && let Some(cid_part) = result.split("ipfs://").nth(1)
    {
        println!("CID: {}", cid_part);
    }

    Ok(())
}

async fn get_hybrid(
    arid: &ARID,
    timeout: u64,
    port: u16,
    verbose: bool,
) -> Result<Option<Envelope>> {
    let url = format!("http://127.0.0.1:{}", port);
    let store = HybridKv::new(&url).await.map_err(|e| anyhow!("{}", e))?;
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
    ttl: Option<u64>,
    verbose: bool,
) -> Result<()> {
    use hubert::server::ServerKvClient;

    let url = format!("http://{}:{}", host, port);
    let store = ServerKvClient::new(&url);
    store
        .put(arid, envelope, ttl, verbose)
        .await
        .map_err(|e| anyhow!("{}", e))?;
    if verbose {
        verbose_println("✓ Stored envelope at ARID");
    }
    Ok(())
}

async fn get_server(
    host: &str,
    port: u16,
    arid: &ARID,
    timeout: u64,
    verbose: bool,
) -> Result<Option<Envelope>> {
    use hubert::server::ServerKvClient;

    let url = format!("http://{}:{}", host, port);
    let store = ServerKvClient::new(&url);
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

    match cli.command {
        Commands::Generate { generate_type } => match generate_type {
            GenerateType::Arid => {
                let arid = ARID::new();
                println!("{}", arid.ur_string());
            }
            GenerateType::Envelope { size } => {
                let envelope = generate_random_envelope(size);
                println!("{}", envelope.ur_string());
            }
        },

        Commands::Put { storage, host, port, arid, envelope, ttl, pin } => {
            // Validate port/host usage based on storage backend
            match storage {
                StorageBackend::Mainline => {
                    if port.is_some() {
                        bail!(
                            "--port option is not supported for --storage mainline"
                        );
                    }
                    if host.is_some() {
                        bail!(
                            "--host option is not supported for --storage mainline"
                        );
                    }
                }
                StorageBackend::Ipfs => {
                    if host.is_some() {
                        bail!(
                            "--host option is not supported for --storage ipfs (always uses 127.0.0.1)"
                        );
                    }
                }
                StorageBackend::Hybrid => {
                    if host.is_some() {
                        bail!(
                            "--host option is not supported for --storage hybrid (always uses 127.0.0.1)"
                        );
                    }
                }
                StorageBackend::Server => {
                    // host and port are allowed
                }
            }

            let arid = parse_arid(&arid)?;
            let envelope = parse_envelope(&envelope)?;

            match storage {
                StorageBackend::Mainline => {
                    if ttl.is_some() {
                        bail!(
                            "--ttl option is only supported for --storage server"
                        );
                    }
                    if pin {
                        bail!(
                            "--pin option is only supported for --storage ipfs or --storage hybrid"
                        );
                    }
                    put_mainline(&arid, &envelope, cli.verbose).await?
                }
                StorageBackend::Ipfs => {
                    if ttl.is_some() {
                        bail!(
                            "--ttl option is only supported for --storage server"
                        );
                    }
                    let port = port.unwrap_or(5001);
                    put_ipfs(&arid, &envelope, port, pin, cli.verbose).await?
                }
                StorageBackend::Hybrid => {
                    if ttl.is_some() {
                        bail!(
                            "--ttl option is only supported for --storage server"
                        );
                    }
                    let port = port.unwrap_or(5001);
                    put_hybrid(&arid, &envelope, port, pin, cli.verbose).await?
                }
                StorageBackend::Server => {
                    if pin {
                        bail!(
                            "--pin option is only supported for --storage ipfs or --storage hybrid"
                        );
                    }
                    let host = host.as_deref().unwrap_or("127.0.0.1");
                    let port = port.unwrap_or(45678);
                    put_server(host, port, &arid, &envelope, ttl, cli.verbose)
                        .await?
                }
            }
        }

        Commands::Get { storage, host, port, arid, timeout } => {
            // Validate port/host usage based on storage backend
            match storage {
                StorageBackend::Mainline => {
                    if port.is_some() {
                        bail!(
                            "--port option is not supported for --storage mainline"
                        );
                    }
                    if host.is_some() {
                        bail!(
                            "--host option is not supported for --storage mainline"
                        );
                    }
                }
                StorageBackend::Ipfs => {
                    if host.is_some() {
                        bail!(
                            "--host option is not supported for --storage ipfs (always uses 127.0.0.1)"
                        );
                    }
                }
                StorageBackend::Hybrid => {
                    if host.is_some() {
                        bail!(
                            "--host option is not supported for --storage hybrid (always uses 127.0.0.1)"
                        );
                    }
                }
                StorageBackend::Server => {
                    // host and port are allowed
                }
            }

            let arid = parse_arid(&arid)?;

            let envelope = match storage {
                StorageBackend::Mainline => {
                    get_mainline(&arid, timeout, cli.verbose).await?
                }
                StorageBackend::Ipfs => {
                    let port = port.unwrap_or(5001);
                    get_ipfs(&arid, timeout, port, cli.verbose).await?
                }
                StorageBackend::Hybrid => {
                    let port = port.unwrap_or(5001);
                    get_hybrid(&arid, timeout, port, cli.verbose).await?
                }
                StorageBackend::Server => {
                    let host = host.as_deref().unwrap_or("127.0.0.1");
                    let port = port.unwrap_or(45678);
                    get_server(host, port, &arid, timeout, cli.verbose).await?
                }
            };

            match envelope {
                Some(env) => {
                    println!("{}", env.ur_string());
                }
                None => {
                    bail!("Value not found within {} seconds", timeout);
                }
            }
        }

        Commands::Check { storage, host, port } => {
            // Validate port/host usage based on storage backend
            match storage {
                StorageBackend::Mainline => {
                    if port.is_some() {
                        bail!(
                            "--port option is not supported for --storage mainline"
                        );
                    }
                    if host.is_some() {
                        bail!(
                            "--host option is not supported for --storage mainline"
                        );
                    }
                }
                StorageBackend::Ipfs => {
                    if host.is_some() {
                        bail!(
                            "--host option is not supported for --storage ipfs (always uses 127.0.0.1)"
                        );
                    }
                }
                StorageBackend::Hybrid => {
                    if host.is_some() {
                        bail!(
                            "--host option is not supported for --storage hybrid (always uses 127.0.0.1)"
                        );
                    }
                }
                StorageBackend::Server => {
                    // host and port are allowed
                }
            }

            match storage {
                StorageBackend::Mainline => check_mainline().await?,
                StorageBackend::Ipfs => {
                    let port = port.unwrap_or(5001);
                    check_ipfs(port).await?
                }
                StorageBackend::Hybrid => {
                    // Check both DHT and IPFS
                    check_mainline().await?;
                    let port = port.unwrap_or(5001);
                    check_ipfs(port).await?;
                    println!("✓ Hybrid storage is available (DHT + IPFS)");
                }
                StorageBackend::Server => {
                    // Check if server is reachable via health endpoint
                    use tokio::time::{Duration, timeout};

                    let host = host.as_deref().unwrap_or("127.0.0.1");
                    let port = port.unwrap_or(45678);
                    let url = format!("http://{}:{}/health", host, port);

                    let client = reqwest::Client::new();

                    // Try to connect to health endpoint with 2-second timeout
                    match timeout(
                        Duration::from_secs(2),
                        client.get(&url).send(),
                    )
                    .await
                    {
                        Ok(Ok(response)) => {
                            if response.status().is_success() {
                                // Try to parse the JSON response
                                if let Ok(text) = response.text().await {
                                    if let Ok(json) = serde_json::from_str::<
                                        serde_json::Value,
                                    >(
                                        &text
                                    ) {
                                        if json
                                            .get("server")
                                            .and_then(|v| v.as_str())
                                            == Some("hubert")
                                        {
                                            let version = json
                                                .get("version")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("unknown");
                                            println!(
                                                "✓ Hubert server is available at {}:{} (version {})",
                                                host, port, version
                                            );
                                        } else {
                                            bail!(
                                                "✗ Server at {}:{} is not a Hubert server",
                                                host,
                                                port
                                            );
                                        }
                                    } else {
                                        bail!(
                                            "✗ Server at {}:{} returned invalid health response",
                                            host,
                                            port
                                        );
                                    }
                                } else {
                                    bail!(
                                        "✗ Server at {}:{} returned invalid health response",
                                        host,
                                        port
                                    );
                                }
                            } else {
                                bail!(
                                    "✗ Server at {}:{} is not available (status: {})",
                                    host,
                                    port,
                                    response.status()
                                );
                            }
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
            }
        }

        Commands::Server { port, sqlite } => {
            use hubert::server::{Server, ServerConfig};

            let port = port.unwrap_or(45678);
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

//! Hubert: Secure Distributed Substrate for Multiparty Transactions
//!
//! A command-line tool for storing and retrieving Gordian Envelopes using
//! distributed storage backends (BitTorrent Mainline DHT or IPFS).

use anyhow::{Result, anyhow, bail};
use bc_components::ARID;
use bc_envelope::Envelope;
use bc_ur::prelude::*;
use clap::{Parser, Subcommand, ValueEnum};
use hubert::{KvStore, ipfs::IpfsKv, mainline::MainlineDhtKv};

/// Hubert: Secure distributed key-value store for Gordian Envelopes
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Storage backend to use
    #[arg(long, short, global = true, default_value = "mainline")]
    storage: StorageBackend,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum StorageBackend {
    /// BitTorrent Mainline DHT (fast, ≤1 KB messages)
    Mainline,
    /// IPFS (large capacity, up to 10 MB messages)
    Ipfs,
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
        /// Port to listen on
        #[arg(long, short, default_value = "45678")]
        port: u16,
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

async fn check_ipfs() -> Result<()> {
    let client = reqwest::Client::new();
    match client
        .post("http://127.0.0.1:5001/api/v0/version")
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                println!("✓ IPFS is available at 127.0.0.1:5001");
                Ok(())
            } else {
                bail!("✗ IPFS daemon returned error: {}", response.status())
            }
        }
        Err(e) => {
            bail!("✗ IPFS is not available at 127.0.0.1:5001: {}", e)
        }
    }
}

async fn put_mainline(arid: &ARID, envelope: &Envelope) -> Result<()> {
    let store = MainlineDhtKv::new().await.map_err(|e| anyhow!("{}", e))?;
    store
        .put(arid, envelope, None) // No TTL for mainline (not supported)
        .await
        .map_err(|e| anyhow!("{}", e))?;
    println!("✓ Stored envelope at ARID");
    Ok(())
}

async fn put_ipfs(arid: &ARID, envelope: &Envelope) -> Result<()> {
    let store = IpfsKv::new("http://127.0.0.1:5001");
    store
        .put(arid, envelope, None) // No TTL (use IPFS default of 24h)
        .await
        .map_err(|e| anyhow!("{}", e))?;
    println!("✓ Stored envelope at ARID");
    Ok(())
}

async fn get_mainline(arid: &ARID, timeout: u64) -> Result<Option<Envelope>> {
    let store = MainlineDhtKv::new().await.map_err(|e| anyhow!("{}", e))?;
    store
        .get(arid, Some(timeout))
        .await
        .map_err(|e| anyhow!("{}", e))
}

async fn get_ipfs(arid: &ARID, timeout: u64) -> Result<Option<Envelope>> {
    let store = IpfsKv::new("http://127.0.0.1:5001");
    store
        .get(arid, Some(timeout))
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
        },

        Commands::Put { arid, envelope } => {
            let arid = parse_arid(&arid)?;
            let envelope = parse_envelope(&envelope)?;

            match cli.storage {
                StorageBackend::Mainline => {
                    put_mainline(&arid, &envelope).await?
                }
                StorageBackend::Ipfs => put_ipfs(&arid, &envelope).await?,
            }
        }

        Commands::Get { arid, timeout } => {
            let arid = parse_arid(&arid)?;

            let envelope = match cli.storage {
                StorageBackend::Mainline => {
                    get_mainline(&arid, timeout).await?
                }
                StorageBackend::Ipfs => get_ipfs(&arid, timeout).await?,
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
            StorageBackend::Ipfs => check_ipfs().await?,
        },

        Commands::Server { port } => {
            use hubert::server::{Server, ServerConfig};

            let config = ServerConfig {
                port,
                ..Default::default() // Use default TTL settings
            };
            let server = Server::new(config);
            server.run().await.map_err(|e| anyhow!("{}", e))?;
        }
    }

    Ok(())
}

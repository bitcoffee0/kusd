mod config;

use clap::{Parser, Subcommand};
use config::Config;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_kusd::indexer::{DeploymentManifest, Indexer, LiveUtxo, validate_live_utxos};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::client::{ConnectOptions, ConnectStrategy};
use kaspa_wrpc_client::{KaspaRpcClient, Resolver, WrpcEncoding};
use std::time::Duration;
use tokio::time::timeout;

#[derive(Parser)]
#[command(
    name = "kusd",
    about = "KUSD covenant tools for the configured Kaspa network"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    CheckRpc,
    Address,
    Balance,
    IndexSummary {
        #[arg(long, default_value = "kusd-index.local.json")]
        file: std::path::PathBuf,
    },
    IndexRpc {
        #[arg(long, default_value = "kusd-index-manifest.local.json")]
        manifest: std::path::PathBuf,
        #[arg(long, default_value = "kusd-index.local.json")]
        output: std::path::PathBuf,
    },
    IndexValidate {
        #[arg(long)]
        manifest: std::path::PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config = Config::load()?;
    match cli.command {
        Command::CheckRpc => check_rpc(&config).await?,
        Command::Address => println!("{}", config.address()?),
        Command::Balance => balance(&config).await?,
        Command::IndexSummary { file } => {
            let indexer = Indexer::load(&file)?;
            let snapshot = indexer.snapshot();
            println!("file: {}", file.display());
            println!("tracked UTXOs: {}", snapshot.entities.len());
            println!("KUSD supply: {}", snapshot.circulating_supply);
            println!("Position debt: {}", snapshot.position_debt);
            println!("active proposals: {}", snapshot.active_governance_proposals);
            println!(
                "proposed allocation: {}",
                snapshot.proposed_module_allocation
            );
            println!("delegated KPS: {}", snapshot.delegated_kps);
            println!("weighted KPS votes: {}", snapshot.weighted_kps_votes);
            println!("Savings balance: {}", snapshot.savings_balance);
            println!("Savings accounts: {}", snapshot.savings_accounts);
            println!("challenged Positions: {}", snapshot.challenged_positions);
            println!("active Challenges: {}", snapshot.active_challenges);
            println!("active Auctions: {}", snapshot.active_auctions);
        }
        Command::IndexRpc { manifest, output } => {
            index_rpc(&config, &manifest, &output).await?;
        }
        Command::IndexValidate { manifest } => {
            let manifest = DeploymentManifest::load(&manifest)?;
            let live = manifest
                .tracked
                .iter()
                .map(|tracked| tracked.outpoint.clone())
                .collect::<std::collections::BTreeSet<_>>();
            let mut indexer = Indexer::default();
            indexer.rebuild_history_from_manifest(&manifest, &live)?;
            println!(
                "{} manifest valid: {} transactions",
                manifest.protocol.as_deref().unwrap_or("KUSD"),
                manifest.history.len()
            );
            println!("final UTXOs: {}", indexer.snapshot().entities.len());
            println!(
                "active proposals: {}",
                indexer.snapshot().active_governance_proposals
            );
        }
    }
    Ok(())
}

async fn rpc_client(config: &Config) -> Result<KaspaRpcClient, Box<dyn std::error::Error>> {
    let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);
    let (encoding, url, resolver) = if config.rpc_url == "public" {
        (WrpcEncoding::Borsh, None, Some(Resolver::default()))
    } else if config.rpc_url.ends_with("/borsh") {
        (WrpcEncoding::Borsh, Some(config.rpc_url.as_str()), None)
    } else {
        (WrpcEncoding::SerdeJson, Some(config.rpc_url.as_str()), None)
    };
    let client = KaspaRpcClient::new(encoding, url, resolver, Some(network_id), None)?;
    timeout(
        Duration::from_secs(20),
        client.connect(Some(ConnectOptions {
            block_async_connect: true,
            connect_timeout: Some(Duration::from_secs(10)),
            strategy: ConnectStrategy::Fallback,
            ..Default::default()
        })),
    )
    .await
    .map_err(|_| "public RPC timeout (20s)")??;
    Ok(client)
}

async fn index_rpc(
    config: &Config,
    manifest_path: &std::path::Path,
    output: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = DeploymentManifest::load(manifest_path)?;
    let addresses = manifest
        .tracked
        .iter()
        .map(|tracked| kaspa_addresses::Address::try_from(tracked.address.as_str()))
        .collect::<Result<Vec<_>, _>>()?;
    let client = rpc_client(config).await?;
    let entries = client.get_utxos_by_addresses(addresses).await?;
    let rpc_entries = entries
        .into_iter()
        .map(|entry| LiveUtxo {
            outpoint: kaspa_kusd::indexer::Outpoint {
                transaction_id: entry.outpoint.transaction_id.to_string(),
                index: entry.outpoint.index,
            },
            address: entry
                .address
                .map(|address| address.to_string())
                .unwrap_or_default(),
            covenant_id: entry
                .utxo_entry
                .covenant_id
                .map(|covenant_id| covenant_id.to_string()),
        })
        .collect::<Vec<_>>();
    let live = validate_live_utxos(&manifest, &rpc_entries)?;
    let mut indexer = Indexer::load(output)?;
    indexer.rebuild_history_from_manifest(&manifest, &live)?;
    indexer.save(output)?;
    println!("manifest: {}", manifest_path.display());
    println!("live UTXOs: {}", indexer.snapshot().entities.len());
    println!("KUSD supply: {}", indexer.snapshot().circulating_supply);
    println!("KPS supply: {}", indexer.snapshot().kps_supply);
    println!("Position debt: {}", indexer.snapshot().position_debt);
    println!("Reserve KUSD: {}", indexer.snapshot().reserve_kusd);
    println!(
        "Reserve KPS supply: {}",
        indexer.snapshot().reserve_total_kps
    );
    println!(
        "Reserve KAS collateral: {}",
        indexer.snapshot().reserve_collateral_sompi
    );
    println!("replayed transactions: {}", manifest.history.len());
    println!(
        "active proposals: {}",
        indexer.snapshot().active_governance_proposals
    );
    println!(
        "proposed allocation: {}",
        indexer.snapshot().proposed_module_allocation
    );
    println!("delegated KPS: {}", indexer.snapshot().delegated_kps);
    println!(
        "weighted KPS votes: {}",
        indexer.snapshot().weighted_kps_votes
    );
    println!("Savings balance: {}", indexer.snapshot().savings_balance);
    println!("Savings accounts: {}", indexer.snapshot().savings_accounts);
    println!(
        "challenged Positions: {}",
        indexer.snapshot().challenged_positions
    );
    println!(
        "active Challenges: {}",
        indexer.snapshot().active_challenges
    );
    println!("active Auctions: {}", indexer.snapshot().active_auctions);
    client.disconnect().await?;
    Ok(())
}

async fn check_rpc(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);
    let (encoding, url, resolver) = if config.rpc_url == "public" {
        (WrpcEncoding::Borsh, None, Some(Resolver::default()))
    } else if config.rpc_url.ends_with("/borsh") {
        (WrpcEncoding::Borsh, Some(config.rpc_url.as_str()), None)
    } else {
        (WrpcEncoding::SerdeJson, Some(config.rpc_url.as_str()), None)
    };
    let client = KaspaRpcClient::new(encoding, url, resolver, Some(network_id), None)?;
    timeout(
        Duration::from_secs(20),
        client.connect(Some(ConnectOptions {
            block_async_connect: true,
            connect_timeout: Some(Duration::from_secs(10)),
            strategy: ConnectStrategy::Fallback,
            ..Default::default()
        })),
    )
    .await
    .map_err(|_| "public RPC timeout (20s)")??;
    let info = client.get_server_info().await?;
    println!("RPC: {}", config.rpc_url);
    println!("configuration: {}", config.network);
    println!("version: {}", info.server_version);
    println!("network: {}", info.network_id);
    println!("synced: {}", info.is_synced);
    println!("index UTXO: {}", info.has_utxo_index);
    if info.network_id != network_id {
        return Err(format!("RPC network is {}; expected testnet-10", info.network_id).into());
    }
    if !info.is_synced || !info.has_utxo_index {
        return Err("RPC is not ready (sync/UTXO index)".into());
    }
    client.disconnect().await?;
    Ok(())
}

async fn balance(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);
    let (encoding, url, resolver) = if config.rpc_url == "public" {
        (WrpcEncoding::Borsh, None, Some(Resolver::default()))
    } else if config.rpc_url.ends_with("/borsh") {
        (WrpcEncoding::Borsh, Some(config.rpc_url.as_str()), None)
    } else {
        (WrpcEncoding::SerdeJson, Some(config.rpc_url.as_str()), None)
    };
    let client = KaspaRpcClient::new(encoding, url, resolver, Some(network_id), None)?;
    timeout(
        Duration::from_secs(20),
        client.connect(Some(ConnectOptions {
            block_async_connect: true,
            connect_timeout: Some(Duration::from_secs(10)),
            strategy: ConnectStrategy::Fallback,
            ..Default::default()
        })),
    )
    .await
    .map_err(|_| "public RPC timeout (20s)")??;
    let address = config.address()?;
    let utxos = client.get_utxos_by_addresses(vec![address.clone()]).await?;
    let total: u64 = utxos.iter().map(|entry| entry.utxo_entry.amount).sum();
    println!("address: {address}");
    println!("UTXO: {}", utxos.len());
    println!(
        "balance: {total} sompi ({:.8} KAS)",
        total as f64 / 100_000_000.0
    );
    for entry in utxos {
        println!(
            "{}:{}  {} sompi",
            entry.outpoint.transaction_id, entry.outpoint.index, entry.utxo_entry.amount
        );
    }
    client.disconnect().await?;
    Ok(())
}

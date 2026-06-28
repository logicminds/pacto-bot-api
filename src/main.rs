use clap::Parser;
use fs2::FileExt;
use pacto_bot_api::client_manager::ClientManager;
use pacto_bot_api::config::DaemonConfig;
use pacto_bot_api::db::Database;
use pacto_bot_api::diagnostics::{DaemonStatus, Diagnostics};
use pacto_bot_api::dispatch::Dispatch;
use pacto_bot_api::nostr::NostrClient;
use pacto_bot_api::signer::Signer;
use pacto_bot_api::transport::TransportLayer;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, oneshot};
use tokio_stream::StreamExt;
use tracing::{info, warn};

const DAEMON_LOCK_FILE: &str = "daemon.lock";
const AGENT_DB_FILE: &str = "agent.db";

#[derive(Parser, Debug)]
#[command(name = "pacto-bot-api")]
#[command(about = "Pacto bot API daemon")]
struct Cli {
    /// Path to the bot configuration file.
    #[arg(short, long, value_name = "PATH", default_value = "pacto-bot-api.toml")]
    config: PathBuf,

    /// Directory for runtime data (database, socket, reports).
    #[arg(short, long, value_name = "DIR")]
    data_dir: Option<PathBuf>,

    /// Enable the optional localhost HTTP transport.
    #[arg(long)]
    enable_http: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if let Err(e) = run_daemon(cli).await {
        eprintln!("{e}");
        process::exit(1);
    }
}

async fn run_daemon(cli: Cli) -> Result<(), String> {
    info!(
        config = %cli.config.display(),
        enable_http = cli.enable_http,
        "starting pacto-bot-api daemon"
    );

    let config =
        DaemonConfig::load(&cli.config).map_err(|e| format!("failed to load config: {e}"))?;

    let data_dir = cli
        .data_dir
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| config.data_dir().to_string());

    fs::create_dir_all(&data_dir)
        .map_err(|e| format!("failed to create data directory {}: {e}", data_dir))?;

    let lock_path = Path::new(&data_dir).join(DAEMON_LOCK_FILE);
    let lock_file = open_lock_file(&lock_path)
        .map_err(|e| format!("failed to open lock file {}: {e}", lock_path.display()))?;

    if lock_file.try_lock_exclusive().is_err() {
        return Err(format!(
            "daemon is already running (lock held at {})",
            lock_path.display()
        ));
    }

    let db_path = Path::new(&data_dir).join(AGENT_DB_FILE);
    let db = Database::open(&db_path)
        .map_err(|e| format!("failed to open database {}: {e}", db_path.display()))?;

    for bot in &config.bots {
        let valid = db
            .validate_npub(&bot.id, &bot.npub)
            .map_err(|e| format!("failed to validate stored npub for {}: {e}", bot.id))?;
        if !valid {
            warn!(
                bot_id = %bot.id,
                "stored npub does not match config; resetting cursor"
            );
            db.reset_cursor(&bot.id)
                .map_err(|e| format!("failed to reset cursor for {}: {e}", bot.id))?;
        }
    }

    if cli.enable_http {
        warn!("localhost HTTP transport is enabled; ensure the secret token is protected");
    }

    if config
        .bots
        .iter()
        .any(|b| matches!(b.signing, pacto_bot_api::config::SigningConfig::Nsec { .. }))
    {
        warn!("local test key (nsec) in use — not for production");
    }

    let unique_relays: Vec<String> = config
        .bots
        .iter()
        .flat_map(|b| b.relays.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let nostr_client = NostrClient::new(unique_relays)
        .await
        .map_err(|e| format!("failed to initialize nostr client: {e}"))?;

    let mut client_manager = ClientManager::new(config.clone(), nostr_client.clone())
        .map_err(|e| format!("failed to initialize client manager: {e}"))?;

    // Register every bot signer so gift wraps addressed to its pubkey can be
    // decrypted. The NostrClient clone held by ClientManager shares the same
    // signer map, so registrations apply to both handles.
    for (pubkey, bot) in client_manager.bots() {
        let signer: Arc<dyn Signer> = Arc::new(bot.signer.clone());
        nostr_client
            .add_signer(*pubkey, bot.bot_id().to_string(), signer)
            .await;
    }

    // Subscribe each bot to its gift-wrap filter and remember the subscription
    // id in the bot state.
    let pubkeys: Vec<_> = client_manager.bots().map(|(k, _)| *k).collect();
    for pubkey in pubkeys {
        let sub_id = nostr_client
            .subscribe_bot(&pubkey)
            .await
            .map_err(|e| format!("failed to subscribe bot: {e}"))?;
        let bot = client_manager
            .get_bot_mut(&pubkey)
            .ok_or("bot disappeared during subscription")?;
        bot.add_subscription(sub_id.to_string());
    }

    let diagnostics = Diagnostics::new();
    diagnostics.set_status(DaemonStatus::Initializing);

    let dispatch = Arc::new(Dispatch::new(
        Arc::new(RwLock::new(client_manager)),
        db,
        diagnostics.clone(),
    ));

    let transport = TransportLayer::new(&config, cli.enable_http);

    let (event_loop_shutdown_tx, mut event_loop_shutdown_rx) = oneshot::channel();
    let (transport_shutdown_tx, transport_shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = event_loop_shutdown_tx.send(());
            let _ = transport_shutdown_tx.send(());
        }
    });

    let transport_handle = tokio::spawn(transport.run(dispatch.clone(), transport_shutdown_rx));

    emit_agent_status(&diagnostics, &dispatch, "initializing").await;
    diagnostics.set_status(DaemonStatus::Ready);
    emit_agent_status(&diagnostics, &dispatch, "ready").await;

    info!("pacto-bot-api daemon ready");

    let mut event_stream = nostr_client.receive_events();
    let mut flush_interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            biased;
            _ = &mut event_loop_shutdown_rx => break,
            _ = flush_interval.tick() => {
                if let Err(e) = diagnostics.flush_report(Path::new(&data_dir)) {
                    warn!(error = %e, "failed to flush diagnostics report");
                }
            }
            Some(event_result) = event_stream.next() => {
                match event_result {
                    Ok(event) => {
                        if let Err(e) = dispatch.dispatch_event(event).await {
                            warn!(error = %e, "failed to dispatch event");
                        }
                    }
                    Err(e) => warn!(error = %e, "nostr event error"),
                }
            }
        }
    }

    info!("pacto-bot-api daemon shutting down");
    diagnostics.set_status(DaemonStatus::ShuttingDown);
    emit_agent_status(&diagnostics, &dispatch, "shutting_down").await;

    let _ = transport_handle.await;

    diagnostics.set_status(DaemonStatus::Stopped);
    if let Err(e) = diagnostics.flush_report(Path::new(&data_dir)) {
        warn!(error = %e, "failed to flush final diagnostics report");
    }

    Ok(())
}

/// Open the daemon lock file with owner-only permissions.
fn open_lock_file(path: &Path) -> Result<File, std::io::Error> {
    #[cfg(unix)]
    {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
    }
}

/// Emit an `agent.status` lifecycle notification.
///
/// The transport layer currently handles inbound JSON-RPC traffic; outbound
/// push to handlers is not yet wired. Until that channel exists we log the
/// notification and defer delivery to handler registration time.
async fn emit_agent_status(diagnostics: &Diagnostics, _dispatch: &Dispatch, state: &str) {
    let status = match state {
        "initializing" => DaemonStatus::Initializing,
        "ready" => DaemonStatus::Ready,
        "shutting_down" => DaemonStatus::ShuttingDown,
        _ => DaemonStatus::Stopped,
    };
    diagnostics.set_status(status);
    info!(method = "agent.status", state, "broadcasting agent status");
}

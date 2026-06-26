#![allow(dead_code, clippy::needless_borrows_for_generic_args)]

mod builder;
mod checker;
mod config;
mod ipc;
mod lock;
mod notifier;
mod repo;
mod scheduler;
mod state;
mod sync;
mod utils;

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use builder::PackageBuilder;
use config::ConfigManager;
use ipc::IpcServer;
use lock::ProcessLock;
use scheduler::TimerScheduler;
use state::StateCoordinator;
use sync::SyncCoordinator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    // 2. Acquire process exclusion lock
    let mut lock = ProcessLock::new();
    if let Err(e) = lock.acquire("/tmp/aurord.lock") {
        eprintln!("{}", e);
        std::process::exit(1);
    }
    tracing::info!("Acquired startup lock on /tmp/aurord.lock");

    // 3. Load configurations
    let config_manager = ConfigManager::new();
    let config = match config_manager.load_or_create_default() {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("Failed to load config: {}", e);
            std::process::exit(1);
        }
    };
    tracing::info!(
        "Loaded config from {}",
        config_manager.get_config_path().display()
    );

    // 4. Resolve package directory path
    let home = home::home_dir().expect("Could not find home directory");
    let repo_root = home.join("git/aur");
    tracing::info!(
        "Monitoring local AUR packages directory: {}",
        repo_root.display()
    );

    // 5. Initialize shared broadcast channel for DaemonResponse messages
    let (log_tx, _log_rx) = tokio::sync::broadcast::channel(2048);

    // 6. Initialize StateCoordinator
    let state = Arc::new(RwLock::new(StateCoordinator::new(log_tx)));

    // Load available package directories
    let mut monitored_packages = Vec::new();
    if repo_root.exists() {
        if let Ok(entries) = std::fs::read_dir(&repo_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        // Only monitor if the folder name matches a package in config
                        if config.packages.contains_key(name) {
                            monitored_packages.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
    tracing::info!(
        "Found {} configured packages under {}",
        monitored_packages.len(),
        repo_root.display()
    );

    {
        let mut s = state.write().await;
        s.init_packages(&monitored_packages);
    }

    // 7. Setup Build Queue and worker task (sequential compilation)
    let notification_webhook_url = config
        .discord
        .as_ref()
        .and_then(|d| d.notification_webhook_url.clone());
    let error_webhook_url = config
        .discord
        .as_ref()
        .and_then(|d| d.error_webhook_url.clone());
    let notifier = Arc::new(notifier::DiscordNotifier::new(
        notification_webhook_url,
        error_webhook_url,
    ));

    let (build_tx, mut build_rx) = tokio::sync::mpsc::unbounded_channel::<(String, String, bool)>();
    let builder = PackageBuilder::new(state.clone(), repo_root.clone(), notifier);

    tokio::spawn(async move {
        tracing::info!("Started sequential package compilation worker task.");
        while let Some((pkg_name, upstream_version, is_forced)) = build_rx.recv().await {
            tracing::info!("Sequential Build Worker: compiling package {}", pkg_name);
            if let Err(e) = builder
                .upgrade_package(pkg_name.clone(), upstream_version, is_forced)
                .await
            {
                tracing::error!("Upgrade failed for package {}: {}", pkg_name, e);
            }
        }
    });

    // 8. Setup SyncCoordinator
    let sync_coordinator = Arc::new(SyncCoordinator::new(
        state.clone(),
        config,
        repo_root,
        build_tx,
    ));

    // 9. Start periodic scheduler task (recurring 3h intervals)
    let scheduler = TimerScheduler::new(state.clone(), sync_coordinator.clone());
    tokio::spawn(async move {
        scheduler.run().await;
    });

    // 10. Start metadata broadcast task (broadcasts uptime and next check countdown every 1s)
    let state_clone = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            let s = state_clone.read().await;
            s.broadcast_metadata();
        }
    });

    // 11. Run IPC Socket Server
    let xdg_runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".to_string());
    let socket_path = PathBuf::from(xdg_runtime_dir)
        .join("aurord.sock")
        .to_string_lossy()
        .into_owned();
    tracing::info!("Starting IPC server on socket: {}", socket_path);
    let ipc_server = IpcServer::new(socket_path, state, sync_coordinator);

    if let Err(e) = ipc_server.run().await {
        tracing::error!("IPC Server crashed: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

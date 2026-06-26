use crate::state::StateCoordinator;
use crate::sync::SyncCoordinator;
use shared::{read_message, write_message, DaemonRequest, DaemonResponse};
use std::path::Path;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::RwLock;

pub struct IpcServer {
    socket_path: String,
    state: Arc<RwLock<StateCoordinator>>,
    sync_coordinator: Arc<SyncCoordinator>,
}

impl IpcServer {
    pub fn new(
        socket_path: String,
        state: Arc<RwLock<StateCoordinator>>,
        sync_coordinator: Arc<SyncCoordinator>,
    ) -> Self {
        Self {
            socket_path,
            state,
            sync_coordinator,
        }
    }

    /// Run the IPC Server listener.
    pub async fn run(&self) -> Result<(), String> {
        let path = Path::new(&self.socket_path);

        // Remove existing socket if any
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }

        // Create parent directory if missing
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let listener = UnixListener::bind(path)
            .map_err(|e| format!("Failed to bind UNIX domain socket: {}", e))?;

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let state = self.state.clone();
                    let sync = self.sync_coordinator.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, state, sync).await {
                            tracing::error!("IPC client connection ended with error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Failed to accept IPC connection: {}", e);
                }
            }
        }
    }

    async fn handle_client(
        stream: tokio::net::UnixStream,
        state: Arc<RwLock<StateCoordinator>>,
        sync: Arc<SyncCoordinator>,
    ) -> std::io::Result<()> {
        let (rx, tx) = stream.into_split();
        let mut rx_reader = tokio::io::BufReader::new(rx);
        let tx_shared = Arc::new(tokio::sync::Mutex::new(tx));

        let mut broadcast_rx = {
            let s = state.read().await;
            s.subscribe_logs()
        };

        // Spawn a background task to forward broadcasted daemon logs/events to the client
        let tx_shared_clone = tx_shared.clone();
        let mut bcast_task = tokio::spawn(async move {
            while let Ok(msg) = broadcast_rx.recv().await {
                let mut locked_tx = tx_shared_clone.lock().await;
                if write_message(&mut *locked_tx, &msg).await.is_err() {
                    break;
                }
            }
        });

        // Request-response loop
        loop {
            let req_opt: Option<DaemonRequest> = tokio::select! {
                res = read_message(&mut rx_reader) => {
                    match res {
                        Ok(val) => val,
                        Err(e) => {
                            tracing::debug!("Client disconnected or failed to read request: {}", e);
                            break;
                        }
                    }
                }
                _ = &mut bcast_task => {
                    break;
                }
            };

            let req = match req_opt {
                Some(r) => r,
                None => break, // EOF reached
            };

            match req {
                DaemonRequest::GetStatus => {
                    let (status_list, metadata_msg) = {
                        let s = state.read().await;
                        (s.get_status_list(), s.get_metadata_response())
                    };
                    {
                        let mut locked_tx = tx_shared.lock().await;
                        write_message(&mut *locked_tx, &DaemonResponse::Status(status_list))
                            .await?;
                        write_message(&mut *locked_tx, &metadata_msg).await?;
                    }
                }
                DaemonRequest::StreamLogs => {
                    let logs = {
                        let s = state.read().await;
                        s.get_log_history()
                    };
                    {
                        let mut locked_tx = tx_shared.lock().await;
                        for log_line in logs {
                            write_message(&mut *locked_tx, &DaemonResponse::LogLine(log_line))
                                .await?;
                        }
                    }
                }
                DaemonRequest::TriggerUpdate(pkg_name) => {
                    let sync_clone = sync.clone();
                    tokio::spawn(async move {
                        if pkg_name == "all" {
                            sync_clone.check_all(true).await;
                        } else {
                            sync_clone.check_package(pkg_name, true).await;
                        }
                    });
                }
            }
        }

        bcast_task.abort();
        Ok(())
    }
}

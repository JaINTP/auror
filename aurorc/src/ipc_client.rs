use shared::{read_message, write_message, DaemonRequest, DaemonResponse};
use std::path::Path;
use tokio::net::UnixStream;
use tokio::sync::mpsc::UnboundedSender;

pub struct IpcClient {
    tx: Option<tokio::net::unix::OwnedWriteHalf>,
}

impl IpcClient {
    pub fn new() -> Self {
        Self { tx: None }
    }

    /// Connects to the daemon UNIX socket and starts forwarding responses to response_tx.
    pub async fn connect<P: AsRef<Path>>(
        &mut self,
        path: P,
        response_tx: UnboundedSender<DaemonResponse>,
    ) -> Result<(), String> {
        let stream = UnixStream::connect(path)
            .await
            .map_err(|e| format!("Failed to connect to aurord.sock: {}", e))?;

        let (rx, tx) = stream.into_split();
        self.tx = Some(tx);

        // Spawn a task to read responses from the socket and forward them to the UI thread
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(rx);
            while let Ok(Some(msg)) = read_message::<_, DaemonResponse>(&mut reader).await {
                if response_tx.send(msg).is_err() {
                    break;
                }
            }
        });

        Ok(())
    }

    /// Sends a DaemonRequest to the socket.
    pub async fn send_request(&mut self, req: &DaemonRequest) -> Result<(), String> {
        if let Some(ref mut tx) = self.tx {
            write_message(tx, req)
                .await
                .map_err(|e| format!("Failed to send request to daemon: {}", e))
        } else {
            Err("Not connected to daemon".to_string())
        }
    }

    /// Disconnects from the daemon.
    pub fn disconnect(&mut self) {
        self.tx = None;
    }
}

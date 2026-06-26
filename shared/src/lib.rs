use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum DaemonRequest {
    GetStatus,
    TriggerUpdate(String), // Package name or "all"
    StreamLogs,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum DaemonResponse {
    Status(Vec<PackageStatus>),
    LogLine(String),
    UpdateComplete(String, bool), // package_name, success
    Metadata {
        uptime_secs: u64,
        countdown_secs: u64,
        daemon_state: String, // "Idle" | "Syncing" | "Building"
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageStatus {
    pub name: String,
    pub current_version: String,
    pub upstream_version: String,
    pub status: StatusState,
    pub last_checked: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum StatusState {
    UpToDate,
    Outdated,
    Checking,
    Building,
    Failed(String),
}

/// Helper to serialize and write a message over a newline-delimited JSON stream.
pub async fn write_message<W, T>(writer: &mut W, msg: &T) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
{
    let mut serialized = serde_json::to_vec(msg)?;
    serialized.push(b'\n');
    writer.write_all(&serialized).await?;
    writer.flush().await?;
    Ok(())
}

/// Helper to read and deserialize a message from a newline-delimited JSON stream.
pub async fn read_message<R, T>(reader: &mut R) -> std::io::Result<Option<T>>
where
    R: AsyncBufReadExt + Unpin,
    T: serde::de::DeserializeOwned,
{
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        return Ok(None);
    }
    let msg: T = serde_json::from_str(&line)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(Some(msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn test_serialization_roundtrip() {
        let req = DaemonRequest::TriggerUpdate("my-package".to_string());
        let mut buffer = Vec::new();
        write_message(&mut buffer, &req).await.unwrap();

        assert!(!buffer.is_empty());
        assert_eq!(*buffer.last().unwrap(), b'\n');

        let mut reader = BufReader::new(&buffer[..]);
        let read_req: DaemonRequest = read_message(&mut reader).await.unwrap().unwrap();

        match read_req {
            DaemonRequest::TriggerUpdate(name) => assert_eq!(name, "my-package"),
            _ => panic!("Expected TriggerUpdate"),
        }
    }
}

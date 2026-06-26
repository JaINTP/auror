use shared::{DaemonResponse, PackageStatus, StatusState};
use std::collections::{HashMap, VecDeque};
use std::time::{Instant, SystemTime};

pub struct StateCoordinator {
    packages: HashMap<String, PackageStatus>,
    log_history: VecDeque<String>,
    uptime_start: Instant,
    next_check: Instant,
    daemon_state: String,
    log_tx: tokio::sync::broadcast::Sender<DaemonResponse>,
}

impl StateCoordinator {
    pub fn new(log_tx: tokio::sync::broadcast::Sender<DaemonResponse>) -> Self {
        Self {
            packages: HashMap::new(),
            log_history: VecDeque::with_capacity(500),
            uptime_start: Instant::now(),
            next_check: Instant::now() + std::time::Duration::from_secs(3 * 3600),
            daemon_state: "Idle".to_string(),
            log_tx,
        }
    }

    pub fn subscribe_logs(&self) -> tokio::sync::broadcast::Receiver<DaemonResponse> {
        self.log_tx.subscribe()
    }

    pub fn send_response(&self, response: DaemonResponse) {
        let _ = self.log_tx.send(response);
    }

    pub fn init_packages(&mut self, pkg_names: &[String]) {
        for name in pkg_names {
            self.packages.insert(
                name.clone(),
                PackageStatus {
                    name: name.clone(),
                    current_version: "Unknown".to_string(),
                    upstream_version: "Unknown".to_string(),
                    status: StatusState::UpToDate,
                    last_checked: "Never".to_string(),
                },
            );
        }
        self.broadcast_status();
    }

    pub fn get_status_list(&self) -> Vec<PackageStatus> {
        let mut list: Vec<PackageStatus> = self.packages.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn update_package_status(&mut self, name: &str, status: StatusState) {
        if let Some(pkg) = self.packages.get_mut(name) {
            pkg.status = status;
            pkg.last_checked = Self::current_timestamp();
            self.broadcast_status();
        }
    }

    pub fn update_package_versions(&mut self, name: &str, current: &str, upstream: &str) {
        if let Some(pkg) = self.packages.get_mut(name) {
            pkg.current_version = current.to_string();
            pkg.upstream_version = upstream.to_string();
            pkg.last_checked = Self::current_timestamp();
            self.broadcast_status();
        }
    }

    pub fn add_log_line(&mut self, line: String) {
        if self.log_history.len() >= 500 {
            self.log_history.pop_front();
        }
        self.log_history.push_back(line.clone());
        let _ = self.log_tx.send(DaemonResponse::LogLine(line));
    }

    pub fn get_log_history(&self) -> Vec<String> {
        self.log_history.iter().cloned().collect()
    }

    pub fn set_daemon_state(&mut self, state: &str) {
        self.daemon_state = state.to_string();
    }

    pub fn get_daemon_state(&self) -> String {
        // If any package is building/checking, we can reflect that or use the manual state
        let has_building = self
            .packages
            .values()
            .any(|p| p.status == StatusState::Building);
        let has_checking = self
            .packages
            .values()
            .any(|p| p.status == StatusState::Checking);
        if has_building {
            "Building".to_string()
        } else if has_checking {
            "Syncing".to_string()
        } else {
            self.daemon_state.clone()
        }
    }

    pub fn set_next_check(&mut self, next: Instant) {
        self.next_check = next;
    }

    pub fn get_metadata_response(&self) -> DaemonResponse {
        let uptime = self.uptime_start.elapsed().as_secs();
        let now = Instant::now();
        let countdown = if self.next_check > now {
            (self.next_check - now).as_secs()
        } else {
            0
        };
        DaemonResponse::Metadata {
            uptime_secs: uptime,
            countdown_secs: countdown,
            daemon_state: self.get_daemon_state(),
        }
    }

    pub fn broadcast_status(&self) {
        let _ = self
            .log_tx
            .send(DaemonResponse::Status(self.get_status_list()));
    }

    pub fn broadcast_metadata(&self) {
        let _ = self.log_tx.send(self.get_metadata_response());
    }

    fn current_timestamp() -> String {
        let now = SystemTime::now();
        if let Ok(elapsed) = now.duration_since(SystemTime::UNIX_EPOCH) {
            let secs = elapsed.as_secs();
            // Simple format HH:MM:SS
            let raw_sec = secs % 60;
            let raw_min = (secs / 60) % 60;
            let raw_hour = (secs / 3600 + 10) % 24; // Simple UTC+10 offset matching ADDED_METADATA (17:21:03+10:00)
            format!("{:02}:{:02}:{:02}", raw_hour, raw_min, raw_sec)
        } else {
            "Unknown".to_string()
        }
    }
}

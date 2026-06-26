use crate::checker::CheckerFactory;
use crate::config::Config;
use crate::repo::PackageRepo;
use crate::state::StateCoordinator;
use shared::StatusState;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct SyncCoordinator {
    state: Arc<RwLock<StateCoordinator>>,
    config: Config,
    repo_root: PathBuf,
    build_tx: tokio::sync::mpsc::UnboundedSender<(String, String, bool)>,
}

impl SyncCoordinator {
    pub fn new(
        state: Arc<RwLock<StateCoordinator>>,
        config: Config,
        repo_root: PathBuf,
        build_tx: tokio::sync::mpsc::UnboundedSender<(String, String, bool)>,
    ) -> Self {
        Self {
            state,
            config,
            repo_root,
            build_tx,
        }
    }

    /// Run upstream version checking for a single package.
    pub async fn check_package(&self, pkg_name: String, is_forced: bool) {
        let upstream_config = match self.config.packages.get(&pkg_name) {
            Some(cfg) => cfg,
            None => {
                let err = format!("Package {} is not configured in upstream checks.", pkg_name);
                let mut state = self.state.write().await;
                state.add_log_line(format!("[{}] {}", pkg_name, err));
                state.update_package_status(&pkg_name, StatusState::Failed(err));
                return;
            }
        };

        let repo = PackageRepo::new(pkg_name.clone(), &self.repo_root);

        {
            let mut state = self.state.write().await;
            state.update_package_status(&pkg_name, StatusState::Checking);
            state.add_log_line(format!("[{}] Checking upstream version...", pkg_name));
        }

        // Get current version
        let current_version = match repo.get_current_version() {
            Ok(v) => v,
            Err(e) => {
                let mut state = self.state.write().await;
                state.add_log_line(format!(
                    "[{}] Error reading current version: {}",
                    pkg_name, e
                ));
                state.update_package_status(&pkg_name, StatusState::Failed(e));
                return;
            }
        };

        // Fetch upstream version
        let checker = CheckerFactory::create(upstream_config);
        match checker.fetch_latest_version().await {
            Ok(upstream_version) => {
                let mut state = self.state.write().await;
                state.update_package_versions(&pkg_name, &current_version, &upstream_version);

                // Check if PKGBUILD has empty checksum arrays
                let mut has_empty_checksums = false;
                if let Ok(content) = std::fs::read_to_string(repo.path.join("PKGBUILD")) {
                    has_empty_checksums = content.contains("sha256sums_x86_64=('')")
                        || content.contains("sha256sums=('')");
                }

                let is_outdated = upstream_version != current_version || has_empty_checksums;

                if is_outdated {
                    state.add_log_line(format!(
                        "[{}] Outdated: local is {}, upstream is {}{}",
                        pkg_name,
                        current_version,
                        upstream_version,
                        if has_empty_checksums {
                            " (has empty checksums)"
                        } else {
                            ""
                        }
                    ));
                    state.update_package_status(&pkg_name, StatusState::Outdated);

                    // Queue for build
                    if let Err(e) =
                        self.build_tx
                            .send((pkg_name.clone(), upstream_version, is_forced))
                    {
                        state.add_log_line(format!(
                            "[{}] Failed to send build request: {}",
                            pkg_name, e
                        ));
                    }
                } else if is_forced {
                    state.add_log_line(format!(
                        "[{}] Up-to-date ({}), but build was explicitly forced.",
                        pkg_name, current_version
                    ));
                    // Queue for build
                    if let Err(e) = self
                        .build_tx
                        .send((pkg_name.clone(), upstream_version, true))
                    {
                        state.add_log_line(format!(
                            "[{}] Failed to send build request: {}",
                            pkg_name, e
                        ));
                    }
                } else {
                    state.add_log_line(format!("[{}] Up-to-date: {}", pkg_name, current_version));
                    state.update_package_status(&pkg_name, StatusState::UpToDate);
                }
            }
            Err(e) => {
                let mut state = self.state.write().await;
                state.add_log_line(format!(
                    "[{}] Failed to fetch upstream version: {}",
                    pkg_name, e
                ));
                state.update_package_status(&pkg_name, StatusState::Failed(e));
            }
        }
    }

    /// Perform a check loop across all configured packages concurrently.
    pub async fn check_all(self: &Arc<Self>, is_forced: bool) {
        {
            let mut state = self.state.write().await;
            state.add_log_line("Kicking off global upstream verification pass...".to_string());
        }

        let mut handles = Vec::new();
        let pkg_names: Vec<String> = self.config.packages.keys().cloned().collect();

        for name in pkg_names {
            let coordinator = self.clone();
            let handle = tokio::spawn(async move {
                coordinator.check_package(name, is_forced).await;
            });
            handles.push(handle);
        }

        for h in handles {
            let _ = h.await;
        }

        {
            let mut state = self.state.write().await;
            state.add_log_line("Global upstream verification pass finished.".to_string());
        }
    }
}

use crate::repo::PackageRepo;
use crate::state::StateCoordinator;
use crate::utils::get_clean_env;
use shared::{DaemonResponse, StatusState};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct PackageBuilder {
    state: Arc<RwLock<StateCoordinator>>,
    repo_root: PathBuf,
    notifier: Arc<crate::notifier::DiscordNotifier>,
}

impl PackageBuilder {
    pub fn new(
        state: Arc<RwLock<StateCoordinator>>,
        repo_root: PathBuf,
        notifier: Arc<crate::notifier::DiscordNotifier>,
    ) -> Self {
        Self {
            state,
            repo_root,
            notifier,
        }
    }

    /// Run the upgrade pipeline for a package.
    pub async fn upgrade_package(
        &self,
        pkg_name: String,
        upstream_version: String,
        is_forced: bool,
    ) -> Result<(), String> {
        let repo = PackageRepo::new(pkg_name.clone(), &self.repo_root);
        let clean_env = get_clean_env();

        let current_version = repo
            .get_current_version()
            .unwrap_or_else(|_| "Unknown".to_string());

        // Log start
        self.log(format!(
            "[{}] Starting package update pipeline. Target version: {} (current: {})",
            pkg_name, upstream_version, current_version
        ))
        .await;

        self.set_status(&pkg_name, StatusState::Building).await;

        let is_git = repo.is_git_repo();
        let mut stash_ran = false;

        if is_git {
            self.log(format!(
                "[{}] Git repository detected. Stashing local changes...",
                pkg_name
            ))
            .await;
            match repo.git_stash(&clean_env).await {
                Ok(stashed) => {
                    stash_ran = stashed;
                    if stash_ran {
                        self.log(format!("[{}] Stashed local modifications.", pkg_name))
                            .await;
                    }
                }
                Err(e) => {
                    self.log(format!("[{}] Warning: git stash failed: {}", pkg_name, e))
                        .await;
                }
            }

            self.log(format!(
                "[{}] Pulling/rebasing tracked upstream remote...",
                pkg_name
            ))
            .await;
            if let Err(e) = repo.git_pull_or_rebase(&clean_env).await {
                self.log(format!("[{}] Error pulling git changes: {}", pkg_name, e))
                    .await;
                self.cleanup_and_fail(&pkg_name, &repo, &clean_env, stash_ran, e)
                    .await;
                return Err("Git pull failed".to_string());
            }

            if stash_ran {
                self.log(format!("[{}] Popping git stash...", pkg_name))
                    .await;
                if let Err(e) = repo.git_stash_pop(&clean_env).await {
                    self.log(format!(
                        "[{}] Warning: git stash pop failed: {}",
                        pkg_name, e
                    ))
                    .await;
                }
                stash_ran = false; // stash popped
            }
        }

        // Check if we need to modify PKGBUILD (only if version differs or is_forced or checksums are empty)
        let mut has_empty_checksums = false;
        if let Ok(content) = std::fs::read_to_string(repo.path.join("PKGBUILD")) {
            has_empty_checksums =
                content.contains("sha256sums_x86_64=('')") || content.contains("sha256sums=('')");
        }

        if current_version == upstream_version && !is_forced && !has_empty_checksums {
            self.log(format!(
                "[{}] Package version already up to date ({})",
                pkg_name, current_version
            ))
            .await;
            self.set_status(&pkg_name, StatusState::UpToDate).await;
            self.update_versions(&pkg_name, &current_version, &upstream_version)
                .await;
            self.broadcast_complete(pkg_name, true).await;
            return Ok(());
        }

        self.log(format!("[{}] Updating PKGBUILD file...", pkg_name))
            .await;
        if let Err(e) = repo.update_pkgbuild(&upstream_version) {
            self.log(format!("[{}] Failed to update PKGBUILD: {}", pkg_name, e))
                .await;
            self.cleanup_and_fail(&pkg_name, &repo, &clean_env, stash_ran, e)
                .await;
            return Err("PKGBUILD update failed".to_string());
        }

        self.log(format!("[{}] Running updpkgsums...", pkg_name))
            .await;
        if let Err(e) = repo.run_updpkgsums(&clean_env).await {
            self.log(format!("[{}] updpkgsums failed: {}", pkg_name, e))
                .await;
            self.cleanup_and_fail(&pkg_name, &repo, &clean_env, stash_ran, e)
                .await;
            return Err("updpkgsums failed".to_string());
        }

        self.log(format!("[{}] Regenerating .SRCINFO...", pkg_name))
            .await;
        match repo.run_printsrcinfo(&clean_env).await {
            Ok(stdout) => {
                if let Err(e) = std::fs::write(repo.path.join(".SRCINFO"), stdout) {
                    let err_msg = format!("Failed to write .SRCINFO: {}", e);
                    self.log(format!("[{}] {}", pkg_name, err_msg)).await;
                    self.cleanup_and_fail(&pkg_name, &repo, &clean_env, stash_ran, err_msg)
                        .await;
                    return Err("SRCINFO write failed".to_string());
                }
            }
            Err(e) => {
                self.log(format!(
                    "[{}] makepkg --printsrcinfo failed: {}",
                    pkg_name, e
                ))
                .await;
                self.cleanup_and_fail(&pkg_name, &repo, &clean_env, stash_ran, e)
                    .await;
                return Err("SRCINFO generation failed".to_string());
            }
        }

        self.log(format!("[{}] Running makepkg build test...", pkg_name))
            .await;
        let state_clone = self.state.clone();
        let build_res = repo
            .run_makepkg_and_stream_logs(&clean_env, move |line| {
                let mut state = tokio::task::block_in_place(|| state_clone.blocking_write());
                state.add_log_line(line);
            })
            .await;

        if let Err(e) = build_res {
            self.log(format!("[{}] Build test failed: {}", pkg_name, e))
                .await;
            self.cleanup_and_fail(&pkg_name, &repo, &clean_env, stash_ran, e)
                .await;
            return Err("makepkg build failed".to_string());
        }

        self.log(format!(
            "[{}] Build test succeeded. Cleaning up built package files...",
            pkg_name
        ))
        .await;
        if let Err(e) = repo.cleanup_built_packages() {
            self.log(format!(
                "[{}] Warning: build artifact cleanup failed: {}",
                pkg_name, e
            ))
            .await;
        }

        if is_git {
            self.log(format!(
                "[{}] Committing and pushing version bump to remote...",
                pkg_name
            ))
            .await;
            if let Err(e) = repo
                .git_commit_and_push(&clean_env, &upstream_version)
                .await
            {
                self.log(format!(
                    "[{}] Warning: git commit/push failed: {}",
                    pkg_name, e
                ))
                .await;
            }
        }

        self.log(format!("[{}] Update completed successfully.", pkg_name))
            .await;
        self.set_status(&pkg_name, StatusState::UpToDate).await;
        self.update_versions(&pkg_name, &upstream_version, &upstream_version)
            .await;
        self.broadcast_complete(pkg_name.clone(), true).await;
        self.notifier
            .notify_success(&pkg_name, &upstream_version)
            .await;

        Ok(())
    }

    async fn cleanup_and_fail(
        &self,
        pkg_name: &str,
        repo: &PackageRepo,
        env: &HashMap<String, String>,
        stash_ran: bool,
        err_msg: String,
    ) {
        if repo.is_git_repo() {
            self.log(format!(
                "[{}] Aborting update. Resetting git state...",
                pkg_name
            ))
            .await;
            let _ = repo.git_rollback(env).await;
            if stash_ran {
                self.log(format!(
                    "[{}] Restoring stashed local modifications...",
                    pkg_name
                ))
                .await;
                let _ = repo.git_stash_pop(env).await;
            }
        }
        let _ = repo.cleanup_built_packages();
        self.set_status(pkg_name, StatusState::Failed(err_msg.clone()))
            .await;
        self.broadcast_complete(pkg_name.to_string(), false).await;
        self.notifier.notify_failure(pkg_name, &err_msg).await;
    }

    async fn log(&self, msg: String) {
        let mut state = self.state.write().await;
        state.add_log_line(msg);
    }

    async fn set_status(&self, pkg_name: &str, status: StatusState) {
        let mut state = self.state.write().await;
        state.update_package_status(pkg_name, status);
    }

    async fn update_versions(&self, pkg_name: &str, current: &str, upstream: &str) {
        let mut state = self.state.write().await;
        state.update_package_versions(pkg_name, current, upstream);
    }

    async fn broadcast_complete(&self, pkg_name: String, success: bool) {
        let state = self.state.read().await;
        state.broadcast_status();
        state.send_response(DaemonResponse::UpdateComplete(pkg_name, success));
    }
}

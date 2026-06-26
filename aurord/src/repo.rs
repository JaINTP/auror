use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub struct PackageRepo {
    pub name: String,
    pub path: PathBuf,
}

impl PackageRepo {
    pub fn new(name: String, root_dir: &Path) -> Self {
        Self {
            name: name.clone(),
            path: root_dir.join(name),
        }
    }

    /// Read the current version from .SRCINFO or PKGBUILD
    pub fn get_current_version(&self) -> Result<String, String> {
        let srcinfo_path = self.path.join(".SRCINFO");
        if srcinfo_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&srcinfo_path) {
                let re = Regex::new(r"(?m)^\s*pkgver\s*=\s*([^\s#]+)").unwrap();
                if let Some(caps) = re.captures(&content) {
                    if let Some(m) = caps.get(1) {
                        return Ok(m.as_str().trim().to_string());
                    }
                }
            }
        }

        let pkgbuild_path = self.path.join("PKGBUILD");
        if pkgbuild_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&pkgbuild_path) {
                let re = Regex::new(r"(?m)^pkgver=([^\s#]+)").unwrap();
                if let Some(caps) = re.captures(&content) {
                    if let Some(m) = caps.get(1) {
                        return Ok(m.as_str().trim().to_string());
                    }
                }
            }
        }

        Err(format!(
            "Could not parse pkgver from .SRCINFO or PKGBUILD in {}",
            self.path.display()
        ))
    }

    /// Checks if there's a .git directory in the package path
    pub fn is_git_repo(&self) -> bool {
        self.path.join(".git").exists()
    }

    /// Run git stash and return true if a stash was actually created
    pub async fn git_stash(&self, env: &HashMap<String, String>) -> Result<bool, String> {
        let output = Command::new("git")
            .arg("stash")
            .current_dir(&self.path)
            .envs(env)
            .output()
            .await
            .map_err(|e| format!("git stash failed: {}", e))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git stash exited with error: {}", err.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stashed = !stdout.contains("No local changes to save");
        Ok(stashed)
    }

    /// Run git stash pop
    pub async fn git_stash_pop(&self, env: &HashMap<String, String>) -> Result<(), String> {
        let status = Command::new("git")
            .args(&["stash", "pop"])
            .current_dir(&self.path)
            .envs(env)
            .status()
            .await
            .map_err(|e| format!("git stash pop execution failed: {}", e))?;

        if !status.success() {
            return Err("git stash pop failed".to_string());
        }
        Ok(())
    }

    /// Resolves the upstream remote/branch and executes fetch + merge/rebase
    pub async fn git_pull_or_rebase(&self, env: &HashMap<String, String>) -> Result<(), String> {
        let upstream_res = Command::new("git")
            .args(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
            .current_dir(&self.path)
            .envs(env)
            .output()
            .await;

        let mut remote = "origin".to_string();
        let mut branch = "master".to_string();

        if let Ok(output) = upstream_res {
            if output.status.success() {
                let upstream = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Some(idx) = upstream.find('/') {
                    remote = upstream[..idx].to_string();
                    branch = upstream[idx + 1..].to_string();
                }
            }
        }

        // Fetch tracked remote
        let fetch_status = Command::new("git")
            .args(&["fetch", &remote])
            .current_dir(&self.path)
            .envs(env)
            .status()
            .await
            .map_err(|e| format!("git fetch failed: {}", e))?;

        if !fetch_status.success() {
            return Err("git fetch failed".to_string());
        }

        // Check pull.rebase config
        let rebase_check = Command::new("git")
            .args(&["config", "--get", "pull.rebase"])
            .current_dir(&self.path)
            .envs(env)
            .output()
            .await;

        let use_rebase = if let Ok(output) = rebase_check {
            String::from_utf8_lossy(&output.stdout).trim() == "true"
        } else {
            false
        };

        let cmd_type = if use_rebase { "rebase" } else { "merge" };
        let target = format!("{}/{}", remote, branch);
        let pull_status = Command::new("git")
            .args(&[cmd_type, &target])
            .current_dir(&self.path)
            .envs(env)
            .status()
            .await
            .map_err(|e| format!("git {} failed: {}", cmd_type, e))?;

        if !pull_status.success() {
            return Err(format!("git {} failed", cmd_type));
        }

        Ok(())
    }

    /// Mutate PKGBUILD file content (version fields, reset pkgrel, empty checksum fields)
    pub fn update_pkgbuild(&self, new_version: &str) -> Result<(), String> {
        let pkgbuild_path = self.path.join("PKGBUILD");
        if !pkgbuild_path.exists() {
            return Err("PKGBUILD file does not exist".to_string());
        }

        let content = std::fs::read_to_string(&pkgbuild_path)
            .map_err(|e| format!("Failed to read PKGBUILD: {}", e))?;

        let re_pkgver = Regex::new(r"(?m)^pkgver=.*$").unwrap();
        let re_version = Regex::new(r"(?m)^_version=.*$").unwrap();
        let re_pkgver_var = Regex::new(r"(?m)^_pkgver=.*$").unwrap();
        let re_pkgrel = Regex::new(r"(?m)^pkgrel=.*$").unwrap();

        let mut content = re_pkgver
            .replace_all(&content, format!("pkgver={}", new_version))
            .into_owned();
        content = re_version
            .replace_all(&content, format!("_version={}", new_version))
            .into_owned();
        content = re_pkgver_var
            .replace_all(&content, format!("_pkgver={}", new_version))
            .into_owned();
        content = re_pkgrel.replace_all(&content, "pkgrel=1").into_owned();

        // Sanitize empty checksum arrays (replace '' or "" with 'SKIP')
        content = self.sanitize_checksum_fields(&content);

        std::fs::write(&pkgbuild_path, content)
            .map_err(|e| format!("Failed to write PKGBUILD: {}", e))?;

        Ok(())
    }

    fn sanitize_checksum_fields(&self, content: &str) -> String {
        let re = Regex::new(r"(?m)(\b\w*sums\w*\s*=\s*\(\s*)([^)]*)(\s*\))").unwrap();
        re.replace_all(content, |caps: &regex::Captures| {
            let prefix = &caps[1];
            let array_content = &caps[2];
            let suffix = &caps[3];
            let sanitized = array_content
                .replace("''", "'SKIP'")
                .replace("\"\"", "'SKIP'");
            format!("{}{}{}", prefix, sanitized, suffix)
        })
        .into_owned()
    }

    /// Run updpkgsums in package directory
    pub async fn run_updpkgsums(&self, env: &HashMap<String, String>) -> Result<(), String> {
        let status = Command::new("updpkgsums")
            .current_dir(&self.path)
            .envs(env)
            .status()
            .await
            .map_err(|e| format!("updpkgsums execution failed: {}", e))?;

        if !status.success() {
            return Err("updpkgsums failed".to_string());
        }
        Ok(())
    }

    /// Run makepkg --printsrcinfo and return stdout bytes
    pub async fn run_printsrcinfo(&self, env: &HashMap<String, String>) -> Result<Vec<u8>, String> {
        let output = Command::new("makepkg")
            .arg("--printsrcinfo")
            .current_dir(&self.path)
            .envs(env)
            .output()
            .await
            .map_err(|e| format!("makepkg --printsrcinfo execution failed: {}", e))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("makepkg --printsrcinfo failed: {}", err.trim()));
        }
        Ok(output.stdout)
    }

    /// Spawn makepkg and stream output line-by-line
    pub async fn run_makepkg_and_stream_logs<F>(
        &self,
        env: &HashMap<String, String>,
        mut log_callback: F,
    ) -> Result<(), String>
    where
        F: FnMut(String) + Send + 'static,
    {
        let mut child = Command::new("makepkg")
            .args(&["--noconfirm", "-cf"])
            .current_dir(&self.path)
            .envs(env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn makepkg: {}", e))?;

        let stdout = child.stdout.take().ok_or("Failed to open stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to open stderr")?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(100);

        let tx_out = tx.clone();
        tokio::spawn(async move {
            while let Ok(Some(line)) = stdout_reader.next_line().await {
                let _ = tx_out.send(line).await;
            }
        });

        let tx_err = tx.clone();
        tokio::spawn(async move {
            while let Ok(Some(line)) = stderr_reader.next_line().await {
                let _ = tx_err.send(line).await;
            }
        });

        // Drop our sender so the channel closes when the helpers finish
        drop(tx);

        while let Some(line) = rx.recv().await {
            log_callback(line);
        }

        let status = child
            .wait()
            .await
            .map_err(|e| format!("Failed waiting for makepkg: {}", e))?;

        if !status.success() {
            return Err(format!("makepkg exited with non-zero code: {}", status));
        }

        Ok(())
    }

    /// Clean up built packages (*.pkg.tar.*) to save disk space
    pub fn cleanup_built_packages(&self) -> Result<(), String> {
        let entries = std::fs::read_dir(&self.path)
            .map_err(|e| format!("Failed to read package directory: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                    if filename.contains(".pkg.tar.") {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
        Ok(())
    }

    /// Commit the version bump and push to git upstream remote
    pub async fn git_commit_and_push(
        &self,
        env: &HashMap<String, String>,
        new_version: &str,
    ) -> Result<(), String> {
        // git add PKGBUILD .SRCINFO
        let add_status = Command::new("git")
            .args(&["add", "PKGBUILD", ".SRCINFO"])
            .current_dir(&self.path)
            .envs(env)
            .status()
            .await
            .map_err(|e| format!("git add failed: {}", e))?;

        if !add_status.success() {
            return Err("git add failed".to_string());
        }

        // Check if there are differences staged
        let diff_status = Command::new("git")
            .args(&["diff", "--cached", "--quiet"])
            .current_dir(&self.path)
            .envs(env)
            .status()
            .await
            .map_err(|e| format!("git diff failed: {}", e))?;

        if diff_status.code() == Some(1) {
            // Commit changes
            let commit_msg = format!("Bumped {} to v{}", self.name, new_version);
            let commit_status = Command::new("git")
                .args(&["commit", "-m", &commit_msg])
                .current_dir(&self.path)
                .envs(env)
                .status()
                .await
                .map_err(|e| format!("git commit failed: {}", e))?;

            if !commit_status.success() {
                return Err("git commit failed".to_string());
            }

            // Push changes
            let push_status = Command::new("git")
                .arg("push")
                .current_dir(&self.path)
                .envs(env)
                .status()
                .await
                .map_err(|e| format!("git push failed: {}", e))?;

            if !push_status.success() {
                return Err("git push failed".to_string());
            }
        }

        Ok(())
    }

    /// Reset any local modifications to PKGBUILD and .SRCINFO
    pub async fn git_rollback(&self, env: &HashMap<String, String>) -> Result<(), String> {
        let status = Command::new("git")
            .args(&["checkout", "PKGBUILD", ".SRCINFO"])
            .current_dir(&self.path)
            .envs(env)
            .status()
            .await
            .map_err(|e| format!("git checkout rollback failed: {}", e))?;

        if !status.success() {
            return Err("git rollback checkout failed".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_sanitize_checksum_fields() {
        let repo = PackageRepo::new("test-pkg".to_string(), Path::new("/tmp"));

        // Case 1: single quotes empty sums
        let content1 = "sha256sums=('' '' '')";
        let expected1 = "sha256sums=('SKIP' 'SKIP' 'SKIP')";
        assert_eq!(repo.sanitize_checksum_fields(content1), expected1);

        // Case 2: double quotes empty sums
        let content2 = "sha256sums=(\"\" \"\")";
        let expected2 = "sha256sums=('SKIP' 'SKIP')";
        assert_eq!(repo.sanitize_checksum_fields(content2), expected2);

        // Case 3: non-empty sums
        let content3 = "sha256sums=('abc' 'def')";
        assert_eq!(repo.sanitize_checksum_fields(content3), content3);
    }

    #[test]
    fn test_get_current_version() {
        let temp_dir = std::env::temp_dir().join("test_pkg_repo_dir");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let repo = PackageRepo::new("test_pkg_repo_dir".to_string(), temp_dir.parent().unwrap());

        // Test with .SRCINFO
        let srcinfo_path = temp_dir.join(".SRCINFO");
        fs::write(&srcinfo_path, "pkgver = 1.2.3\n").unwrap();
        assert_eq!(repo.get_current_version().unwrap(), "1.2.3");

        // Test fallback to PKGBUILD when .SRCINFO is absent
        fs::remove_file(&srcinfo_path).unwrap();
        let pkgbuild_path = temp_dir.join("PKGBUILD");
        fs::write(&pkgbuild_path, "pkgver=4.5.6\n").unwrap();
        assert_eq!(repo.get_current_version().unwrap(), "4.5.6");

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }
}

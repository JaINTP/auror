use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

/// Returns a clean copy of the current environment variables, removing VIRTUAL_ENV and
/// stripping its bin directory from PATH to ensure makepkg/updpkgsums use the system Python.
pub fn get_clean_env() -> HashMap<String, String> {
    let mut clean_env = env::vars().collect::<HashMap<String, String>>();
    if let Some(venv_val) = clean_env.remove("VIRTUAL_ENV") {
        let venv_bin = Path::new(&venv_val).join("bin");
        if let Some(path_val) = clean_env.get("PATH").cloned() {
            let paths = env::split_paths(&path_val).collect::<Vec<PathBuf>>();
            let clean_paths = paths
                .into_iter()
                .filter(|p| p != &venv_bin)
                .collect::<Vec<PathBuf>>();
            if let Ok(new_path) = env::join_paths(clean_paths) {
                if let Some(s) = new_path.to_str() {
                    clean_env.insert("PATH".to_string(), s.to_string());
                }
            }
        }
    }
    // Disable interactive prompts to prevent automated runs from hanging
    clean_env.insert(
        "GIT_SSH_COMMAND".to_string(),
        "ssh -o BatchMode=yes".to_string(),
    );
    clean_env.insert("GIT_TERMINAL_PROMPT".to_string(), "0".to_string());
    clean_env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_clean_env_removes_virtual_env() {
        // Prepare environment mock
        std::env::set_var("VIRTUAL_ENV", "/fake/venv");
        std::env::set_var("PATH", "/fake/venv/bin:/usr/bin:/bin");

        let clean = get_clean_env();

        assert!(!clean.contains_key("VIRTUAL_ENV"));
        let path = clean.get("PATH").unwrap();
        assert!(!path.contains("/fake/venv/bin"));
        assert!(path.contains("/usr/bin"));
        assert!(path.contains("/bin"));
    }
}

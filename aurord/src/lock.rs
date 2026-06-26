use fs4::FileExt;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub struct ProcessLock {
    file: Option<std::fs::File>,
}

impl ProcessLock {
    pub fn new() -> Self {
        Self { file: None }
    }

    /// Tries to acquire an exclusive lock on the file.
    /// If another process is holding the lock, it returns an Error.
    pub fn acquire<P: AsRef<Path>>(&mut self, path: P) -> Result<(), String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .map_err(|e| format!("Failed to open lock file: {}", e))?;

        // Attempt exclusive lock without blocking
        file.try_lock_exclusive().map_err(|_| {
            "Failed to acquire lock: another instance of aurord is already running".to_string()
        })?;

        // Write the current process ID into the lockfile
        let mut file_ref = &file;
        let pid = std::process::id();
        writeln!(file_ref, "{}", pid)
            .map_err(|e| format!("Failed to write PID to lock file: {}", e))?;

        self.file = Some(file);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_lock_mechanism() {
        let mut temp_path = env::temp_dir();
        temp_path.push("test_aurord.lock");

        // Cleanup before test
        let _ = std::fs::remove_file(&temp_path);

        let mut lock1 = ProcessLock::new();
        // 1. First lock acquisition should succeed
        assert!(lock1.acquire(&temp_path).is_ok());

        // 2. Second lock acquisition on the same file should fail
        let mut lock2 = ProcessLock::new();
        assert!(lock2.acquire(&temp_path).is_err());

        // 3. Drop lock1 (releases the lock)
        drop(lock1);

        // 4. Acquisition should now succeed
        assert!(lock2.acquire(&temp_path).is_ok());

        // Cleanup
        let _ = std::fs::remove_file(&temp_path);
    }
}

//! Test helpers: isolated temp directories standing in for $HOME and cwd.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::types::Ctx;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A fresh directory removed on drop. Layout: `<root>/home` and `<root>/proj`.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("mcpm_test_{}_{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::create_dir_all(root.join("proj")).unwrap();
        TempDir(root)
    }

    pub fn home(&self) -> PathBuf {
        self.0.join("home")
    }

    pub fn ctx(&self) -> Ctx {
        Ctx {
            home: Some(self.home()),
            cwd: self.0.join("proj"),
        }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

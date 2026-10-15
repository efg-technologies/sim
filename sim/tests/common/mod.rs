//! Helpers shared by the integration tests.
#![allow(dead_code, clippy::unwrap_used)]
use std::path::{Path, PathBuf};
use std::process::Command;

/// A unique empty directory, removed when dropped.
pub struct TempDir(pub PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn temp_dir(name: &str) -> TempDir {
    let d = std::env::temp_dir().join(format!("sim-it-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    TempDir(d)
}

/// Writes `files` (relative paths; a trailing `/` makes a directory) under `dir`.
pub fn make(dir: &Path, files: &[&str]) {
    for f in files {
        let p = dir.join(f);
        if f.ends_with('/') {
            std::fs::create_dir_all(&p).unwrap();
        } else {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, f).unwrap();
        }
    }
}

/// Runs git in `dir` with none of the user's configuration (no global config, no global ignore
/// file) and a fixed identity. Returns stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=sim", "-c", "user.email=sim@example.com"])
        .args(["-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("XDG_CONFIG_HOME", "/nonexistent")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A repository in `dir` with these files committed.
pub fn repo(dir: &Path, files: &[&str]) {
    git(dir, &["init", "-q"]);
    make(dir, files);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

/// Compares `actual` with `tests/golden/{name}.txt`; `SIM_BLESS=1` rewrites the file.
pub fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/{name}.txt"));
    if std::env::var_os("SIM_BLESS").is_some() {
        std::fs::write(&path, actual).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(actual, want, "{name}: differs from {} (SIM_BLESS=1 rewrites it)", path.display());
}

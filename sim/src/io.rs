//! FIND-3, MARK-1, MARK-2
//! The impure helpers: directory listings, git, and the history file in sim's state directory.
use crate::rank::History;
use crate::tree::Entry;
use si::io::file::why;
use std::ffi::OsString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The entries of a directory. A symlink to a directory counts as one, unless it leads back to
/// the directory itself or to one above it (a loop); a link that cannot be followed is a file.
pub fn read_dir(dir: &Path) -> Result<Vec<Entry>, String> {
    let list =
        fs::read_dir(dir).map_err(|e| format!("cannot read {}: {}", dir.display(), why(&e)))?;
    let here = fs::canonicalize(dir).ok();
    let is_dir = |e: &fs::DirEntry| match e.file_type() {
        Ok(t) if t.is_symlink() => {
            let target = fs::canonicalize(e.path()).ok().filter(|t| t.is_dir());
            target.is_some_and(|t| here.as_ref().is_some_and(|h| !h.starts_with(t)))
        }
        Ok(t) => t.is_dir(),
        Err(_) => false,
    };
    Ok(list
        .filter_map(Result::ok)
        .map(|e| Entry { name: e.file_name().to_string_lossy().into_owned(), is_dir: is_dir(&e) })
        .collect())
}

/// Runs git in `dir`. In English, so that "not a git repository" can be told from other failures.
fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .arg("--no-optional-locks")
        .args(args)
        .current_dir(dir)
        .env("LC_ALL", "C")
        .output();
    let out = out.map_err(|e| format!("cannot run git in {}: {}", dir.display(), why(&e)))?;
    if out.status.success() {
        return Ok(out.stdout);
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let err = err.lines().next().unwrap_or_default();
    Err(if err.contains("not a git repository") {
        format!("not a git repository: {}", dir.display())
    } else {
        format!("git {}: {err}", args[0])
    })
}

/// FIND-3: the files of the repository, NUL-separated and relative to `dir`: tracked ones and
/// untracked ones that are not ignored.
pub fn git_files(dir: &Path) -> Result<Vec<u8>, String> {
    git(dir, &["ls-files", "--cached", "--others", "--exclude-standard", "-z"])
}

/// MARK-1: `git status --porcelain=v1 -z`, with every untracked file listed (not just its
/// directory). Its paths are relative to the top of the repository, not to `dir`.
pub fn git_status(dir: &Path) -> Result<Vec<u8>, String> {
    git(dir, &["status", "--porcelain=v1", "-z", "--untracked-files=all"])
}

/// Where `dir` lies below the top of its repository: empty at the top, else `sub/dir/`.
pub fn git_prefix(dir: &Path) -> Result<String, String> {
    let out = git(dir, &["rev-parse", "--show-prefix"])?;
    Ok(String::from_utf8_lossy(&out).trim_end_matches('\n').to_string())
}

/// MARK-2: `$XDG_STATE_HOME/sim`, else `$HOME/.local/state/sim` (the values of the two
/// variables are passed in; a relative one is ignored, as the XDG spec says). There is no
/// fallback to a temporary directory: the history must not sit where others can read it.
pub fn state_dir(xdg_state_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let abs = |v: Option<OsString>| v.map(PathBuf::from).filter(|p| p.is_absolute());
    let base = abs(xdg_state_home).or_else(|| abs(home).map(|h| h.join(".local/state")))?;
    Some(base.join("sim"))
}

/// The history saved in `dir`; empty if there is none, or it cannot be read.
pub fn load_history(dir: &Path) -> History {
    fs::read_to_string(dir.join("history"))
        .map_or_else(|_| History::default(), |text| History::parse(&text))
}

/// Saves the history in `dir` (made with 0700 if needed): a temporary file (0600) is written and
/// synced, then renamed over the old one, so the file is always whole.
pub fn save_history(dir: &Path, history: &History) -> Result<(), String> {
    let fail = |what: &str, e: std::io::Error| {
        format!("history: cannot {what} {}: {}", dir.display(), why(&e))
    };
    DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(|e| fail("create", e))?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|e| fail("protect", e))?;
    let tmp = dir.join(format!("history.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let write = || -> std::io::Result<()> {
        let mut f = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        f.write_all(history.serialize().as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, dir.join("history"))
    };
    write().map_err(|e| {
        let _ = fs::remove_file(&tmp);
        fail("write in", e)
    })?;
    let _ = File::open(dir).and_then(|d| d.sync_all()); // the rename has happened; this only makes it durable
    Ok(())
}

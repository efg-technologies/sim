//! MARK-1, UI-3
//! Which files get the change color and which directories get `~` (pure), from the output of
//! `git status --porcelain=v1 -z` and the paths seen changing on disk; and, for the interface,
//! each path's git status (`M A U D R !`), the strongest status below each directory, and which
//! entries git ignores.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// UI-3: what git says about a path, from the weakest to the strongest: a directory shows the
/// strongest status of what is below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Git {
    Untracked,
    Added,
    Renamed,
    Modified,
    Deleted,
    Conflict,
}

impl Git {
    /// The letter at the right edge of a file's row.
    pub fn letter(self) -> char {
        ['U', 'A', 'R', 'M', 'D', '!'][self as usize]
    }

    /// UI-3: the status of an entry `XY` of `git status --porcelain=v1`: a conflict when either
    /// side is unmerged (or both added or deleted it), else the work tree's letter when it is a
    /// change or a deletion, else the index's.
    fn of(x: u8, y: u8) -> Option<Git> {
        Some(match (x, y) {
            (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => Git::Conflict,
            (b'?', b'?') => Git::Untracked,
            (_, b'M' | b'T') | (b'M' | b'T', _) => Git::Modified,
            (_, b'D') | (b'D', _) => Git::Deleted,
            (b'A' | b'C', _) => Git::Added,
            (b'R', _) => Git::Renamed,
            _ => return None,
        })
    }
}

/// Paths are relative to the tree's root, as the tree's rows are.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Marks {
    files: HashSet<PathBuf>,
    dirs: HashSet<PathBuf>,
    /// The paths that changed on disk while sim ran.
    disk: HashSet<PathBuf>,
    git: HashMap<PathBuf, Git>,
    below: HashMap<PathBuf, Git>,
    ignored: HashSet<PathBuf>,
}

impl Marks {
    /// `status` is git's output. Its paths are relative to the top of the repository, so
    /// `prefix` is where the root lies below it (`git rev-parse --show-prefix`: empty at the top,
    /// else `sub/dir/`); changes outside the root are left out. `disk` are the paths (relative
    /// to the root) that changed while sim was running. Entries `!! path` are what git ignores
    /// (UI-3): a directory has a `/` at its end.
    pub fn new(status: &[u8], prefix: &str, disk: &HashSet<PathBuf>) -> Marks {
        let mut m = Marks::default();
        let rel = |p: &[u8]| String::from_utf8_lossy(p).strip_prefix(prefix).map(PathBuf::from);
        let mut fields = status.split(|&b| b == 0).filter(|f| !f.is_empty());
        while let Some(field) = fields.next() {
            if field.len() < 4 {
                continue;
            }
            let (xy, path) = (&field[..2], &field[3..]);
            if xy == b"!!" {
                let dir = path.strip_suffix(b"/").unwrap_or(path);
                m.ignored.extend(rel(dir));
                continue;
            }
            let (new, git) = (rel(path), Git::of(xy[0], xy[1]));
            m.add(new.clone());
            if let (Some(p), Some(g)) = (&new, git) {
                m.git.insert(p.clone(), g);
                m.up(p, g);
            }
            // A rename or a copy is followed by the old path: a rename removed it, a copy did not.
            if xy.contains(&b'R') || xy.contains(&b'C') {
                let old = fields.next().filter(|_| xy.contains(&b'R')).and_then(rel);
                m.add(old.clone());
                old.iter().for_each(|o| m.up(o, Git::Renamed));
            }
        }
        for p in disk {
            m.disk.insert(p.clone());
            m.add(Some(p.clone()));
        }
        m
    }

    fn add(&mut self, path: Option<PathBuf>) {
        let Some(path) = path else { return };
        self.dirs.extend(
            path.ancestors().skip(1).filter(|a| !a.as_os_str().is_empty()).map(Path::to_path_buf),
        );
        self.files.insert(path);
    }

    /// The directories above `path` take `g` if it is stronger than what they have.
    fn up(&mut self, path: &Path, g: Git) {
        for a in path.ancestors().skip(1).filter(|a| !a.as_os_str().is_empty()) {
            let s = self.below.entry(a.to_path_buf()).or_insert(g);
            *s = (*s).max(g);
        }
    }

    /// Every file that changed (MARK-3 narrows the tree to these, the open ones and the recent).
    pub fn files(&self) -> impl Iterator<Item = &PathBuf> {
        self.files.iter()
    }

    /// Whether the name of this path gets the change color.
    pub fn changed(&self, path: &Path) -> bool {
        self.files.contains(path)
    }

    /// Whether this directory gets `~`: something below it changed.
    pub fn changed_below(&self, dir: &Path) -> bool {
        self.dirs.contains(dir)
    }

    /// UI-3: whether the file changed on disk while sim ran (the green of flat and depth).
    pub fn on_disk(&self, path: &Path) -> bool {
        self.disk.contains(path)
    }

    /// UI-3: the git status of a file.
    pub fn git(&self, path: &Path) -> Option<Git> {
        self.git.get(path).copied()
    }

    /// UI-3: the strongest git status below a directory.
    pub fn below(&self, dir: &Path) -> Option<Git> {
        self.below.get(dir).copied()
    }

    /// UI-3: whether git ignores this entry, or a directory above it.
    pub fn ignored(&self, path: &Path) -> bool {
        !self.ignored.is_empty() && path.ancestors().any(|a| self.ignored.contains(a))
    }
}

//! MARK-1
//! Which files get the change color and which directories get `~` (pure), from the output of
//! `git status --porcelain=v1 -z` and the paths seen changing on disk.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Paths are relative to the tree's root, as the tree's rows are.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Marks {
    files: HashSet<PathBuf>,
    dirs: HashSet<PathBuf>,
}

impl Marks {
    /// `status` is git's output. Its paths are relative to the top of the repository, so
    /// `prefix` is where the root lies below it (`git rev-parse --show-prefix`: empty at the top,
    /// else `sub/dir/`); changes outside the root are left out. `disk` are the paths (relative
    /// to the root) that changed while sim was running.
    pub fn new(status: &[u8], prefix: &str, disk: &HashSet<PathBuf>) -> Marks {
        let mut m = Marks::default();
        let mut fields = status.split(|&b| b == 0).filter(|f| !f.is_empty());
        while let Some(field) = fields.next() {
            if field.len() < 4 {
                continue;
            }
            let (xy, path) = (&field[..2], String::from_utf8_lossy(&field[3..]));
            m.add(path.strip_prefix(prefix).map(PathBuf::from));
            // A rename or a copy is followed by the old path: a rename removed it, a copy did not.
            if xy.contains(&b'R') || xy.contains(&b'C') {
                let old = fields.next().filter(|_| xy.contains(&b'R'));
                m.add(old.and_then(|o| {
                    String::from_utf8_lossy(o).strip_prefix(prefix).map(PathBuf::from)
                }));
            }
        }
        disk.iter().for_each(|p| m.add(Some(p.clone())));
        m
    }

    fn add(&mut self, path: Option<PathBuf>) {
        let Some(path) = path else { return };
        self.dirs.extend(
            path.ancestors().skip(1).filter(|a| !a.as_os_str().is_empty()).map(Path::to_path_buf),
        );
        self.files.insert(path);
    }

    /// Whether the name of this path gets the change color.
    pub fn changed(&self, path: &Path) -> bool {
        self.files.contains(path)
    }

    /// Whether this directory gets `~`: something below it changed.
    pub fn changed_below(&self, dir: &Path) -> bool {
        self.dirs.contains(dir)
    }
}

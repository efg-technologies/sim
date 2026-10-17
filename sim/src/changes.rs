//! DIFF-1, DIFF-2, DIFF-3, DIFF-4
//! The numbers of the changes view (pure): `+n −n` for each file that differs from HEAD, from
//! `git diff --numstat -z HEAD` and the line counts of the untracked files, and the sums of the
//! directories above them.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Lines added and deleted by a file, or `bin` for a file git cannot count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stat {
    pub add: usize,
    pub del: usize,
    pub bin: bool,
}

impl Stat {
    /// DIFF-2: how it is written: `+12 −3`, with a part left out when it is zero (nothing for a pure
    /// rename), and `bin` for a binary file.
    pub fn show(&self) -> String {
        let parts = [(self.add, '+'), (self.del, '−')];
        let words: Vec<String> =
            parts.iter().filter(|p| p.0 > 0).map(|p| format!("{}{}", p.1, p.0)).collect();
        if self.bin && words.is_empty() { "bin".into() } else { words.join(" ") }
    }

    fn plus(&mut self, o: Stat) {
        (self.add, self.del, self.bin) = (self.add + o.add, self.del + o.del, self.bin || o.bin);
    }
}

/// What the thread that asks git for the numbers answers: git's numstat (paths from the top of the
/// repository), and the line count of each untracked file (paths from the root; none for a file it
/// did not count: binary, or too big).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub numstat: Vec<u8>,
    pub lines: Vec<(PathBuf, Option<usize>)>,
}

/// The numbers of every file that differs from HEAD, and the sums of the directories above them.
/// Paths are relative to the tree's root, as the tree's rows are.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Changes {
    stats: HashMap<PathBuf, Stat>,
}

impl Changes {
    /// `counts` of the thread; `prefix` is where the root lies below the top of the repository
    /// (`git rev-parse --show-prefix`), and what is outside the root is left out. The format of
    /// `git diff --numstat -z` is `added TAB deleted TAB path NUL`, and for a rename
    /// `added TAB deleted TAB NUL old NUL new NUL`; a binary file has `-` for both numbers.
    pub fn new(counts: &Counts, prefix: &str) -> Changes {
        let mut files: HashMap<PathBuf, Stat> = HashMap::new();
        let rel = |p: &[u8]| String::from_utf8_lossy(p).strip_prefix(prefix).map(PathBuf::from);
        let mut fields = counts.numstat.split(|&b| b == 0);
        while let Some(field) = fields.next() {
            let mut parts = field.splitn(3, |&b| b == b'\t');
            let (Some(a), Some(d), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            // (a rename has no path of its own: the old name and the new one come next)
            let path = if path.is_empty() { fields.next().and(fields.next()) } else { Some(path) };
            let n = |x: &[u8]| std::str::from_utf8(x).ok().and_then(|x| x.parse::<usize>().ok());
            let stat = match (n(a), n(d)) {
                (Some(add), Some(del)) => Stat { add, del, bin: false },
                _ => Stat { bin: a == b"-" && d == b"-", ..Stat::default() },
            };
            if let Some(p) = path.filter(|p| !p.is_empty()).and_then(rel) {
                files.insert(p, stat);
            }
        }
        for (path, lines) in &counts.lines {
            files.insert(path.clone(), Stat { add: lines.unwrap_or(0), ..Stat::default() });
        }
        let mut stats = files.clone();
        for (path, stat) in &files {
            for dir in path.ancestors().skip(1).filter(|a| !a.as_os_str().is_empty()) {
                stats.entry(dir.to_path_buf()).or_default().plus(*stat);
            }
        }
        Changes { stats }
    }

    /// The numbers of a file, or the sums of a directory.
    pub fn get(&self, path: &Path) -> Option<Stat> {
        self.stats.get(path).copied()
    }
}

/// DIFF-2: the lines of a file's text, or none for a binary one (a NUL in its first 8 KB): the
/// newlines, and one more for a last line that has none.
pub fn lines(bytes: &[u8]) -> Option<usize> {
    if bytes.iter().take(8192).any(|&b| b == 0) {
        return None;
    }
    let n = bytes.iter().filter(|&&b| b == b'\n').count();
    Some(n + usize::from(bytes.last().is_some_and(|&b| b != b'\n')))
}

//! TREE-3, TREE-4, TREE-5, FIND-1
//! The tree of the project (pure): entries in a stable order, expansion, and the visible rows.
//! The data side only: keys, the panel and the background reads belong to the event loop.
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One entry of a directory listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

/// One visible row of the panel. `path` is relative to the root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub depth: usize,
    pub name: String,
    pub is_dir: bool,
    pub open: bool,
    pub path: PathBuf,
}

struct Node {
    name: String,
    is_dir: bool,
    parent: usize,
    /// Child ids in display order; `None` until the listing has arrived.
    kids: Option<Vec<usize>>,
    open: bool,
    /// The rows of its visible subtree, itself included: 1 unless it is open and loaded. Every
    /// ancestor's count includes it, so a window of rows is found by skipping whole subtrees
    /// instead of walking them.
    rows: usize,
}

/// Node 0 is the root: always open, never a row itself. Nodes are never freed.
pub struct Tree {
    nodes: Vec<Node>,
}

impl Node {
    fn new(name: String, is_dir: bool, parent: usize, open: bool) -> Node {
        Node { name, is_dir, parent, kids: None, open, rows: 1 }
    }
}

impl Default for Tree {
    fn default() -> Tree {
        Tree { nodes: vec![Node::new(String::new(), true, 0, true)] }
    }
}

/// One token of a name: a run of digits (it sorts where `0` would, by value) or one lowercased
/// char; and how many bytes it takes. Comparing token by token keeps the order transitive.
fn token(s: &str) -> Option<((char, usize, &str), usize)> {
    let c = s.chars().next()?;
    let n = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if n == 0 {
        return Some(((c.to_lowercase().next().unwrap_or(c), 0, ""), c.len_utf8()));
    }
    let value = s[..n].trim_start_matches('0');
    Some((('0', value.len(), value), n))
}

/// Case-insensitive, with runs of digits compared as numbers: `a2` < `a10` < `B`.
fn natural(mut a: &str, mut b: &str) -> Ordering {
    loop {
        match (token(a), token(b)) {
            (Some((x, i)), Some((y, j))) if x == y => (a, b) = (&a[i..], &b[j..]),
            (x, y) => return x.map(|t| t.0).cmp(&y.map(|t| t.0)),
        }
    }
}

/// TREE-3: directories first, then the natural order of the names, then the bytes (so that
/// `a01` and `a1`, or `A` and `a`, still have one fixed order).
fn order((a, a_dir): (&str, bool), (b, b_dir): (&str, bool)) -> Ordering {
    b_dir.cmp(&a_dir).then_with(|| natural(a, b)).then_with(|| a.cmp(b))
}

/// The order of the rows for two paths (relative, `/`-separated): every component but the last
/// is a directory, and `a_dir` / `b_dir` say what the last one is. A directory comes before
/// what is inside it (FIND-1: `n` and `N` walk the matches in the order they are on screen).
pub fn cmp_paths(a: &str, a_dir: bool, b: &str, b_dir: bool) -> Ordering {
    let (mut x, mut y) = (a.split('/').peekable(), b.split('/').peekable());
    loop {
        match (x.next(), y.next()) {
            (Some(p), Some(q)) => {
                let o = order((p, x.peek().is_some() || a_dir), (q, y.peek().is_some() || b_dir));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (p, q) => return p.is_some().cmp(&q.is_some()),
        }
    }
}

impl Tree {
    fn kids(&self, i: usize) -> &[usize] {
        self.nodes[i].kids.as_deref().unwrap_or(&[])
    }

    /// The node at a path relative to the root (`a/b`, no `.` or `..`).
    fn find(&self, path: &Path) -> Option<usize> {
        path.iter().try_fold(0, |i, n| {
            self.kids(i).iter().copied().find(|&k| n == self.nodes[k].name.as_str())
        })
    }

    /// Recomputes the row count of node `i` from its children and passes the difference up
    /// through the ancestors that are open (a closed one hides it, and counts 1 anyway).
    fn refresh(&mut self, i: usize) {
        let n = &self.nodes[i];
        let below: usize =
            if n.open { self.kids(i).iter().map(|&k| self.nodes[k].rows).sum() } else { 0 };
        let delta = 1 + below as isize - n.rows as isize;
        self.nodes[i].rows = 1 + below;
        let mut j = i;
        while j != 0 {
            j = self.nodes[j].parent;
            if !self.nodes[j].open {
                break;
            }
            self.nodes[j].rows = (self.nodes[j].rows as isize + delta) as usize;
        }
    }

    /// TREE-5: stores the listing of `dir` (relative to the root, empty for the root) when it
    /// arrives, whether or not `dir` is still expanded. `.git` is dropped (TREE-3). Entries
    /// already known keep their expansion and their own listing, so a second listing refreshes.
    /// False if `dir` is not a directory of the tree.
    pub fn insert_listing(&mut self, dir: &Path, mut entries: Vec<Entry>) -> bool {
        let Some(d) = self.find(dir).filter(|&d| self.nodes[d].is_dir) else { return false };
        entries.retain(|e| e.name != ".git");
        entries.sort_by(|a, b| order((&a.name, a.is_dir), (&b.name, b.is_dir)));
        let mut old: HashMap<String, usize> =
            self.kids(d).iter().map(|&k| (self.nodes[k].name.clone(), k)).collect();
        let mut kids = Vec::with_capacity(entries.len());
        for e in entries {
            let known = old.remove(&e.name).filter(|&k| self.nodes[k].is_dir == e.is_dir);
            kids.push(known.unwrap_or_else(|| {
                self.nodes.push(Node::new(e.name, e.is_dir, d, false));
                self.nodes.len() - 1
            }));
        }
        self.nodes[d].kids = Some(kids);
        self.refresh(d);
        true
    }

    /// Opens a directory. True if its listing has not arrived yet and must be read (TREE-5).
    pub fn expand(&mut self, path: &Path) -> bool {
        let Some(i) = self.find(path).filter(|&i| self.nodes[i].is_dir) else { return false };
        self.nodes[i].open = true;
        self.refresh(i);
        self.nodes[i].kids.is_none()
    }

    pub fn collapse(&mut self, path: &Path) {
        if let Some(i) = self.find(path).filter(|&i| i != 0) {
            self.nodes[i].open = false;
            self.refresh(i);
        }
    }

    /// TREE-4 `-`: only the root stays open. Listings that have arrived are kept.
    pub fn collapse_all(&mut self) {
        for n in self.nodes.iter_mut().skip(1) {
            (n.open, n.rows) = (false, 1);
        }
        self.refresh(0);
    }

    /// The directories that are open (a closed directory above one does not close it).
    pub fn open_dirs(&self) -> HashSet<PathBuf> {
        let open = (1..self.nodes.len()).filter(|&i| self.nodes[i].open && self.nodes[i].is_dir);
        open.map(|mut i| {
            let mut parts = vec![];
            while i != 0 {
                parts.push(self.nodes[i].name.as_str());
                i = self.nodes[i].parent;
            }
            parts.iter().rev().collect()
        })
        .collect()
    }

    /// FIND-1: closes every directory that is open and not in `keep`, so that the expansion is
    /// what it was when `open_dirs` gave `keep`. Listings that arrived are kept.
    pub fn restore(&mut self, keep: &HashSet<PathBuf>) {
        for dir in self.open_dirs().difference(keep) {
            self.collapse(dir);
        }
    }

    /// How many rows are visible.
    pub fn visible(&self) -> usize {
        self.nodes[0].rows - 1
    }

    /// The rows `start..start + n` (fewer at the end). The work is proportional to the window
    /// and to the siblings along the way to it, never to the size of the tree.
    pub fn rows(&self, start: usize, n: usize) -> Vec<Row> {
        let mut w = Window { tree: self, skip: start, left: n, path: PathBuf::new(), rows: vec![] };
        w.walk(0, 0);
        w.rows
    }
    /// The row a path is shown on, if it is visible (every directory above it is open).
    pub fn row_of(&self, path: &Path) -> Option<usize> {
        let mut i = self.find(path).filter(|&i| i != 0)?;
        let mut row = 0;
        loop {
            let p = self.nodes[i].parent;
            let earlier = self.kids(p).iter().take_while(|&&k| k != i);
            row += earlier.map(|&k| self.nodes[k].rows).sum::<usize>();
            if p == 0 {
                return Some(row);
            }
            if !self.nodes[p].open {
                return None;
            }
            (row, i) = (row + 1, p); // the row of the directory itself comes first
        }
    }

    /// TREE-4 `h`: the row of the directory that holds the entry on `row`, none at the top.
    pub fn parent_row(&self, row: usize) -> Option<usize> {
        let parent = self.rows(row, 1).pop()?.path.parent()?.to_path_buf();
        self.row_of(&parent)
    }
}

/// A window of rows being collected: `skip` rows to pass, then `left` to take.
struct Window<'a> {
    tree: &'a Tree,
    skip: usize,
    left: usize,
    path: PathBuf,
    rows: Vec<Row>,
}

impl Window<'_> {
    fn walk(&mut self, i: usize, depth: usize) {
        for &k in self.tree.kids(i) {
            let node = &self.tree.nodes[k];
            if self.left == 0 {
                return;
            }
            if self.skip >= node.rows {
                self.skip -= node.rows; // the whole subtree is above the window
                continue;
            }
            self.path.push(&node.name);
            if self.skip > 0 {
                self.skip -= 1;
            } else {
                let (name, path) = (node.name.clone(), self.path.clone());
                self.rows.push(Row { depth, name, is_dir: node.is_dir, open: node.open, path });
                self.left -= 1;
            }
            if node.rows > 1 {
                self.walk(k, depth + 1);
            }
            self.path.pop();
        }
    }
}

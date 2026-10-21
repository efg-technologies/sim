//! TREE-3, TREE-4, TREE-5: the order of the tree never changes, expanding and collapsing never
//! moves what is above, and the visible rows are what a plain recursive walk gives.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
use proptest::prelude::*;
use sim::tree::{Entry, Row, Tree};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn d(name: &str) -> Entry {
    Entry { name: name.into(), is_dir: true, link: false }
}

fn f(name: &str) -> Entry {
    Entry { name: name.into(), is_dir: false, link: false }
}

fn names(t: &Tree) -> Vec<String> {
    t.rows(0, t.visible()).into_iter().map(|r| r.name).collect()
}

fn paths(t: &Tree) -> Vec<PathBuf> {
    t.rows(0, t.visible()).into_iter().map(|r| r.path).collect()
}

fn with_root(entries: Vec<Entry>) -> Tree {
    let mut t = Tree::default();
    assert!(t.insert_listing(Path::new(""), entries));
    t
}

#[test]
fn tree_3_directories_first_then_names_without_regard_to_case() {
    let t = with_root(vec![f("b.rs"), d("src"), f("A.md"), d("Docs"), f("a.txt"), d("bin")]);
    assert_eq!(names(&t), ["bin", "Docs", "src", "A.md", "a.txt", "b.rs"]);
}

#[test]
fn tree_3_runs_of_digits_compare_as_numbers() {
    let t = with_root(vec![
        f("a10"),
        f("a2"),
        f("a1"),
        f("a01"),
        f("a1b10"),
        f("a1b9"),
        f("a"),
        f("b"),
    ]);
    // "a01" and "a1" are the same number; the bytes decide, once and for all.
    assert_eq!(names(&t), ["a", "a01", "a1", "a1b9", "a1b10", "a2", "a10", "b"]);
    // A number is not a letter: it sorts where 0 would, before letters and after `-`.
    let t = with_root(vec![f("x-1"), f("x1"), f("xa"), f("x0"), f("x00"), f("x9")]);
    assert_eq!(names(&t), ["x-1", "x0", "x00", "x1", "x9", "xa"]);
    // Numbers longer than any integer type still compare by value.
    let t = with_root(vec![f("n99999999999999999999999"), f("n100000000000000000000000")]);
    assert_eq!(names(&t), ["n99999999999999999999999", "n100000000000000000000000"]);
}

#[test]
fn tree_3_git_is_never_shown_but_dotfiles_are() {
    let mut t = with_root(vec![d(".git"), f(".gitignore"), f(".env"), d(".github")]);
    assert_eq!(names(&t), [".github", ".env", ".gitignore"]);
    // A worktree has a `.git` file instead of a directory.
    assert!(t.insert_listing(Path::new(""), vec![f(".git"), f("a")]));
    assert_eq!(names(&t), ["a"]);
}

#[test]
fn tree_3_expansion_stays_as_you_left_it() {
    let mut t = with_root(vec![d("a"), d("b"), f("z")]);
    t.insert_listing(Path::new("a"), vec![d("x"), f("y")]);
    t.insert_listing(Path::new("a/x"), vec![f("deep")]);
    for p in ["a", "a/x"] {
        assert!(!t.expand(Path::new(p)));
    }
    assert_eq!(names(&t), ["a", "x", "deep", "y", "b", "z"]);
    t.collapse(Path::new("a"));
    assert_eq!(names(&t), ["a", "b", "z"]);
    // `a/x` is still open inside the collapsed `a`.
    t.expand(Path::new("a"));
    assert_eq!(names(&t), ["a", "x", "deep", "y", "b", "z"]);
    let rows = t.rows(0, 99);
    assert_eq!(rows.iter().map(|r| r.depth).collect::<Vec<_>>(), [0, 1, 2, 1, 0, 0]);
    assert!(rows[0].open && rows[1].open && !rows[3].open && !rows[4].open);
}

#[test]
fn tree_3_rows_carry_the_path_below_the_root() {
    let mut t = with_root(vec![d("src")]);
    t.insert_listing(Path::new("src"), vec![d("core"), f("main.rs")]);
    t.expand(Path::new("src"));
    let r: Vec<&Path> = vec![Path::new("src"), Path::new("src/core"), Path::new("src/main.rs")];
    assert_eq!(paths(&t).iter().map(PathBuf::as_path).collect::<Vec<_>>(), r);
    assert_eq!(
        t.rows(1, 1)[0],
        Row { depth: 1, name: "core".into(), is_dir: true, open: false, path: "src/core".into() }
    );
}

#[test]
fn tree_3_a_second_listing_refreshes_without_losing_state() {
    let mut t = with_root(vec![d("a"), d("c"), f("m")]);
    t.insert_listing(Path::new("a"), vec![f("one")]);
    t.expand(Path::new("a"));
    // `b` appears between the two directories; `a` keeps its expansion and its listing, `m` goes.
    t.insert_listing(Path::new(""), vec![d("a"), d("b"), d("c"), f("n")]);
    assert_eq!(names(&t), ["a", "one", "b", "c", "n"]);
    // A name that changes kind is a new entry: its old listing is gone.
    t.insert_listing(Path::new(""), vec![f("a"), d("b"), d("c"), f("n")]);
    assert_eq!(names(&t), ["b", "c", "a", "n"]);
}

#[test]
fn tree_3_unknown_or_non_directory_paths_are_refused() {
    let mut t = with_root(vec![d("a"), f("file")]);
    assert!(!t.insert_listing(Path::new("nope"), vec![f("x")]));
    assert!(!t.insert_listing(Path::new("file"), vec![f("x")]));
    assert!(!t.insert_listing(Path::new("a/deeper"), vec![f("x")]));
    assert!(!t.expand(Path::new("file")));
    assert!(!t.expand(Path::new("nope")));
    t.collapse(Path::new("nope"));
    assert_eq!(names(&t), ["a", "file"]);
}

#[test]
fn tree_3_the_root_cannot_be_collapsed() {
    let mut t = with_root(vec![f("a")]);
    t.collapse(Path::new(""));
    t.collapse_all();
    assert_eq!(names(&t), ["a"]);
}

#[test]
fn tree_3_a_big_directory_is_read_through_a_window() {
    let mut t = with_root(vec![d("big")]);
    t.insert_listing(Path::new("big"), (0..100_000).map(|i| f(&format!("f{i}"))).collect());
    t.expand(Path::new("big"));
    assert_eq!(t.visible(), 100_001);
    let w = t.rows(99_990, 20);
    assert_eq!(w.len(), 11);
    assert_eq!(w[0].name, "f99989");
    assert_eq!(w[10].name, "f99999");
    assert_eq!(t.row_of(Path::new("big/f99999")), Some(100_000));
    assert!(t.rows(200_000, 5).is_empty());
    assert!(t.rows(0, 0).is_empty());
}

#[test]
fn tree_4_row_of_and_parent_row() {
    let mut t = with_root(vec![d("a"), d("b")]);
    t.insert_listing(Path::new("a"), vec![d("x"), f("y")]);
    t.insert_listing(Path::new("a/x"), vec![f("deep")]);
    t.expand(Path::new("a"));
    assert_eq!(names(&t), ["a", "x", "y", "b"]);
    let row = |t: &Tree, p: &str| t.row_of(Path::new(p));
    let rows = |t: &Tree| ["a", "a/x", "a/y", "b"].map(|p| row(t, p));
    assert_eq!(rows(&t), [Some(0), Some(1), Some(2), Some(3)]);
    // `a/x/deep` is in the tree but hidden by the closed `a/x`; the root and strangers have no row.
    assert_eq!((row(&t, "a/x/deep"), row(&t, ""), row(&t, "nope")), (None, None, None));
    assert_eq!((t.parent_row(1), t.parent_row(2)), (Some(0), Some(0)));
    assert_eq!((t.parent_row(0), t.parent_row(3), t.parent_row(99)), (None, None, None));
    t.expand(Path::new("a/x"));
    assert_eq!((row(&t, "a/x/deep"), t.parent_row(2), row(&t, "b")), (Some(2), Some(1), Some(4)));
}

#[test]
fn tree_4_expand_asks_for_a_listing_only_when_it_is_missing() {
    let mut t = with_root(vec![d("a"), d("b")]);
    assert!(t.expand(Path::new("a")), "nothing was read yet");
    assert_eq!(names(&t), ["a", "b"], "an open directory with no listing shows nothing below it");
    t.insert_listing(Path::new("b"), vec![f("x")]);
    assert!(!t.expand(Path::new("b")), "the listing came before the expansion");
    assert_eq!(names(&t), ["a", "b", "x"]);
}

#[test]
fn tree_4_collapse_all_keeps_the_listings() {
    let mut t = with_root(vec![d("a"), d("b")]);
    t.insert_listing(Path::new("a"), vec![d("x")]);
    t.insert_listing(Path::new("a/x"), vec![f("deep")]);
    t.expand(Path::new("a"));
    t.expand(Path::new("a/x"));
    t.expand(Path::new("b"));
    assert_eq!(t.visible(), 4); // a, x, deep, b (`b` has no listing yet)
    t.collapse_all();
    assert_eq!((t.visible(), names(&t)), (2, vec!["a".to_string(), "b".into()]));
    assert!(!t.expand(Path::new("a")), "its listing is still there");
    assert_eq!(names(&t), ["a", "x", "b"]);
}

#[test]
fn tree_5_a_listing_that_arrives_late_moves_nothing_above_it() {
    let mut t = with_root(vec![d("a"), d("b"), d("c"), f("z")]);
    assert!(t.expand(Path::new("b")));
    let before = paths(&t);
    // The listing of `b` arrives: its entries appear below `b`, and `a` and `b` stay put.
    t.insert_listing(Path::new("b"), vec![f("1"), d("2")]);
    let after = paths(&t);
    assert_eq!(&after[..2], &before[..2]);
    assert_eq!(after.len(), before.len() + 2);
    assert_eq!(names(&t), ["a", "b", "2", "1", "c", "z"]);
    // The listing of a directory that was collapsed meanwhile is stored, and shows nothing.
    t.collapse(Path::new("c"));
    let before = paths(&t);
    t.insert_listing(Path::new("c"), vec![f("late")]);
    assert_eq!(paths(&t), before);
    assert!(!t.expand(Path::new("c")));
    assert_eq!(names(&t), ["a", "b", "2", "1", "c", "late", "z"]);
}

// ---- properties -------------------------------------------------------------------------

/// What the tree must show, computed the slow and obvious way.
struct Model {
    /// The listing every directory has on disk, `.git` included.
    fs: BTreeMap<PathBuf, Vec<Entry>>,
    loaded: BTreeSet<PathBuf>,
    open: BTreeSet<PathBuf>,
}

/// The key of the order in words: a name is a list of tokens, each a lowercased char or a number.
fn key(name: &str) -> Vec<(char, usize, String)> {
    let mut out = vec![];
    let mut digits = String::new();
    for c in name.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if !digits.is_empty() {
            let v = digits.trim_start_matches('0').to_string();
            out.push(('0', v.len(), v));
            digits.clear();
        }
        out.push((c.to_lowercase().next().unwrap(), 0, String::new()));
    }
    out.pop(); // the sentinel
    out
}

impl Model {
    /// Whether the tree has heard of this directory: its parent's listing has arrived and names it.
    fn known(&self, dir: &Path) -> bool {
        let Some(parent) = dir.parent() else { return true };
        let name = dir.file_name().and_then(|n| n.to_str());
        self.loaded.contains(parent)
            && self.listing(parent).iter().any(|e| e.is_dir && Some(e.name.as_str()) == name)
    }

    fn listing(&self, dir: &Path) -> Vec<Entry> {
        let mut v: Vec<Entry> = self.fs[dir].iter().filter(|e| e.name != ".git").cloned().collect();
        v.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| key(&a.name).cmp(&key(&b.name)))
                .then_with(|| a.name.cmp(&b.name))
        });
        v
    }

    fn walk(&self, dir: &Path, depth: usize, out: &mut Vec<(usize, PathBuf, bool)>) {
        if !self.loaded.contains(dir) {
            return;
        }
        for e in self.listing(dir) {
            let p = dir.join(&e.name);
            out.push((depth, p.clone(), e.is_dir));
            if e.is_dir && self.open.contains(&p) {
                self.walk(&p, depth + 1, out);
            }
        }
    }

    fn rows(&self) -> Vec<(usize, PathBuf, bool)> {
        let mut out = vec![];
        self.walk(Path::new(""), 0, &mut out);
        out
    }
}

#[derive(Clone, Debug)]
enum Op {
    Listing(usize),
    Expand(usize),
    Collapse(usize),
    CollapseAll,
}

const NAMES: [&str; 11] = ["a", "B", "b", "c1", "c10", "c2", "C2", ".x", ".git", "d", "D"];

/// A random file system: paths of one to four names; a name that something lies below is a directory.
fn file_system() -> impl Strategy<Value = BTreeMap<PathBuf, Vec<Entry>>> {
    proptest::collection::vec(proptest::collection::vec(0..NAMES.len(), 1..5), 1..40).prop_map(
        |paths| {
            let mut dirs: BTreeSet<PathBuf> = BTreeSet::from([PathBuf::new()]);
            let mut all: BTreeSet<PathBuf> = BTreeSet::new();
            for p in paths {
                let p: PathBuf = p.into_iter().map(|i| NAMES[i]).collect();
                dirs.extend(p.ancestors().skip(1).map(Path::to_path_buf));
                all.insert(p);
            }
            let mut fs: BTreeMap<PathBuf, Vec<Entry>> =
                dirs.iter().map(|d| (d.clone(), vec![])).collect();
            for p in dirs.iter().chain(all.iter()).filter(|p| !p.as_os_str().is_empty()) {
                if let Some(list) = fs.get_mut(p.parent().unwrap()) {
                    let e = Entry {
                        name: p.file_name().unwrap().to_str().unwrap().into(),
                        is_dir: dirs.contains(p),
                        link: false,
                    };
                    if !list.contains(&e) {
                        list.push(e);
                    }
                }
            }
            fs
        },
    )
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    let op = prop_oneof![
        4 => (0..99usize).prop_map(Op::Listing),
        4 => (0..99usize).prop_map(Op::Expand),
        2 => (0..99usize).prop_map(Op::Collapse),
        1 => Just(Op::CollapseAll),
    ];
    proptest::collection::vec(op, 1..60)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tree_3_order_is_stable_and_rows_equal_a_reference_walk(fs in file_system(), ops in ops(), win in (0..30usize, 0..30usize)) {
        let dirs: Vec<PathBuf> = fs.keys().cloned().collect();
        let mut m = Model { fs, loaded: BTreeSet::new(), open: BTreeSet::new() };
        let mut t = Tree::default();
        for op in ops {
            let before = paths(&t);
            let target = match op {
                Op::Listing(i) | Op::Expand(i) | Op::Collapse(i) => Some(dirs[i % dirs.len()].clone()),
                Op::CollapseAll => None,
            };
            let known = target.as_ref().is_some_and(|d| m.known(d));
            match (&op, &target) {
                (Op::Listing(_), Some(d)) => {
                    prop_assert_eq!(t.insert_listing(d, m.fs[d].clone()), known);
                    if known {
                        m.loaded.insert(d.clone());
                    }
                }
                (Op::Expand(_), Some(d)) => {
                    // It asks for the listing exactly when the directory is known and nothing was read.
                    prop_assert_eq!(t.expand(d), known && !m.loaded.contains(d));
                    if known {
                        m.open.insert(d.clone());
                    }
                }
                (Op::Collapse(_), Some(d)) => {
                    t.collapse(d);
                    m.open.remove(d);
                }
                _ => {
                    t.collapse_all();
                    m.open.clear();
                }
            }
            let after = paths(&t);
            // The rows that were there and still are keep their order.
            let kept_before: Vec<_> = before.iter().filter(|p| after.contains(p)).collect();
            let kept_after: Vec<_> = after.iter().filter(|p| before.contains(p)).collect();
            prop_assert_eq!(kept_before, kept_after);
            // Filling, expanding or collapsing a directory leaves its row and all above it as they were.
            if let Some(row) = target.as_ref().and_then(|d| before.iter().position(|p| p == d)) {
                prop_assert_eq!(&after[..=row], &before[..=row]);
            }
            // The rows are what the reference walk gives, whole and through a window.
            let want = m.rows();
            let got = t.rows(0, t.visible());
            prop_assert_eq!(t.visible(), want.len());
            prop_assert_eq!(got.iter().map(|r| (r.depth, r.path.clone(), r.is_dir)).collect::<Vec<_>>(), want);
            for (i, r) in got.iter().enumerate() {
                prop_assert_eq!(r.name.as_str(), r.path.file_name().unwrap().to_str().unwrap());
                prop_assert_eq!(t.row_of(&r.path), Some(i));
                prop_assert_eq!(t.parent_row(i), if r.depth == 0 { None } else { t.row_of(r.path.parent().unwrap()) });
                prop_assert_eq!(r.open, r.is_dir && m.open.contains(&r.path));
            }
            let window: Vec<Row> = got.iter().skip(win.0).take(win.1).cloned().collect();
            prop_assert_eq!(t.rows(win.0, win.1), window);
        }
    }
}

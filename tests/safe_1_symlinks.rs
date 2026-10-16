//! SAFE-1: the walk of a path's symlinks (`file::symlinks`, `file::foreign_link`) against an independent
//! walker written the way the kernel does it, and against `canonicalize`, on random trees of
//! directories and links (loops, dangling links, `..` after links, absolute and relative targets).
#![allow(clippy::unwrap_used)]
mod common;
use common::*;
use proptest::prelude::*;
use si::io::file::{foreign_link, resolve, symlinks};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{MetadataExt, symlink};
use std::path::{Component, Path, PathBuf};

const NAMES: [&str; 6] = ["a", "b", "c", "d", "e", "f"];
/// Directories made first, so that links can live one and two levels down.
const SKELETON: [&str; 6] = ["p", "q", "r", "p/s", "q/s", "r/s"];
/// Link targets, relative to the tree's root `R` (`R/` is replaced by the root).
const TARGETS: [&str; 30] = [
    "a", "b", "c", "d", "../a", "../b", "..", "../..", ".", "a/b", "../p/a", "R/", "R/a", "R/b/c",
    "R/p/s", "zz", "R/zz/x", "a/..", "p/../q", "./c", "R/a/../b", "e/f/..", "p", "q/s", "s",
    "../s", "../q/s/a", "R/r", "s/a", "../../a",
];

#[derive(Clone, Debug)]
enum Make {
    Dir(Vec<usize>),
    Link(Vec<usize>, usize),
}

/// The path of an entry under the root: a skeleton directory (or none) and a name.
fn rel(v: &[usize]) -> PathBuf {
    let dir = if v[0] % 7 == 6 { "" } else { SKELETON[v[0] % 7 % 6] };
    Path::new(dir).join(NAMES[v[1] % NAMES.len()])
}

fn build(root: &Path, plan: &[Make]) {
    for m in plan {
        match m {
            Make::Dir(v) => {
                let p = root.join(rel(v));
                if fs::symlink_metadata(&p).is_err() {
                    let _ = fs::create_dir(&p); // parents may be missing: skip
                }
            }
            Make::Link(v, t) => {
                let p = root.join(rel(v));
                if fs::symlink_metadata(&p).is_err() {
                    let t = TARGETS[t % TARGETS.len()];
                    let t = match t.strip_prefix("R/") {
                        Some(rest) => root.join(rest).to_string_lossy().into_owned(),
                        None if t == "R/" => root.to_string_lossy().into_owned(),
                        None => t.to_string(),
                    };
                    let _ = symlink(t, &p); // parent may be missing: skip
                }
            }
        }
    }
}

/// A walk written the way the kernel does it, independently of `symlinks`: Some(list) or None when
/// more than 40 links are met. Each link as (absolute name, owner).
fn reference(p: &Path) -> (Option<Vec<(PathBuf, u32)>>, PathBuf) {
    let mut todo: VecDeque<OsString> = p.components().map(|c| c.as_os_str().to_owned()).collect();
    let (mut cur, mut found, mut count) = (PathBuf::from("/"), vec![], 0);
    todo.pop_front(); // the root
    while let Some(c) = todo.pop_front() {
        if c == ".." {
            cur.pop();
        } else if c != "." {
            let next = cur.join(&c);
            match fs::symlink_metadata(&next) {
                Ok(m) if m.file_type().is_symlink() => {
                    count += 1;
                    if count > 40 {
                        return (None, cur);
                    }
                    // The path's own final link keeps the name it was given, any other is absolute.
                    let own = found.is_empty() && todo.is_empty();
                    found.push((if own { p.to_path_buf() } else { next.clone() }, m.uid()));
                    let to = fs::read_link(&next).unwrap();
                    if to.is_absolute() {
                        cur = PathBuf::from("/");
                    }
                    let parts: Vec<OsString> = to
                        .components()
                        .filter(|c| !matches!(c, Component::RootDir))
                        .map(|c| c.as_os_str().to_owned())
                        .collect();
                    for part in parts.into_iter().rev() {
                        todo.push_front(part);
                    }
                }
                _ => cur = next,
            }
        }
    }
    (Some(found), cur)
}

fn query() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(
        prop::sample::select(vec![
            "a", "b", "c", "d", "e", "f", "p", "q", "r", "s", "..", ".", "..", "a", "b", "p", "q",
            "s",
        ]),
        1..8,
    )
    .prop_map(|v| v.into_iter().map(String::from).collect())
}

fn plan() -> impl Strategy<Value = Vec<Make>> {
    let path = prop::collection::vec(0usize..7, 2..3);
    prop::collection::vec(
        prop_oneof![
            1 => path.clone().prop_map(Make::Dir),
            6 => (path, 0usize..30).prop_map(|(p, t)| Make::Link(p, t)),
        ],
        4..24,
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(2000), failure_persistence: None, ..ProptestConfig::default() })]

    /// `symlinks` meets exactly the links the kernel does, and `foreign_link` names the first one
    /// that neither the user nor root owns.
    #[test]
    fn safe_1_the_walk_of_a_path_meets_the_links_the_kernel_meets(plan in plan(), qs in prop::collection::vec(query(), 1..8)) {
        let root = temp_dir("hunt-links");
        // The tree sits 12 directories down in a directory of its own, so a path that climbs out
        // of it (a query's 7 `..` and a link's 2 at most) meets only those, never the system's own
        // directories: macOS keeps a `C` beside its temp dir, which its case-insensitive disk takes
        // for `c`.
        let root_path = (0..12).fold(fs::canonicalize(&root.0).unwrap(), |p, i| p.join(format!("u{i}")));
        fs::create_dir_all(&root_path).unwrap();
        // The tree's own base directories, so deeper names exist.
        for n in SKELETON {
            fs::create_dir(root_path.join(n)).unwrap();
        }
        build(&root_path, &plan);
        let me = fs::metadata(&root_path).unwrap().uid();
        for q in qs {
            let p = root_path.join(q.join("/"));
            let (want, end) = reference(&p);
            let got = symlinks(p.to_str().unwrap());
            match &want {
                None => prop_assert_eq!(got.len(), 40, "a loop or more than 40 links ends the list at 40: {:?} in {:?}", p, plan),
                Some(w) => {
                    let w: Vec<(String, u32)> = w.iter().map(|(n, u)| (n.display().to_string(), *u)).collect();
                    prop_assert_eq!(got.len(), w.len(), "{:?} in {:?}: got {:?} want {:?}", p, plan, got, w);
                    for (g, w) in got.iter().zip(&w) {
                        prop_assert_eq!(g.1, w.1);
                        prop_assert_eq!(&g.0, &w.0, "{:?} in {:?}", p, plan);
                    }
                    // The kernel agrees with the reference about where the path ends, and `resolve`
                    // (what the save writes to) with the kernel.
                    if let Ok(c) = fs::canonicalize(&p) {
                        prop_assert_eq!(&c, &end, "{:?} in {:?}", p, plan);
                        prop_assert_eq!(resolve(p.to_str().unwrap()), c, "resolve of {:?} in {:?}", p, plan);
                    }
                    // Every link is mine here, so nobody else's is found; with a pretend user, the first is.
                    prop_assert_eq!(foreign_link(p.to_str().unwrap(), me), None);
                    prop_assert_eq!(foreign_link(p.to_str().unwrap(), me + 1).is_some(), !w.is_empty());
                }
            }
        }
    }
}

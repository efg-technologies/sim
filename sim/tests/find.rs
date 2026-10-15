//! FIND-1, FIND-2, FIND-3: fuzzy matching of relative paths, the ranking, and the speed of a key.
//!
//!   cargo test --release -p sim --test find -- --ignored bench --nocapture
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
use proptest::prelude::*;
use sim::find::Query;
use std::cell::Cell;
use std::time::{Duration, Instant};

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

/// The paths of `list` that match, best first: the best one, then the best of the rest, and so on.
fn ranked_by<'a>(query: &str, list: &'a [String], lik: impl Fn(&str) -> f64) -> Vec<&'a str> {
    let q = Query::new(query);
    let (mut rest, mut out) = (list.to_vec(), vec![]);
    loop {
        let found = q.search(&rest);
        let Some(best) = found.best(&rest, &lik) else { return out };
        let path = rest.remove(best.index);
        out.push(list.iter().find(|p| **p == path).unwrap().as_str());
    }
}

/// The same with no history.
fn ranked<'a>(query: &str, list: &'a [String]) -> Vec<&'a str> {
    ranked_by(query, list, |_| 0.0)
}

/// Where the query matched in `path`, as the chars at the positions.
fn matched(query: &str, path: &str) -> Option<String> {
    let list = vec![path.to_string()];
    let found = Query::new(query).search(&list);
    let hit = found.hits.first()?;
    let chars: Vec<char> = path.chars().collect();
    Some(found.positions(hit).iter().map(|&p| chars[p]).collect())
}

#[test]
fn find_1_the_chars_of_the_query_must_occur_in_order() {
    let list =
        paths(&["src/main.rs", "src/core/app.rs", "README.md", "Cargo.toml", "tests/map.rs"]);
    assert_eq!(ranked("mrs", &list), ["src/main.rs", "tests/map.rs"]);
    assert_eq!(ranked("mp", &list), ["tests/map.rs"]);
    assert_eq!(ranked("sma", &list), ["src/main.rs", "tests/map.rs"]);
    assert!(ranked("zz", &list).is_empty());
    assert!(ranked("xs.m", &list).is_empty(), "order matters");
    assert_eq!(ranked("src/c", &list), ["src/core/app.rs"], "the query may contain the separator");
}

#[test]
fn find_1_an_empty_query_matches_nothing() {
    let list = paths(&["a", ""]);
    assert!(ranked("", &list).is_empty());
    assert!(Query::new("").search(&list).dirs.is_empty());
    assert!(ranked("a", &paths(&[""])).is_empty());
}

#[test]
fn find_1_smartcase() {
    let list = paths(&["Makefile", "makefile", "docs/MAKE.md", "a/MaIn.rs", "a/main.rs"]);
    assert_eq!(ranked("make", &list).len(), 3, "lowercase ignores case");
    assert_eq!(ranked("Make", &list), ["Makefile"], "an uppercase letter makes it exact");
    assert_eq!(ranked("MAKE", &list), ["docs/MAKE.md"]);
    assert_eq!(ranked("aI", &list), ["a/MaIn.rs"], "the whole query is case-sensitive then");
    assert_eq!(ranked("ai", &paths(&["a/MaIn.rs", "a/main.rs"])).len(), 2);
    assert_eq!(ranked("bA", &paths(&["AbA"])), ["AbA"]);
    // Non-ASCII letters fold too.
    assert_eq!(ranked("été", &paths(&["Été.txt", "ete.txt"])), ["Été.txt"]);
}

#[test]
fn find_1_positions_are_char_positions_not_bytes() {
    let path = "日本語/main.rs";
    assert_eq!(matched("日main", path).as_deref(), Some("日main"));
    let list = vec![path.to_string()];
    let found = Query::new("本m").search(&list);
    assert_eq!(found.positions(&found.hits[0]), [1, 4]);
}

#[test]
fn find_1_the_match_is_the_tightest_one_that_ends_first() {
    let at = |q: &str, p: &str| {
        let list = vec![p.to_string()];
        let found = Query::new(q).search(&list);
        found.positions(&found.hits[0]).to_vec()
    };
    // Not the a at 0: the a nearest to the b that ends the match.
    assert_eq!(at("ab", "a_a_b"), [2, 4]);
    assert_eq!(at("ab", "xaxxab_b"), [4, 5]);
    // The file name is tried too: `ab` is contiguous there, scattered over the directory names.
    assert_eq!(at("ab", "a_b/ab.rs"), [4, 5]);
}

#[test]
fn find_1_directories_leading_to_matches_are_collected() {
    let list = paths(&["a/b/c/target.rs", "a/b/other.rs", "x/target.rs", "top.rs", "q/z/none.rs"]);
    let found = Query::new("tar").search(&list);
    let mut dirs: Vec<_> = found.dirs.iter().map(String::as_str).collect();
    dirs.sort_unstable();
    assert_eq!(dirs, ["a", "a/b", "a/b/c", "x"]);
    assert_eq!(
        found.hits.iter().map(|h| h.index).collect::<Vec<_>>(),
        [0, 2],
        "hits keep the list's order"
    );
    // A match inside a directory name counts for that file as well.
    let found = Query::new("zon").search(&list);
    assert_eq!(
        found.hits.iter().map(|h| list[h.index].as_str()).collect::<Vec<_>>(),
        ["q/z/none.rs"]
    );
    assert_eq!(found.dirs.len(), 2);
}

#[test]
fn find_2_a_run_of_chars_beats_scattered_ones() {
    let list = paths(&["xaxbxcx", "xabcxxx"]);
    assert_eq!(ranked("abc", &list), ["xabcxxx", "xaxbxcx"]);
}

#[test]
fn find_2_a_word_start_beats_the_middle_of_a_word() {
    let list = paths(&["src/domain.rs", "src/main.rs"]);
    assert_eq!(ranked("main", &list), ["src/main.rs", "src/domain.rs"]);
    let list = paths(&["src/pandemic", "src/helper_dir", "src/xHelperDir"]);
    assert_eq!(
        ranked("hd", &list),
        ["src/helper_dir", "src/xHelperDir"],
        "after `_` and at a hump"
    );
}

#[test]
fn find_2_the_file_name_beats_a_directory() {
    // Both are the same match by every other measure (and `tree/z/q.rs` is first by path).
    let list = paths(&["tree/z/q.rs", "zzz/tree.rs"]);
    assert_eq!(ranked("tree", &list), ["zzz/tree.rs", "tree/z/q.rs"]);
}

#[test]
fn find_2_a_shorter_path_wins_when_the_match_is_the_same() {
    let list = paths(&["a/b/c/d/e/f/g/h/lib.rs", "x/lib.rs", "lib.rs"]);
    assert_eq!(ranked("lib", &list), ["lib.rs", "x/lib.rs", "a/b/c/d/e/f/g/h/lib.rs"]);
}

#[test]
fn find_2_likelihood_breaks_ties_and_nothing_else() {
    let list = paths(&["b/x.rs", "a/x.rs", "c/x.rs"]);
    let lik = |p: &str| {
        if p == "c/x.rs" {
            2.0
        } else if p == "b/x.rs" {
            1.0
        } else {
            0.0
        }
    };
    assert_eq!(ranked_by("x.rs", &list, lik), ["c/x.rs", "b/x.rs", "a/x.rs"]);
    // With no history the path decides, so the order never depends on the list's.
    assert_eq!(ranked("x.rs", &list), ["a/x.rs", "b/x.rs", "c/x.rs"]);
    // A better match is not overtaken by a likelier file, and the likelihood is not even asked for.
    let list = paths(&["z/q/xx.rs", "x.rs"]);
    let asked = Cell::new(0);
    let found = Query::new("x.rs").search(&list);
    let best = found.best(&list, |_| {
        asked.set(asked.get() + 1);
        9.0
    });
    assert_eq!(best.map(|h| list[h.index].as_str()), Some("x.rs"));
    assert_eq!(asked.get(), 0);
    assert!(Query::new("x.rs").search(&paths(&["nope"])).best(&list, |_| 0.0).is_none());
}

proptest! {
    #[test]
    fn find_1_matches_are_exactly_the_subsequences(
        query in "[a-cA]{1,4}",
        list in proptest::collection::vec("[a-cA/._é日]{0,12}", 0..30),
    ) {
        let found = Query::new(&query).search(&list);
        let exact = query.chars().any(char::is_uppercase);
        let fold = |c: char| if exact { c } else { c.to_lowercase().next().unwrap() };
        let is_subsequence = |path: &str| {
            let mut q = query.chars().map(fold);
            let mut want = q.next();
            for c in path.chars() {
                if want == Some(fold(c)) { want = q.next(); }
            }
            want.is_none()
        };
        let want: Vec<usize> = (0..list.len()).filter(|&i| is_subsequence(&list[i])).collect();
        prop_assert_eq!(found.hits.iter().map(|h| h.index).collect::<Vec<_>>(), want);
        for h in &found.hits {
            let chars: Vec<char> = list[h.index].chars().collect();
            let pos = found.positions(h);
            prop_assert_eq!(pos.len(), query.chars().count());
            prop_assert!(pos.windows(2).all(|w| w[0] < w[1]) && pos.iter().all(|&p| p < chars.len()));
            prop_assert_eq!(pos.iter().map(|&p| fold(chars[p])).collect::<String>(), query.chars().map(fold).collect::<String>());
        }
    }

    #[test]
    fn find_1_every_directory_above_a_hit_is_listed_and_no_other(
        list in proptest::collection::vec("([a-c]/){0,3}[a-c]{1,2}", 0..30),
        query in "[a-c]{1,2}",
    ) {
        let found = Query::new(&query).search(&list);
        let mut want = std::collections::BTreeSet::new();
        for h in &found.hits {
            let mut dir = list[h.index].as_str();
            while let Some((up, _)) = dir.rsplit_once('/') {
                want.insert(up.to_string());
                dir = up;
            }
        }
        prop_assert_eq!(found.dirs.iter().cloned().collect::<std::collections::BTreeSet<_>>(), want);
    }

    #[test]
    fn find_2_the_order_does_not_depend_on_the_order_of_the_list(
        list in proptest::collection::vec("[a-c/]{1,8}", 1..20),
        query in "[a-c]{1,2}",
    ) {
        let mut list = list;
        list.sort();
        list.dedup();
        let mut reversed = list.clone();
        reversed.reverse();
        prop_assert_eq!(ranked(&query, &list), ranked(&query, &reversed));
    }
}

// ---- FIND-3 -----------------------------------------------------------------------------

/// Paths that look like a big repository's: `per_dir` files to a directory, 2 to 5 levels deep,
/// about 40 chars, sorted.
fn synthetic(n: usize, per_dir: usize) -> Vec<String> {
    const WORDS: [&str; 16] = [
        "core", "view", "buffer", "parser", "render", "index", "server", "client", "config",
        "utils", "model", "handler", "store", "layout", "theme", "events",
    ];
    const EXT: [&str; 6] = ["rs", "md", "toml", "json", "ts", "py"];
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let dirs: Vec<String> = (0..n.div_ceil(per_dir))
        .map(|_| {
            let depth = 2 + (next() % 4) as usize;
            let words: Vec<&str> = (0..depth).map(|_| WORDS[(next() % 16) as usize]).collect();
            words.join(if next() % 3 == 0 { "_v2/" } else { "/" })
        })
        .collect();
    let mut paths: Vec<String> = (0..n)
        .map(|i| {
            let dir = &dirs[(next() % dirs.len() as u64) as usize];
            format!("{dir}/{}_{i}.{}", WORDS[(next() % 16) as usize], EXT[(next() % 6) as usize])
        })
        .collect();
    paths.sort_unstable(); // as `git ls-files` lists them
    paths.iter().map(|p| p.to_string()).collect() // and allocated in that order, as when parsed from its output
}

#[test]
fn find_3_a_key_on_the_synthetic_list_finds_what_it_should() {
    let list = synthetic(2000, 10);
    let found = Query::new("clientrs").search(&list);
    assert!(!found.hits.is_empty() && found.hits.len() < list.len());
    let best = found.best(&list, |_| 0.0).unwrap();
    assert!(list[best.index].contains("client"));
}

/// One key of the search: the pass over every path, the directories, and the best hit (with a
/// history of 200 files, asked on ties). Typing `srv`, `clientrs`, `utl` and a query that matches
/// nothing, on a repository-like list (ten files to a directory) and on the worst list for the
/// directory set (a directory of its own for every file).
#[test]
#[ignore = "benchmark: run with --release"]
fn find_3_bench_one_key_over_100000_paths() {
    let mut worst = Duration::ZERO;
    for per_dir in [10, 1] {
        let list = synthetic(100_000, per_dir);
        let total: usize = list.iter().map(String::len).sum();
        let history: std::collections::HashMap<&str, f64> =
            list.iter().step_by(500).map(|p| (p.as_str(), 1.0)).collect();
        println!(
            "{} paths, {per_dir} to a directory, {} chars on average",
            list.len(),
            total / list.len()
        );
        let queries =
            ["s", "sr", "srv", "c", "cl", "client", "clientrs", "u", "ut", "utl", "q", "zzzz"];
        for query in queries {
            let q = Query::new(query);
            let (mut best, mut hits) = (Duration::MAX, 0);
            for _ in 0..5 {
                let t = Instant::now();
                let found = q.search(&list);
                let top = found.best(&list, |p| history.get(p).copied().unwrap_or(0.0));
                best = best.min(t.elapsed());
                hits = found.hits.len();
                assert_eq!(top.is_some(), hits > 0);
            }
            println!("{query:>9}: {best:>9.2?} {hits:>6} hits");
            worst = worst.max(best);
        }
    }
    println!("worst key: {worst:.2?}");
    assert!(worst <= Duration::from_millis(16), "{worst:?}");
}

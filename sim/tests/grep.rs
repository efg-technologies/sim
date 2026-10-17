//! GREP-2, GREP-3, GREP-5, GREP-7, GREP-8: the pure parts of the content search (the literal
//! smart-case query, the lines it finds, the rows of the panel, rg's output), and the search itself
//! over real directories, by the built-in reader and by rg.
//!
//!   cargo test --release -p sim --test grep -- --ignored bench --nocapture
#![allow(clippy::unwrap_used, clippy::single_range_in_vec_init)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::grep::{self, Batch, Group, Line, MAX_LINES, Query, Request, is_binary, parse_rg};
use sim::io;
use sim::sim::SimEvent;
use std::ffi::OsString;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn q(s: &str) -> Query {
    Query::new(s).unwrap()
}

fn marked(query: &str, line: &str) -> Vec<String> {
    let cs: Vec<char> = line.chars().collect();
    q(query).ranges(line).iter().map(|r| cs[r.clone()].iter().collect()).collect()
}

#[test]
fn grep_2_the_query_is_a_literal_not_a_regex() {
    assert_eq!(q("a.c").ranges("abc a.c"), [4..7]);
    assert_eq!(marked("(x)", "f(x) g(x)"), ["(x)", "(x)"]);
    assert_eq!(marked("a|b", "a b a|b"), ["a|b"]);
    assert_eq!(marked("[0-9]+", "x 12 [0-9]+"), ["[0-9]+"]);
    assert_eq!(marked("\\d", "1 \\d"), ["\\d"]);
}

#[test]
fn grep_2_an_empty_query_searches_nothing() {
    assert!(Query::new("").is_none());
}

#[test]
fn grep_2_smart_case_one_uppercase_letter_makes_the_query_exact() {
    assert_eq!(marked("needle", "Needle NEEDLE needle"), ["Needle", "NEEDLE", "needle"]);
    assert_eq!(marked("Needle", "Needle NEEDLE needle"), ["Needle"]);
    assert_eq!(marked("nEEdle", "Needle NEEDLE nEEdle"), ["nEEdle"]);
    assert_eq!(marked("é", "É é"), ["É", "é"], "non-ASCII letters fold too");
    assert_eq!(marked("日本", "日本語 日本"), ["日本", "日本"]);
}

#[test]
fn grep_2_every_occurrence_is_marked_in_chars_left_to_right_without_overlap() {
    assert_eq!(q("aa").ranges("aaaaa"), [0..2, 2..4]);
    assert_eq!(q("日本").ranges("x日本語日本"), [1..3, 4..6], "chars, not bytes");
    assert_eq!(q("ab").ranges("ab ab"), [0..2, 3..5]);
    assert!(q("ab").ranges("a b").is_empty());
}

#[test]
fn grep_2_a_line_matches_once_and_lines_are_numbered_from_one() {
    let text = "none\nfoo and foo\n\nfoo\nlast foo";
    let lines = q("foo").lines(text, 100);
    assert_eq!(lines.iter().map(|l| l.no).collect::<Vec<_>>(), [2, 4, 5]);
    assert_eq!(lines[0].marks, [0..3, 8..11], "both are marked, the line is one result");
    assert_eq!(lines[2].text, "last foo");
}

#[test]
fn grep_2_the_fast_and_the_slow_paths_agree() {
    // an ASCII query is looked for in the whole text; a query with a non-ASCII letter that has a
    // case is looked for line by line. Both must give the same answer on text that has both.
    let text = "Alpha café\nbeta CAFÉ\n\n  gamma\tcafé  \r\ncafe";
    for query in ["café", "caf", "a", "É", "gamma", "e"] {
        let fast = q(query).lines(text, 100);
        let by_line: Vec<Line> = text
            .split('\n')
            .enumerate()
            .map(|(i, l)| q(query).line(i + 1, l))
            .filter(|l| !l.marks.is_empty())
            .collect();
        assert_eq!(fast, by_line, "query {query:?}");
    }
}

#[test]
fn grep_2_a_limit_stops_the_scan_one_line_past_it() {
    let text = "x\n".repeat(50);
    assert_eq!(q("x").lines(&text, 10).len(), 11, "one more than the limit says there was more");
    assert_eq!(q("x").lines(&text, 100).len(), 50);
}

#[test]
fn grep_2_a_nul_in_the_first_8_kib_makes_a_file_binary() {
    let mut text = b"hello needle\n".to_vec();
    assert!(!is_binary(&text));
    text[3] = 0;
    assert!(is_binary(&text));
    let mut long = vec![b'a'; 9000];
    long[8191] = 0;
    assert!(is_binary(&long), "the last byte of the first 8 KiB");
    long[8191] = b'a';
    long[8192] = 0;
    assert!(!is_binary(&long), "one past it is not looked at");
}

#[test]
fn grep_2_a_line_is_trimmed_a_tab_is_a_space_and_the_column_is_the_first_matchs() {
    let l = q("need").line(7, "\t  a\tneedle \r");
    assert_eq!(l.no, 7);
    assert_eq!(l.text, "a needle");
    assert_eq!(l.marks, [2..6]);
    assert_eq!(l.col, 5, "counted in the line as it is on disk, indent included");
}

#[test]
fn grep_3_a_long_line_is_clipped_around_its_first_match() {
    let line = format!("{}needle{}", "x".repeat(500), "y".repeat(500));
    let l = q("needle").line(1, &line);
    assert_eq!(l.col, 500);
    assert!(l.text.starts_with('…'), "{}", l.text);
    assert!(l.text.chars().count() <= 201, "{}", l.text.chars().count());
    let cs: Vec<char> = l.text.chars().collect();
    assert_eq!(cs[l.marks[0].clone()].iter().collect::<String>(), "needle");
    // a match near the start keeps the whole start of the line
    let l = q("needle").line(1, &format!("ab needle{}", "y".repeat(500)));
    assert!(l.text.starts_with("ab needle"), "{}", l.text);
    assert_eq!(l.marks, [3..9]);
}

#[test]
fn grep_3_a_row_is_the_path_the_number_and_the_text_cut_to_the_width() {
    let l = q("needle").line(42, "    let needle = 1; // and some more text after it");
    let r = grep::row("src/a.rs", &l, 40);
    assert_eq!(r.text, "src/a.rs:42  let needle = 1; // and some");
    assert_eq!(r.bold, [17..23]);
    assert_eq!(r.dim, [0..11], "the path and the number");
    assert!(!r.sel);
    let wide = grep::row("src/a.rs", &l, 80);
    assert_eq!(wide.text, "src/a.rs:42  let needle = 1; // and some more text after it");
    // a path takes at most two fifths of the row, made to fit
    let r = grep::row("some/deeply/nested/directory/file.rs", &l, 40);
    assert!(r.text.starts_with("…/file.rs:42  let"), "{}", r.text);
    assert!(width(&r.text) <= 40);
}

#[test]
fn grep_3_a_match_the_width_would_cut_off_is_brought_into_view_after_an_ellipsis() {
    let l = q("needle").line(5, "aaaa bbbb cccc dddd eeee ffff gggg needle hhhh");
    let r = grep::row("a", &l, 26);
    assert_eq!(r.text, "a:5  …ggg needle hhhh", "4 chars before the match, after the ellipsis");
    assert_eq!(r.bold, [10..16]);
    // a match that starts in view but ends past the edge is cut there
    let r = grep::row("a", &q("needle").line(5, "x needle needle"), 12);
    assert_eq!(r.text, "a:5  x needl");
    assert_eq!(r.bold, [7..12]);
    // a match that starts past the edge brings the row to it
    let r = grep::row("a", &q("needle").line(5, "0123456789 needle"), 14);
    assert_eq!(r.text, "a:5  …789 need");
    assert_eq!(r.bold, [10..14]);
}

#[test]
fn grep_3_wide_chars_are_counted_in_columns() {
    let l = q("語").line(3, "日本語のテキスト");
    let r = grep::row("a", &l, 15);
    assert_eq!(width(&r.text), 15, "{:?}", r.text);
    assert_eq!(r.text, "a:3  日本語のテ");
    assert_eq!(r.bold, [9..11], "a wide char is two columns");
}

#[test]
fn grep_3_a_file_row_has_the_directories_secondary_and_the_matched_chars_bold() {
    let r = grep::file_row("src/sim.rs", &[4..7], 30);
    assert_eq!(r.text, "src/sim.rs");
    assert_eq!((r.bold, r.dim), (vec![4..7], vec![0..4]));
    // shortened, the matches are found again in what is left of the path
    let r = grep::file_row("some/deeply/nested/directory/file.rs", &[29..33], 20);
    assert_eq!(r.text, "…/directory/file.rs");
    assert_eq!((r.bold, r.dim), (vec![12..16], vec![0..12]));
    // a match in the part that was cut off is not drawn
    let r = grep::file_row("some/deeply/nested/directory/file.rs", &[0..2, 29..30], 20);
    assert_eq!(r.bold, [12..13]);
    // a name with nothing above it has no directory
    assert_eq!(grep::file_row("README.md", &[], 30).dim, Vec::<std::ops::Range<u16>>::new());
}

#[test]
fn grep_3_hostile_chars_in_a_row_are_replaced() {
    let l = q("x").line(1, "a\u{1b}[2Jx\u{202e}\u{7}");
    let r = grep::row("p", &l, 40);
    assert!(!r.text.chars().any(|c| c.is_control() || c == '\u{202e}'), "{:?}", r.text);
    let r = grep::row("dir\u{1b}[31m/x\n.rs", &l, 60);
    assert!(!r.text.chars().any(char::is_control), "{:?}", r.text);
    let r = grep::file_row("dir\u{1b}[31m/x\n.rs", &[], 40);
    assert!(!r.text.chars().any(char::is_control), "{:?}", r.text);
}

#[test]
fn grep_3_files_are_ordered_by_likelihood_then_path() {
    use std::cmp::Ordering::{Greater, Less};
    assert_eq!(grep::rank(("b", 2.0), ("a", 1.0)), Less, "the likelier first");
    assert_eq!(grep::rank(("a", 1.0), ("b", 2.0)), Greater);
    assert_eq!(grep::rank(("a", 0.0), ("b", 0.0)), Less, "then by path");
    assert_eq!(grep::rank(("a", 0.0), ("a", 0.0)), std::cmp::Ordering::Equal);
}

#[test]
fn grep_5_rg_output_is_path_nul_line_colon_text() {
    assert_eq!(
        parse_rg(b"src/a.rs\x0012:let x = 1;"),
        Some((&b"src/a.rs"[..], 12, &b"let x = 1;"[..]))
    );
    assert_eq!(
        parse_rg(b"x:y.txt\x001:a:b needle"),
        Some((&b"x:y.txt"[..], 1, &b"a:b needle"[..])),
        "a colon in the path or the text is not a separator"
    );
    assert_eq!(parse_rg(b"a\x007:"), Some((&b"a"[..], 7, &b""[..])));
    assert_eq!(
        parse_rg(b"d/bin.dat: binary file matches (found \"\\0\" byte around offset 6)"),
        None
    );
    assert_eq!(parse_rg(b"no separator"), None);
    assert_eq!(parse_rg(b"a\x00x:text"), None, "the line number must be one");
}

// ---- the search over real directories ----

/// Writes `files` (`path=content`) under `dir`.
fn put(dir: &Path, files: &[(&str, &str)]) {
    for (rel, content) in files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
}

fn rg() -> Option<PathBuf> {
    let found = io::which_rg(std::env::var_os("PATH"));
    if found.is_none() {
        eprintln!("skipped: rg is not on PATH");
    }
    found
}

/// What a search sent: the batches, in order.
struct Sent {
    batches: Vec<Batch>,
}

impl Sent {
    fn groups(&self) -> Vec<&Group> {
        self.batches.iter().flat_map(|b| &b.groups).collect()
    }

    /// `path:line` for every line found, in the order sent.
    fn found(&self) -> Vec<String> {
        let all = self.groups().into_iter();
        all.flat_map(|g| g.lines.iter().map(move |l| format!("{}:{}", g.path, l.no))).collect()
    }

    fn sorted(&self) -> Vec<String> {
        let mut f = self.found();
        f.sort();
        f
    }

    fn capped(&self) -> bool {
        self.batches.last().is_some_and(|b| b.capped)
    }

    fn done(&self) -> bool {
        self.batches.last().is_some_and(|b| b.done)
    }
}

fn request(dir: &Path, files: &[&str], first: &[&str], query: &str) -> Request {
    Request {
        generation: 1,
        root: dir.to_path_buf(),
        query: query.into(),
        first: first.iter().map(ToString::to_string).collect(),
        files: Arc::new(files.iter().map(ToString::to_string).collect()),
    }
}

fn search(dir: &Path, files: &[&str], query: &str, with: Option<&Path>) -> Sent {
    search_first(dir, files, &[], query, with)
}

fn search_first(
    dir: &Path,
    files: &[&str],
    first: &[&str],
    query: &str,
    rg: Option<&Path>,
) -> Sent {
    let req = request(dir, files, first, query);
    let mut batches = vec![];
    io::grep(&req, rg, &|| true, &mut |b| batches.push(b));
    assert!(batches.iter().all(|b| b.generation == 1));
    Sent { batches }
}

/// Runs a test body once with the built-in reader and once with rg (when it is there).
fn both(body: impl Fn(Option<&Path>)) {
    body(None);
    if let Some(rg) = rg() {
        body(Some(&rg));
    }
}

#[test]
fn grep_2_the_files_of_the_list_are_searched_and_text_files_only() {
    both(|with| {
        let d = temp_dir("g2-list");
        put(
            &d.0,
            &[
                ("a.txt", "one\nneedle two\n"),
                ("sub/b.txt", "needle\n\nneedle again\n"),
                ("not-listed.txt", "needle\n"),
                ("bin.dat", "needle\0binary\n"),
                ("late-nul.dat", "needle\n"),
            ],
        );
        let mut late = b"needle first\n".to_vec();
        late.resize(9000, b'x');
        late.extend(b"\0\nneedle after the nul\n");
        std::fs::write(d.0.join("late-nul.dat"), late).unwrap();
        let list = ["a.txt", "bin.dat", "late-nul.dat", "sub/b.txt", "gone.txt"];
        let sent = search(&d.0, &list, "needle", with);
        let found = sent.sorted();
        assert!(found.contains(&"a.txt:2".to_string()), "{found:?}");
        assert!(
            found.contains(&"sub/b.txt:1".to_string())
                && found.contains(&"sub/b.txt:3".to_string())
        );
        assert!(!found.iter().any(|f| f.starts_with("not-listed")), "only the list: {found:?}");
        assert!(!found.iter().any(|f| f.starts_with("bin.dat")), "binary: {found:?}");
        assert!(!found.iter().any(|f| f.starts_with("gone.txt")));
        assert!(sent.done());
    });
}

#[test]
fn grep_2_the_built_in_reader_reads_crlf_and_bytes_that_are_not_utf8() {
    let d = temp_dir("g2-bytes");
    std::fs::write(d.0.join("w.txt"), b"x\r\nfoo bar\r\n\xffzz foo\n").unwrap();
    let sent = search(&d.0, &["w.txt"], "foo", None);
    let g = &sent.groups()[0].lines;
    assert_eq!(g.iter().map(|l| l.no).collect::<Vec<_>>(), [2, 3]);
    assert_eq!(g[0].text, "foo bar", "the CR of the line break is not text");
    assert_eq!(g[1].text, "\u{fffd}zz foo", "bytes that are not UTF-8 are read as U+FFFD");
}

#[test]
fn grep_2_files_that_are_no_text_or_too_big_are_skipped_without_a_word() {
    let d = temp_dir("g2-skip");
    put(&d.0, &[("ok.txt", "needle\n")]);
    // a FIFO would block the thread that opens it; a sparse file of 17 MiB is over the limit
    let fifo = d.0.join("fifo");
    assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    let big = std::fs::File::create(d.0.join("big.txt")).unwrap();
    big.set_len(17 * 1024 * 1024).unwrap();
    std::fs::write(d.0.join("small-dir-as-file"), "").unwrap();
    std::fs::create_dir(d.0.join("dir.txt")).unwrap();
    let list = ["big.txt", "dir.txt", "fifo", "ok.txt", "small-dir-as-file"];
    let sent = search(&d.0, &list, "needle", None);
    assert_eq!(sent.found(), ["ok.txt:1"]);
    assert!(sent.done());
}

use std::process::Command;

#[test]
fn grep_5_the_built_in_reader_and_rg_find_the_same_lines() {
    let Some(rg) = rg() else { return };
    let d = temp_dir("g5-same");
    let code = code_text(3000, 60);
    put(
        &d.0,
        &[
            ("src/code.rs", &code),
            ("src/Upper.rs", "Foo foo FOO\nfoo()\n"),
            ("docs/日本.md", "日本語のfooです\n  foo\t(x)\n"),
            ("-n.txt", "foo in a file that looks like an option\n"),
            ("empty.txt", ""),
            ("nonl.txt", "foo without a newline"),
        ],
    );
    let list = ["-n.txt", "docs/日本.md", "empty.txt", "nonl.txt", "src/Upper.rs", "src/code.rs"];
    for query in ["foo", "Foo", "foo(", "(x)", "日本", "-n", "zzqx", "o"] {
        let (a, b) = (search(&d.0, &list, query, None), search(&d.0, &list, query, Some(&rg)));
        let by = |s: &Sent| -> Vec<String> {
            let mut v: Vec<_> = s
                .groups()
                .iter()
                .flat_map(|g| {
                    g.lines.iter().map(|l| format!("{}:{}:{}:{:?}", g.path, l.no, l.text, l.marks))
                })
                .collect();
            v.sort();
            v
        };
        let (mut a, mut b) = (a, b);
        assert_eq!(a.capped(), b.capped(), "query {query:?}");
        if !a.capped() {
            assert_eq!(by(&a), by(&b), "query {query:?}");
        }
        a.batches.clear();
        b.batches.clear();
    }
}

#[test]
fn grep_8_a_query_or_a_file_name_that_looks_like_an_option_is_not_one() {
    both(|with| {
        let d = temp_dir("g8-option");
        put(
            &d.0,
            &[("-n.txt", "a --files b\n"), ("--help", "--files here\n"), ("x.txt", "nothing\n")],
        );
        let sent = search(&d.0, &["--help", "-n.txt", "x.txt"], "--files", with);
        assert_eq!(sent.sorted(), ["--help:1", "-n.txt:1"]);
    });
}

#[test]
fn grep_5_the_first_group_is_sent_at_once_and_a_file_is_never_split() {
    both(|with| {
        let d = temp_dir("g5-batches");
        let names: Vec<String> = (0..12).map(|i| format!("f{i:02}.txt")).collect();
        for n in &names {
            put(&d.0, &[(n, &"needle line\n".repeat(60))]);
        }
        let list: Vec<&str> = names.iter().map(String::as_str).collect();
        let sent = search(&d.0, &list, "needle", with);
        assert_eq!(
            sent.batches[0].groups.len(),
            1,
            "the first file is sent as soon as it is whole"
        );
        assert!(sent.batches.len() > 1, "and the rest follows in batches");
        let mut seen = std::collections::HashSet::new();
        for g in sent.groups() {
            assert!(seen.insert(g.path.clone()), "{} is in two groups", g.path);
            assert_eq!(g.lines.len(), 60, "{} was cut", g.path);
        }
        assert_eq!(seen.len(), 12);
        assert!(sent.done() && sent.batches[..sent.batches.len() - 1].iter().all(|b| !b.done));
    });
}

#[test]
fn grep_5_the_search_stops_at_1000_lines_and_says_so_only_when_there_were_more() {
    both(|with| {
        let d = temp_dir("g5-cap");
        put(
            &d.0,
            &[
                ("a.txt", &"hit\n".repeat(600)),
                ("b.txt", &"hit\n".repeat(400)),
                ("c.txt", "miss\n"),
            ],
        );
        let sent = search(&d.0, &["a.txt", "b.txt", "c.txt"], "hit", with);
        assert_eq!(sent.found().len(), MAX_LINES);
        assert!(!sent.capped(), "exactly 1000: nothing more was found");
        put(&d.0, &[("c.txt", "hit\n")]);
        let sent = search(&d.0, &["a.txt", "b.txt", "c.txt"], "hit", with);
        assert_eq!(sent.found().len(), MAX_LINES);
        assert!(sent.capped(), "a 1001st line was found");
        assert!(sent.done());
        // one file with many more than a thousand
        put(&d.0, &[("d.txt", &"hit\n".repeat(5000))]);
        let sent = search(&d.0, &["d.txt"], "hit", with);
        assert_eq!(sent.found().len(), MAX_LINES);
        assert!(sent.capped());
    });
}

#[test]
fn grep_5_files_are_taken_in_the_order_of_the_history_first() {
    // The built-in reader goes file by file, in the order asked. rg reads a piece of the list on
    // all cores, so inside a piece its files come as they are done (the panel puts them in their
    // place anyway, GREP-3); the pieces are in order.
    let d = temp_dir("g5-order");
    let names = ["a.txt", "b.txt", "c.txt", "d.txt"];
    for n in names {
        put(&d.0, &[(n, "hit\n")]);
    }
    let sent = search_first(&d.0, &names, &["c.txt", "a.txt"], "hit", None);
    let order: Vec<&str> = sent.groups().iter().map(|g| g.path.as_str()).collect();
    assert_eq!(order, ["c.txt", "a.txt", "b.txt", "d.txt"]);
    if let Some(rg) = rg() {
        let sent = search_first(&d.0, &names, &["c.txt", "a.txt"], "hit", Some(&rg));
        assert_eq!(sent.sorted(), ["a.txt:1", "b.txt:1", "c.txt:1", "d.txt:1"]);
    }
    // the cap keeps the likeliest files
    put(&d.0, &[("c.txt", &"hit\n".repeat(1000)), ("a.txt", &"hit\n".repeat(10))]);
    let sent = search_first(&d.0, &names, &["c.txt"], "hit", None);
    assert_eq!(sent.groups()[0].path, "c.txt");
    assert_eq!(sent.groups()[0].lines.len(), 1000);
    assert!(sent.capped() && sent.groups().len() == 1);
}

#[test]
fn grep_5_a_newer_generation_stops_the_old_search() {
    both(|with| {
        let d = temp_dir("g5-stale");
        let names: Vec<String> = (0..40).map(|i| format!("f{i:02}.txt")).collect();
        for n in &names {
            put(&d.0, &[(n, "needle\n")]);
        }
        let list: Vec<&str> = names.iter().map(String::as_str).collect();
        let req = request(&d.0, &list, &[], "needle");
        let asked = AtomicUsize::new(0);
        let mut sent = vec![];
        // the generation is not the newest any more after the first few questions
        let live = || asked.fetch_add(1, Ordering::SeqCst) < 5;
        io::grep(&req, with, &live, &mut |b| sent.push(b));
        let n: usize = sent.iter().map(|b| b.groups.len()).sum();
        assert!(n < 40, "{n} files were searched after the generation was old");
        assert!(sent.iter().all(|b| !b.done), "a cancelled search does not say it is done");
        // and one that is old from the start sends nothing
        let mut sent = vec![];
        io::grep(&req, with, &|| false, &mut |b| sent.push(b));
        assert!(sent.is_empty());
    });
}

#[test]
fn grep_5_a_thread_answers_with_batches_tagged_with_its_generation_and_the_newest_wins() {
    let d = temp_dir("g5-thread");
    put(&d.0, &[("a.txt", "needle\n"), ("b.txt", "other\n")]);
    let (tx, rx) = std::sync::mpsc::channel();
    let greps = io::Greps::new(tx, None);
    let mut old = request(&d.0, &["a.txt", "b.txt"], &[], "needle");
    old.generation = 1;
    greps.ask(old);
    let mut new = request(&d.0, &["a.txt", "b.txt"], &[], "other");
    new.generation = 2;
    greps.ask(new);
    let mut found = vec![];
    while let Ok(ev) = rx.recv_timeout(Duration::from_secs(5)) {
        let sim::sim::SimEvent::Grep(b) = ev else { panic!("{ev:?}") };
        if b.generation == 2 {
            found.extend(b.groups.iter().map(|g| g.path.clone()));
        }
        if b.generation == 2 && b.done {
            break;
        }
    }
    assert_eq!(found, ["b.txt"]);
}

#[test]
fn grep_7_links_are_not_followed_out_of_the_root() {
    both(|with| {
        let d = temp_dir("g7-links");
        let outside = temp_dir("g7-outside");
        put(&outside.0, &[("secret.txt", "needle outside\n"), ("dir/deep.txt", "needle deep\n")]);
        put(&d.0, &[("in.txt", "needle inside\n"), ("real/x.txt", "needle real\n")]);
        symlink(outside.0.join("secret.txt"), d.0.join("link.txt")).unwrap();
        symlink(outside.0.join("dir"), d.0.join("dirlink")).unwrap();
        symlink(d.0.join("in.txt"), d.0.join("alias.txt")).unwrap(); // a link inside the root
        // the list is what git says; a tracked file whose directory became a link is in it
        let list = ["alias.txt", "dirlink/deep.txt", "in.txt", "link.txt", "real/x.txt"];
        let sent = search(&d.0, &list, "needle", with);
        assert_eq!(sent.sorted(), ["in.txt:1", "real/x.txt:1"]);
        // a directory that is a link to another inside the root stays in the tree, and is read
        symlink("real", d.0.join("realink")).unwrap();
        let sent = search(&d.0, &["realink/x.txt"], "needle", with);
        assert_eq!(sent.found(), ["realink/x.txt:1"]);
    });
}

#[test]
fn grep_7_a_list_with_nothing_to_read_never_makes_rg_search_the_directory() {
    both(|with| {
        let d = temp_dir("g7-empty");
        let outside = temp_dir("g7-empty-out");
        put(&outside.0, &[("s.txt", "needle\n")]);
        put(&d.0, &[("unlisted.txt", "needle\n")]);
        symlink(outside.0.join("s.txt"), d.0.join("link.txt")).unwrap();
        // rg given no file at all would search the working directory, which is not the list
        let sent = search(&d.0, &["link.txt", "gone.txt"], "needle", with);
        assert!(sent.found().is_empty());
        assert!(sent.done());
    });
}

#[test]
fn grep_7_a_name_that_climbs_out_of_the_root_is_not_searched() {
    both(|with| {
        let d = temp_dir("g7-climb");
        let outside = temp_dir("g7-climb-out");
        put(&outside.0, &[("x.txt", "needle\n")]);
        put(&d.0, &[("in.txt", "needle\n")]);
        let up = format!("../{}/x.txt", outside.0.file_name().unwrap().to_str().unwrap());
        let abs = outside.0.join("x.txt").to_string_lossy().into_owned();
        let sent = search(&d.0, &["in.txt", &up, &abs], "needle", with);
        assert_eq!(sent.found(), ["in.txt:1"]);
    });
}

#[test]
fn grep_7_a_preview_does_not_follow_a_link_either_and_reads_at_most_2_mib() {
    let d = temp_dir("g7-preview");
    let outside = temp_dir("g7-preview-out");
    put(&outside.0, &[("secret.txt", "secret\n")]);
    put(&d.0, &[("in.txt", "inside\n")]);
    symlink(outside.0.join("secret.txt"), d.0.join("link.txt")).unwrap();
    std::fs::write(d.0.join("big.txt"), vec![b'x'; 3 * 1024 * 1024]).unwrap();
    let read = |rel: &str| {
        let sim::sim::SimEvent::Preview(pv) = io::preview(&d.0, rel) else { panic!() };
        assert_eq!(pv.path, rel);
        pv.file
    };
    assert_eq!(read("in.txt"), Ok(Some(b"inside\n".to_vec())));
    assert!(read("link.txt").is_err());
    assert!(read("big.txt").unwrap_err().contains("too big"));
    assert_eq!(read("gone.txt"), Ok(None), "a file that is gone is a new file, as :e makes");
}

#[test]
fn grep_8_rg_is_found_only_in_the_absolute_directories_of_path() {
    let d = temp_dir("g8-which");
    let bin = d.0.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let fake = bin.join("rg");
    std::fs::write(&fake, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = |dirs: &[&str]| Some(OsString::from(dirs.join(":")));
    let abs = bin.to_str().unwrap();
    assert_eq!(io::which_rg(path(&["", ".", "bin", "./bin", "../bin", abs])), Some(fake.clone()));
    assert_eq!(io::which_rg(path(&["", ".", "bin", "./bin", "../bin", "target/debug"])), None);
    assert_eq!(io::which_rg(path(&["/nonexistent", abs])), Some(fake.clone()));
    assert_eq!(io::which_rg(None), None);
    // a file that is not executable, or a directory called rg, is not it
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(io::which_rg(path(&[abs])), None);
    std::fs::remove_file(&fake).unwrap();
    std::fs::create_dir(&fake).unwrap();
    assert_eq!(io::which_rg(path(&[abs])), None);
}

/// A `rg` that is this script, in a directory of its own.
fn fake_rg(dir: &Path, script: &str) -> PathBuf {
    let rg = dir.join("fake-rg");
    std::fs::write(&rg, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&rg, std::fs::Permissions::from_mode(0o755)).unwrap();
    rg
}

#[test]
fn grep_5_a_rg_that_fails_or_cannot_start_leaves_the_piece_to_the_built_in_reader() {
    let d = temp_dir("g5-badrg");
    put(&d.0, &[("a.txt", "needle\n"), ("b.txt", "none\n")]);
    let list = ["a.txt", "b.txt"];
    for script in ["exit 2", "kill -9 $$", "echo 'rg: broken' >&2; exit 127"] {
        let rg = fake_rg(&d.0, script);
        let sent = search(&d.0, &list, "needle", Some(&rg));
        assert_eq!(sent.found(), ["a.txt:1"], "{script}");
    }
    let sent = search(&d.0, &list, "needle", Some(&d.0.join("not-there")));
    assert_eq!(sent.found(), ["a.txt:1"], "rg that cannot be started");
    // an rg that finds nothing (exit 1) is believed: the built-in reader does not look again
    let rg = fake_rg(&d.0, "exit 1");
    assert!(search(&d.0, &list, "needle", Some(&rg)).found().is_empty());
}

/// The inner half of the next test: it runs in a process whose environment has a config for rg.
#[test]
#[ignore = "run by grep_8_rg_does_not_read_its_config_and_runs_no_pre_script"]
fn grep_8_inner_search_with_a_config_in_the_environment() {
    let (Some(dir), Some(rg)) = (std::env::var_os("SIM_G8_DIR"), rg()) else { return };
    let dir = PathBuf::from(dir);
    let sent = search(&dir, &["a.txt", "b.txt"], "needle", Some(&rg));
    println!("FOUND {}", sent.sorted().join(" "));
}

#[test]
fn grep_8_rg_does_not_read_its_config_and_runs_no_pre_script() {
    let Some(rg) = rg() else { return };
    let d = temp_dir("g8-config");
    let witness = d.0.join("witness");
    let pre = d.0.join("pre.sh");
    std::fs::write(&pre, format!("#!/bin/sh\ntouch '{}'\ncat \"$1\"\n", witness.display()))
        .unwrap();
    std::fs::set_permissions(&pre, std::fs::Permissions::from_mode(0o755)).unwrap();
    let rc = d.0.join("rgrc");
    std::fs::write(&rc, format!("--pre={}\n--pre-glob=*\n--no-line-number\n", pre.display()))
        .unwrap();
    put(&d.0, &[("a.txt", "needle a\n"), ("b.txt", "needle b\n")]);
    // the premise: rg, given this config, does run the script
    let plain = Command::new(&rg)
        .env("RIPGREP_CONFIG_PATH", &rc)
        .current_dir(&d.0)
        .args(["needle", "a.txt"])
        .output()
        .unwrap();
    assert!(
        plain.status.success() && witness.exists(),
        "the config must work for this test to mean something"
    );
    std::fs::remove_file(&witness).unwrap();
    // sim's search in a process that has that config in its environment
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "--ignored",
            "--nocapture",
            "grep_8_inner_search_with_a_config_in_the_environment",
        ])
        .env("RIPGREP_CONFIG_PATH", &rc)
        .env("SIM_G8_DIR", &d.0)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("FOUND a.txt:1 b.txt:1"), "{text}");
    assert!(!witness.exists(), "the search ran a script the repository's config named");
}

#[test]
fn grep_8_a_rg_in_the_repository_is_not_run_even_with_a_relative_path_entry() {
    // The search is given a rg by its full path (found by `which_rg`); a `rg` in the root is
    // never what it runs, whatever the entries of PATH look like.
    let d = temp_dir("g8-repo-rg");
    let witness = d.0.join("witness");
    let evil = d.0.join("rg");
    std::fs::write(&evil, format!("#!/bin/sh\ntouch '{}'\n", witness.display())).unwrap();
    std::fs::set_permissions(&evil, std::fs::Permissions::from_mode(0o755)).unwrap();
    put(&d.0, &[("a.txt", "needle\n")]);
    assert_eq!(io::which_rg(Some(OsString::from(".:"))), None);
    let sent = search(
        &d.0,
        &["a.txt", "rg"],
        "needle",
        io::which_rg(Some(OsString::from(".::rel"))).as_deref(),
    );
    assert_eq!(sent.found(), ["a.txt:1"]);
    assert!(!witness.exists());
}

// ---- the bench ----

/// A tree like a big repository: `n` files in `n / 100` directories, each of `lines` lines of
/// code-like text, `needle`-ish words in about one line in 3,000, and a git repository over it.
fn big_tree(name: &str, n: usize, lines: usize) -> (TempDir, Vec<String>) {
    let d = temp_dir(name);
    let text = code_text(lines, 50);
    let mut list = Vec::with_capacity(n);
    for i in 0..n {
        let rel = format!("pkg{:03}/mod{:02}/file{i}.rs", i / 1000, (i / 100) % 10);
        if i % 100 == 0 {
            std::fs::create_dir_all(d.0.join(&rel).parent().unwrap()).unwrap();
        }
        // every file is its own text: the line numbers differ, a few lines say `quasar`
        let mut body = text.clone();
        if i % 7 == 0 {
            body.push_str(&format!("let quasar{i} = 1;\n"));
        }
        std::fs::write(d.0.join(&rel), body).unwrap();
        list.push(rel);
    }
    list.sort();
    (d, list)
}

#[test]
#[ignore = "benchmark: run with --release"]
fn grep_5_bench_a_search_over_a_large_tree() {
    release_only();
    let n: usize =
        std::env::var("SIM_GREP_BENCH_FILES").ok().and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let (d, list) = big_tree("g5-bench", n, 20);
    let bytes: u64 =
        list.iter().take(200).map(|p| std::fs::metadata(d.0.join(p)).unwrap().len()).sum::<u64>()
            / 200;
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!(
        "grep_5: load average {}",
        load.split_whitespace().take(3).collect::<Vec<_>>().join(" ")
    );
    println!(
        "grep_5: {} files of about {bytes} bytes in {} directories",
        list.len(),
        list.len() / 100
    );
    let files = Arc::new(list);
    let rg = rg();
    let run = |query: &str, with: Option<&Path>| {
        let req = Request {
            generation: 1,
            root: d.0.clone(),
            query: query.into(),
            first: vec![],
            files: files.clone(),
        };
        let (t, mut first, mut lines, mut capped) = (Instant::now(), None, 0, false);
        io::grep(&req, with, &|| true, &mut |b| {
            first.get_or_insert_with(|| t.elapsed());
            lines += b.groups.iter().map(|g| g.lines.len()).sum::<usize>();
            capped |= b.capped;
        });
        (first.unwrap_or_else(|| t.elapsed()), t.elapsed(), lines, capped)
    };
    // (the first run of each warms the page cache; the best of three is kept)
    let mut worst_first = Duration::ZERO;
    for (label, with) in [("built-in", None), ("rg", rg.as_deref())] {
        if label == "rg" && with.is_none() {
            continue;
        }
        for query in ["quasar1", "Quasar", "quasar", "let", "zzqxzz"] {
            let best = (0..3).map(|_| run(query, with)).min_by_key(|r| r.1).unwrap();
            println!(
                "{label:>9} {query:>8}: first batch {:>9.2?} total {:>9.2?} {:>5} lines{}",
                best.0,
                best.1,
                best.2,
                if best.3 { " (1000+)" } else { "" }
            );
            worst_first = worst_first.max(best.0);
        }
    }
    println!("slowest first batch: {worst_first:.2?}");
}

#[test]
#[ignore = "benchmark: run with --release"]
fn grep_5_bench_a_batch_and_the_keys_over_1000_results_in_sim() {
    release_only();
    let mut r = rig("g5-bench-sim", &["a.txt=x\n"], &[]);
    r.hold_grep = true;
    r.keys("<C-f>hit");
    let generation = r.grep_held.last().unwrap().generation;
    let line = |no: usize| Line {
        no,
        col: 4,
        text: "let hit = 1; // some text after it".into(),
        marks: vec![4..7],
    };
    // 1000 files of one line each: the most rows (2,000) a capped search can fill
    let groups: Vec<Group> = (0..1000)
        .map(|i| Group {
            path: format!("pkg{:03}/mod/file{i}.rs", i % 40),
            lines: vec![line(i + 1)],
        })
        .collect();
    let (mut worst, t) = (Duration::ZERO, Instant::now());
    let ev = SimEvent::Grep(grep::Batch { generation, groups, done: true, capped: true });
    r.sim.update(ev);
    let batch = t.elapsed();
    for k in "jjjjjjjjjjkkkkkkkkkkGggnnnNNN".chars().cycle().take(3000) {
        let t = Instant::now();
        r.sim.update(SimEvent::Term(sim_core::core::app::Event::Key(
            sim_core::core::app::Key::Char(k),
        )));
        worst = worst.max(t.elapsed());
    }
    println!("grep_5: a batch of 1000 files with 1000 lines in Sim::update: {batch:.2?}");
    println!("grep_5: the worst of 3000 keys over those results: {worst:.2?}");
    assert!(
        batch < Duration::from_millis(16) && worst < Duration::from_millis(16),
        "{batch:?} {worst:?}"
    );
}

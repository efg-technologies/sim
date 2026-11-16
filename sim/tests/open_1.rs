//! OPEN-1: `gf` opens the file named under the cursor, in Sim: a path with `:line[:col]`, beside the file
//! first and then in the root; a Markdown link's target; a name that does not exist says so.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;

/// The cursor as `line:col`, 1-based.
fn at(r: &Rig) -> (usize, usize) {
    let b = &r.app().buf;
    (b.line_of(b.cursor) + 1, b.col() + 1)
}

fn shown(r: &Rig) -> String {
    r.open_files()[r.sim.cur].clone()
}

const FILES: [&str; 5] = [
    "a.txt=see src/b.rs:2:3 and nope.rs here\n",
    "src/b.rs=one\ntwo two\nthree\n",
    "docs/guide.md=# guide\n[the bee](../src/b.rs#top) and [twin](twin.md \"title\") and src/b.rs\n",
    "docs/twin.md=beside\n",
    "twin.md=in the root\n",
];

#[test]
fn open_1_gf_opens_the_file_named_at_the_cursor_at_its_line_and_column() {
    let mut r = rig("o1-line", &FILES, &["a.txt"]);
    r.keys("w");
    assert_eq!(at(&r), (1, 5));
    r.keys("gf");
    assert_eq!(shown(&r), "src/b.rs");
    assert_eq!(at(&r), (2, 3));
    assert_eq!(r.text(), "one\ntwo two\nthree");
    // the file left stays open, and Tab goes back to it (no `''` across files)
    assert_eq!(r.open_files(), ["a.txt", "src/b.rs"]);
    r.keys("gg<Tab>");
    assert_eq!((shown(&r), at(&r)), ("a.txt".into(), (1, 5)));
    // a file that is open already is shown, its cursor at the line
    r.keys("gf");
    assert_eq!((shown(&r), at(&r)), ("src/b.rs".into(), (2, 3)));
    assert_eq!(r.open_files().len(), 2);
}

#[test]
fn open_1_a_name_that_does_not_exist_says_so_and_nothing_opens() {
    let mut r = rig("o1-none", &FILES, &["a.txt"]);
    r.keys("/nope<CR>gf");
    assert!(r.status().starts_with("gf: no file nope.rs "), "{}", r.status());
    assert_eq!(r.open_files(), ["a.txt"]);
    assert!(!r.cwd.join("nope.rs").exists(), "nothing is made");
    r.keys("$gf");
    assert!(r.status().starts_with("gf: no file here "), "{}", r.status());
    r.keys("0f gf");
    assert!(r.status().starts_with("gf: no file name under the cursor"), "{}", r.status());
}

#[test]
fn open_1_beside_the_file_first_then_in_the_root_and_a_markdown_link_s_target() {
    let mut r = rig("o1-md", &FILES, &["docs/guide.md"]);
    // the link's target, from its text, without its `#`: relative to the file's directory
    r.keys("jlgf");
    assert_eq!(shown(&r), "src/b.rs", "../src/b.rs, out of docs/, with `..` taken out");
    r.keys("<Tab>");
    // the target without its title; beside the file wins over the root
    r.keys("f[lgf");
    assert_eq!(shown(&r), "docs/twin.md");
    r.keys("<S-Tab><S-Tab>");
    assert_eq!(shown(&r), "docs/guide.md");
    // a plain name that is not beside the file is the root's
    r.keys("$gf");
    assert_eq!(shown(&r), "src/b.rs");
}

#[test]
fn open_1_a_name_out_of_the_root_opens_since_it_was_asked_for() {
    let outside = temp_dir("o1-outside");
    std::fs::write(outside.0.join("far.txt"), "far away\n").unwrap();
    let line = format!("a.txt={}/far.txt\n", outside.0.display());
    let mut r = rig("o1-out", &[&line], &["a.txt"]);
    r.keys("gf");
    assert_eq!(r.text(), "far away");
    assert!(shown(&r).ends_with("/far.txt"), "{}", shown(&r));
}

#[test]
fn open_1_plain_mode_is_one_file_and_gf_is_the_core_s_there() {
    let mut r = rig("o1-plain", &FILES, &["--plain", "a.txt"]);
    r.keys("wgf");
    assert_eq!(r.open_files(), ["a.txt"]);
    assert!(!r.jobs.contains(&"seek"));
}

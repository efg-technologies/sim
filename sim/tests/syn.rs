//! SYN-1, SYN-3, SYN-4: typographic spans from syntect, the cache of parse states, and the
//! choice of language.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::golden;
use proptest::prelude::*;
use sim::syn::{Attr, EVERY, Spans, Syn};

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn spans(syn: &mut Syn, text: &[String], from: usize, n: usize) -> Vec<Spans> {
    syn.spans(&|i| text.get(i).cloned(), from, n)
}

/// Each line, and under it one symbol per char: `.` plain, `-` dim, `B` bold, `i` italic.
fn dump(name: &str, first_line: &str, text: &str) -> String {
    let text = lines(text);
    let mut syn = Syn::new(name, first_line).unwrap();
    let all = spans(&mut syn, &text, 0, text.len());
    let mut out = format!("{name}: {}\n", syn.name);
    for (line, spans) in text.iter().zip(&all) {
        let mut sym = vec!['.'; line.chars().count()];
        for (range, attr) in spans {
            for s in &mut sym[range.clone()] {
                *s = match attr {
                    Attr::Dim => '-',
                    Attr::Bold => 'B',
                    Attr::Italic => 'i',
                };
            }
        }
        out += &format!("|{line}|\n|{}|\n", sym.iter().collect::<String>());
    }
    out
}

const RUST: &str = r##"//! Module docs
use std::io;

/// Adds one.
pub fn add(x: u32) -> u32 {
    let s = "a \"quoted\" word"; // trailing
    let c = 'x';
    /* block
       comment fn if */
    if x == 1 { return x + 1; } // else
    match x { _ => x }
}

pub struct S<T: Clone> where T: 'static { a: u8, 日本: &'static str }
"##;

const PYTHON: &str = r#"#!/usr/bin/env python3
# comment with def and return
import os

class A:
    """Docstring with return and if."""

    def f(self, x):
        s = 'single' + "double" + f"{x} fmt"
        if x and not s:
            return None  # done
        for i in range(3):
            pass
        return x
"#;

const SHELL: &str = r#"#!/bin/sh
# a comment with if and then
if [ -f "$HOME/x" ]; then echo 'single' "double $USER"; fi
for i in 1 2 3; do ls -l | grep x; done
case $x in a) echo a ;; *) echo b ;; esac
x=$(date) # trailing
"#;

const MARKDOWN: &str = "# Title with `code`\n\nSome *emphasis* and **strong** text, plain again.\n\n- item one\n- item two\n\n> quote\n\n```rust\nfn main() {}\n```\n\n## Second heading\n\n<!-- hidden -->\n";

#[test]
fn syn_1_golden_rust() {
    golden("syn_rust", &dump("src/lib.rs", "", RUST));
}

#[test]
fn syn_1_golden_python() {
    golden("syn_python", &dump("tool.py", "#!/usr/bin/env python3", PYTHON));
}

#[test]
fn syn_1_golden_shell() {
    golden("syn_shell", &dump("deploy.sh", "#!/bin/sh", SHELL));
}

#[test]
fn syn_1_golden_markdown() {
    golden("syn_markdown", &dump("README.md", "# Title", MARKDOWN));
}

#[test]
fn syn_1_comments_are_dim_keywords_bold_strings_and_chars_italic() {
    let mut syn = Syn::new("a.rs", "").unwrap();
    let text = lines("fn f() { let c = 'x'; let s = \"s\"; // c\n}");
    let all = spans(&mut syn, &text, 0, 2);
    let attrs =
        |line: usize| all[line].iter().map(|(r, a)| (r.start, r.end, *a)).collect::<Vec<_>>();
    // `fn` and `let` are bold, the literals italic with their quotes, the comment dim to the end.
    assert_eq!(
        attrs(0),
        [
            (0, 2, Attr::Bold),
            (9, 12, Attr::Bold),
            (17, 20, Attr::Italic),
            (22, 25, Attr::Bold),
            (30, 33, Attr::Italic),
            (35, 39, Attr::Dim)
        ]
    );
    assert!(all[1].is_empty(), "a closing brace is plain");
}

#[test]
fn syn_1_the_wording_of_a_comment_or_a_string_never_makes_it_bold() {
    let mut syn = Syn::new("a.rs", "").unwrap();
    let text = lines("// fn if let return\nlet s = \"fn if let return\";\n/* fn\n   return */");
    let all = spans(&mut syn, &text, 0, 4);
    assert_eq!(all[0], [(0..19, Attr::Dim)]);
    assert_eq!(all[1], [(0..3, Attr::Bold), (8..26, Attr::Italic)]);
    assert_eq!(all[2], [(0..5, Attr::Dim)]);
    assert_eq!(all[3], [(0..12, Attr::Dim)]);
}

#[test]
fn syn_1_operators_and_numbers_stay_plain() {
    let mut syn = Syn::new("a.rs", "").unwrap();
    let all = spans(&mut syn, &lines("x = 1 + 2 * 3 == 4;"), 0, 1);
    assert!(all[0].is_empty(), "{:?}", all[0]);
}

#[test]
fn syn_1_a_multiline_construct_carries_over_to_the_next_line() {
    let mut syn = Syn::new("a.py", "").unwrap();
    let all = spans(&mut syn, &lines("x = \"\"\"one\ntwo\nthree\"\"\" + y"), 0, 3);
    assert_eq!(all[1], [(0..3, Attr::Italic)]);
    assert_eq!(all[2], [(0..8, Attr::Italic)]);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn syn_1_spans_are_in_order_apart_non_empty_and_inside_the_line(
        text in proptest::collection::vec("[ a-z\"'/*#(){};=日本é\\\\]{0,30}", 1..20),
        ext in prop_oneof![Just("rs"), Just("py"), Just("sh"), Just("md"), Just("c"), Just("html")],
    ) {
        let mut syn = Syn::new(&format!("f.{ext}"), "").unwrap();
        let all = spans(&mut syn, &text, 0, text.len());
        prop_assert_eq!(all.len(), text.len());
        for (line, spans) in text.iter().zip(&all) {
            let mut end = 0;
            for (r, _) in spans {
                prop_assert!(r.start < r.end && r.start >= end && r.end <= line.chars().count());
                end = r.end;
            }
        }
    }
}

/// A Rust file of `n` lines, each a short statement and a comment.
fn big(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("    let v{i} = \"s{i}\"; // c{i}")).collect()
}

#[test]
fn syn_3_the_cache_gives_the_same_spans_as_parsing_from_the_top() {
    let text = big(1000);
    let mut warm = Syn::new("a.rs", "").unwrap();
    spans(&mut warm, &text, 0, 1000); // all states cached
    let mut cold = Syn::new("a.rs", "").unwrap();
    let want = spans(&mut cold, &text, 0, 1000);
    for from in [0, 1, EVERY - 1, EVERY, EVERY + 1, 500, 777, 990] {
        assert_eq!(
            spans(&mut warm, &text, from, 10),
            want[from..(from + 10).min(1000)],
            "from {from}"
        );
    }
}

#[test]
fn syn_3_an_edit_parses_again_only_from_the_nearest_state_at_or_above_it() {
    let mut text = big(1000);
    let mut syn = Syn::new("a.rs", "").unwrap();
    spans(&mut syn, &text, 900, 40);
    assert_eq!(syn.parsed, 940, "the first time, from the top");
    // Lines 900..940 again: every state is there, so it parses from the one at or above 900.
    syn.parsed = 0;
    spans(&mut syn, &text, 900, 40);
    assert_eq!(syn.parsed, 940 - 900 / EVERY * EVERY);
    // An edit at line 500 keeps the states up to 500: the viewport at 900 parses from there.
    text[500] = "    /* open".to_string();
    syn.edit(500);
    syn.parsed = 0;
    let all = spans(&mut syn, &text, 900, 40);
    assert_eq!(syn.parsed, 940 - 500 / EVERY * EVERY);
    // The comment that was opened above now swallows the viewport, as a fresh parse says.
    let mut fresh = Syn::new("a.rs", "").unwrap();
    assert_eq!(all, spans(&mut fresh, &text, 900, 40));
    assert!(all.iter().all(|l| l.len() == 1 && l[0].1 == Attr::Dim));
    // An edit below the viewport costs the viewport nothing but the lines since its state.
    syn.edit(990);
    syn.parsed = 0;
    spans(&mut syn, &text, 100, 40);
    assert_eq!(syn.parsed, 140 - 100 / EVERY * EVERY);
}

#[test]
fn syn_3_after_an_edit_the_edited_line_costs_less_than_a_step_of_parsing() {
    let text = big(500);
    let mut syn = Syn::new("a.rs", "").unwrap();
    for e in [0, 1, EVERY - 1, EVERY, 100, 101, 499] {
        spans(&mut syn, &text, 0, 500); // every state is cached
        syn.edit(e);
        syn.parsed = 0;
        assert_eq!(spans(&mut syn, &text, e, 1).len(), 1);
        assert_eq!(syn.parsed, e % EVERY + 1, "edit at {e}");
    }
}

#[test]
fn syn_3_an_edit_on_a_state_boundary_keeps_the_state_before_it_and_drops_the_one_after() {
    let text = big(300);
    let mut syn = Syn::new("a.rs", "").unwrap();
    spans(&mut syn, &text, 0, 300);
    let at = 8 * EVERY;
    syn.edit(at); // the state at the start of that line depends only on the lines above it
    assert_eq!(syn.distance(at), 0);
    assert_eq!(syn.distance(at + 100), 100);
    syn.edit(0);
    assert_eq!((syn.distance(0), syn.distance(1)), (0, 1));
}

#[test]
fn syn_3_distance_says_how_much_a_jump_has_to_parse() {
    let text = big(5000);
    let mut syn = Syn::new("a.rs", "").unwrap();
    assert_eq!(syn.distance(4000), 4000, "nothing is cached yet");
    spans(&mut syn, &text, 0, 100);
    assert_eq!(syn.distance(50), 50 % EVERY);
    assert_eq!(syn.distance(99), 99 % EVERY);
    assert_eq!(syn.distance(4000), 4000 - 96, "the last state is the one at line 96");
    spans(&mut syn, &text, 4000, 10);
    assert_eq!(syn.distance(4000), 4000 % EVERY);
}

#[test]
fn syn_3_lines_past_the_end_give_nothing_and_an_empty_window_costs_nothing() {
    let text = big(10);
    let mut syn = Syn::new("a.rs", "").unwrap();
    assert_eq!(spans(&mut syn, &text, 8, 10).len(), 2);
    assert!(spans(&mut syn, &text, 50, 5).is_empty());
    assert!(spans(&mut syn, &text, 3, 0).is_empty());
    assert!(spans(&mut syn, &[], 0, 5).is_empty());
}

#[test]
fn syn_3_the_state_can_move_to_a_background_thread() {
    fn send<T: Send>(_: &T) {}
    let syn = Syn::new("a.rs", "").unwrap();
    send(&syn);
    let text = big(100);
    let h = std::thread::spawn(move || {
        let mut syn = syn;
        spans(&mut syn, &text, 90, 5).len()
    });
    assert_eq!(h.join().unwrap(), 5);
}

#[test]
fn syn_4_the_language_comes_from_the_extension() {
    let name = |file: &str, first: &str| Syn::new(file, first).map(|s| s.name);
    assert_eq!(name("src/main.rs", ""), Some("Rust"));
    assert_eq!(name("a/b/tool.PY", ""), Some("Python"), "case does not matter");
    assert_eq!(name("page.html", ""), Some("HTML"));
    assert_eq!(name("notes.md", ""), Some("Markdown"));
    assert_eq!(name("conf.yaml", ""), Some("YAML"));
    assert_eq!(name("x.tar.gz.sh", ""), Some("Bourne Again Shell (bash)"), "the last extension");
}

#[test]
fn syn_4_the_language_comes_from_the_file_name_first() {
    let name = |file: &str| Syn::new(file, "").map(|s| s.name);
    assert_eq!(name("Makefile"), Some("Makefile"));
    assert_eq!(name("sub/dir/Makefile"), Some("Makefile"));
    assert_eq!(
        name("/home/me/.bashrc"),
        Some("Bourne Again Shell (bash)"),
        "a dotfile is all name, no extension"
    );
    assert_eq!(name("Rakefile"), Some("Ruby"));
    assert_eq!(name("Gemfile"), Some("Ruby"));
    // The name beats the first line.
    assert_eq!(Syn::new("Makefile", "#!/usr/bin/env python").map(|s| s.name), Some("Makefile"));
}

#[test]
fn syn_4_without_a_known_name_the_first_line_decides() {
    let name = |file: &str, first: &str| Syn::new(file, first).map(|s| s.name);
    assert_eq!(name("script", "#!/usr/bin/env python3"), Some("Python"));
    assert_eq!(name("script", "#!/bin/sh"), Some("Bourne Again Shell (bash)"));
    assert_eq!(name("tool", "#!/usr/bin/perl -w"), Some("Perl"));
    assert_eq!(name("data.unknown", "<?xml version=\"1.0\"?>"), Some("XML"));
    assert_eq!(name("script", "just some text"), None);
    assert_eq!(name("script", ""), None);
}

#[test]
fn syn_4_plain_text_and_unknown_files_are_not_highlighted() {
    assert!(Syn::new("notes.txt", "").is_none());
    assert!(Syn::new("data.xyz", "").is_none());
    assert!(Syn::new("", "").is_none());
    assert!(Syn::new("dir/", "").is_none());
}

/// Documents a gap of the default set (SYN-4: "the languages are syntect's default set"):
/// these have no syntax in it and show plain until the set changes.
#[test]
fn syn_4_languages_missing_from_the_default_set_show_plain() {
    for file in ["Cargo.toml", "app.ts", "main.tsx", "Dockerfile", "lib.zig", "x.kt", "page.svelte"]
    {
        assert!(Syn::new(file, "").is_none(), "{file} now has a syntax: revisit SPEC");
    }
}

//! SYN-1, SYN-3, SYN-4: the looks of syntect's scopes, the cache of parse states, and the
//! choice of language.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::golden;
use proptest::prelude::*;
use sim::paint::Look;
use sim::syn::{EVERY, MAX_LINE, Spans, Syn};

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn spans(syn: &mut Syn, text: &[String], from: usize, n: usize) -> Vec<Spans> {
    syn.spans(&|i| text.get(i).cloned(), from, n)
}

/// UI-8: the symbol of a look of markdown in the dumps: `1`-`6` headings, `m` a marker, `k` the
/// backticks of inline code, `C` its text, `e` emphasis, `L` a link's text, `U` its URL, `b` a
/// bullet, `q` a quote's bar, `l` the language of a fence, `~` the zero-width band.
fn md_symbol(l: Look) -> char {
    match l {
        Look::H1 => '1',
        Look::H2 => '2',
        Look::H3 => '3',
        Look::H4 => '4',
        Look::H5 => '5',
        Look::H6 => '6',
        Look::Mark => 'm',
        Look::Tick => 'k',
        Look::Code => 'C',
        Look::Em => 'e',
        Look::Link => 'L',
        Look::Url => 'U',
        Look::Bullet => 'b',
        Look::Quote => 'q',
        Look::Lang => 'l',
        _ => '~',
    }
}

/// Each line, and under it one symbol per char: `.` plain, `K` keyword, `S` string, `N` number or
/// constant, `T` type, `F` function, `c` comment, `B` bold.
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
                    Look::Com => 'c',
                    Look::Str => 'S',
                    Look::Kw => 'K',
                    Look::Num => 'N',
                    Look::Ty => 'T',
                    Look::Fun => 'F',
                    Look::Bold => 'B',
                    Look::Sel => 'g',
                    l => md_symbol(*l),
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
fn syn_1_comments_keywords_strings_numbers_types_and_functions_have_their_looks() {
    let mut syn = Syn::new("a.rs", "").unwrap();
    let text = lines("fn f(x: Vec<u8>) { let s = \"s\"; g(1); // c\n}");
    let all = spans(&mut syn, &text, 0, 2);
    let attrs =
        |line: usize| all[line].iter().map(|(r, a)| (r.start, r.end, *a)).collect::<Vec<_>>();
    assert_eq!(
        attrs(0),
        [
            (0, 2, Look::Kw),
            (3, 4, Look::Fun),
            (8, 11, Look::Ty),
            (12, 14, Look::Kw),
            (19, 22, Look::Kw),
            (27, 30, Look::Str),
            (32, 33, Look::Fun),
            (34, 35, Look::Num),
            (38, 42, Look::Com)
        ]
    );
    assert!(all[1].is_empty(), "a closing brace is plain");
}

#[test]
fn syn_1_the_wording_of_a_comment_or_a_string_never_makes_it_a_keyword() {
    let mut syn = Syn::new("a.rs", "").unwrap();
    let text = lines("// fn if let return\nlet s = \"fn if let 1\";\n/* fn\n   return */");
    let all = spans(&mut syn, &text, 0, 4);
    assert_eq!(all[0], [(0..19, Look::Com)]);
    assert_eq!(all[1], [(0..3, Look::Kw), (8..21, Look::Str)]);
    assert_eq!(all[2], [(0..5, Look::Com)]);
    assert_eq!(all[3], [(0..12, Look::Com)]);
}

#[test]
fn syn_1_no_look_is_blue_and_blue_has_no_scope() {
    // The looks are the five hues of SYN-1 (red, green, yellow, magenta, cyan) and typography;
    // there is no variant that could be drawn blue.
    for look in [Look::Com, Look::Str, Look::Kw, Look::Num, Look::Ty, Look::Fun] {
        assert!(!format!("{look:?}").contains("Blue"));
    }
}

#[test]
fn syn_1_operators_names_and_punctuation_stay_plain_and_numbers_are_numbers() {
    let mut syn = Syn::new("a.rs", "").unwrap();
    let all = spans(&mut syn, &lines("x = 1 + 2 * 3 == 4;"), 0, 1);
    let at: Vec<_> = all[0].iter().map(|(r, l)| (r.start, *l)).collect();
    assert_eq!(at, [(4, Look::Num), (8, Look::Num), (12, Look::Num), (17, Look::Num)]);
}

#[test]
fn syn_1_a_multiline_construct_carries_over_to_the_next_line() {
    let mut syn = Syn::new("a.py", "").unwrap();
    let all = spans(&mut syn, &lines("x = \"\"\"one\ntwo\nthree\"\"\" + y"), 0, 3);
    assert_eq!(all[1], [(0..3, Look::Str)]);
    assert_eq!(all[2], [(0..8, Look::Str)]);
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
    assert!(all.iter().all(|l| l.len() == 1 && l[0].1 == Look::Com));
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
fn syn_3_a_line_over_the_limit_is_plain_and_not_parsed_and_the_lines_around_it_are_not_hurt() {
    // The parse time of one line is bounded: a shell heredoc whose word is 100,000 chars took
    // 5.8 s (release) in the one thread that highlights every file; minified code, a megabyte
    // on a line, takes seconds at every edit.
    let long = format!("cat <<{}", "E".repeat(MAX_LINE));
    assert!(long.len() > MAX_LINE);
    let text = vec!["if true; then".to_string(), long, "fi # done".to_string()];
    let mut syn = Syn::new("a.sh", "").unwrap();
    let all = spans(&mut syn, &text, 0, 3);
    assert_eq!(syn.parsed, 2, "the long line was not parsed");
    assert!(all[1].is_empty(), "and is plain");
    assert_eq!(all[0], [(0..2, Look::Kw), (3..7, Look::Fun), (9..13, Look::Kw)]);
    assert_eq!(
        all[2],
        [(0..2, Look::Kw), (3..9, Look::Com)],
        "the lines around it have their looks"
    );
    // One byte less is parsed.
    let text = vec![format!("cat <<{}", "E".repeat(MAX_LINE - 6))];
    spans(&mut Syn::new("a.sh", "").unwrap(), &text, 0, 1);
    let mut syn = Syn::new("a.sh", "").unwrap();
    spans(&mut syn, &text, 0, 1);
    assert_eq!(syn.parsed, 1);
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

/// SYN-4: the languages syntect's own default set lacked, which bat's set (`two-face`) has.
#[test]
fn syn_4_the_languages_the_author_uses_daily_have_a_syntax() {
    let name = |file: &str| Syn::new(file, "").map(|s| s.name);
    assert_eq!(name("Cargo.toml"), Some("TOML"));
    assert_eq!(name("app.ts"), Some("TypeScript"));
    assert_eq!(name("main.tsx"), Some("TypeScriptReact"));
    assert_eq!(name("Dockerfile"), Some("Dockerfile"));
    for file in ["lib.zig", "x.kt", "page.svelte", "flake.nix", "a.json", "x.go", "a.js"] {
        assert!(Syn::new(file, "").is_some(), "{file} has no syntax");
    }
}

#[test]
fn syn_4_javascript_is_read_with_typescript_s_syntax_because_the_one_for_javascript_loops() {
    // bat's "JavaScript" syntax loops for ever in syntect 5.3 with the pure-Rust regex engine on
    // `x = y /* c */`, a statement without a semicolon that ends in a comment, and has no `.mjs`,
    // `.cjs` or `.jsx` (bat maps those itself). TypeScript's syntax reads JavaScript, and does not.
    let name = |file: &str, first: &str| Syn::new(file, first).map(|s| s.name);
    for file in ["a.js", "a.mjs", "a.cjs", "a.es6", "A.JS"] {
        assert_eq!(name(file, ""), Some("TypeScript"), "{file}");
    }
    assert_eq!(name("a.jsx", ""), Some("TypeScriptReact"));
    assert_eq!(
        name("cli", "#!/usr/bin/env node"),
        Some("TypeScript"),
        "a first line that names node"
    );
    assert_eq!(name("a.ts", ""), Some("TypeScript"));
    // The text that loops, in a thread that is given up on if it does.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let text = ["x = y /* c */".to_string(), "/".into(), "/".into(), " /*".into()];
        let mut syn = Syn::new("a.js", "").unwrap();
        let _ = tx.send(syn.spans(&|i| text.get(i).cloned(), 0, 4));
    });
    let spans = rx.recv_timeout(std::time::Duration::from_secs(10)).expect("a.js did not finish");
    assert!(spans[0].contains(&(6..13, Look::Com)), "{:?}", spans[0]);
}

#[test]
fn syn_1_golden_toml() {
    golden("syn_toml", &dump("Cargo.toml", "", TOML));
}

#[test]
fn syn_1_golden_typescript() {
    golden("syn_typescript", &dump("app.ts", "", TYPESCRIPT));
}

#[test]
fn syn_1_golden_dockerfile() {
    golden("syn_dockerfile", &dump("Dockerfile", "", DOCKERFILE));
}

const TOML: &str =
    "[package]\nname = \"sim\" # c\nedition = 2024\nflag = true\nlist = [\"a\", 1.5]\n";

const TYPESCRIPT: &str = "import { a } from \"x\";\ninterface Foo<T> { a: string }\nexport class Bar extends Baz {\n  private x: number = 3;\n  async run(): Promise<void> { if (this.x) { return; } } // c\n}\nconst r = `t${1}`;\n";

const DOCKERFILE: &str = "FROM rust:1.98 AS build\nARG X=1\nRUN cargo build --release && echo \"done\"\nCMD [\"sim\"]\n# comment\n";

//! MD-10: the class of what makes a link, and what a detector of lying links must not hang on. Every
//! construct of CommonMark and GFM that makes a link is judged, fails closed, or is out of scope for a
//! reason (a table of them, run through the walk that draws the screen); the attempts to get a link to
//! one place past the mark while it says another; the files the highlighter calls Markdown; and the lines
//! and documents that a quadratic detector would hang on, which are bounded in time and fail closed past
//! their caps.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::lie::{Budget, Refs, deceptive, line_marks};
use sim::syn::{self, Syn};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The marks of a Markdown document as the walk that draws it makes them: for each line, the chars that
/// are marked, as text, in the order of the lines and of the chars. Spans come from the highlighter, as
/// they do on the screen (inline code and fenced blocks are known by them).
fn doc_marks(path: &str, text: &str) -> Vec<String> {
    let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let mut syn = Syn::new(path, "").unwrap();
    let sp = syn.spans(&|i| lines.get(i).cloned(), 0, lines.len());
    let refs = Arc::new(Refs::new(lines.len(), &|l| lines.get(l).cloned()));
    let get = || refs.clone();
    let mut budget = Budget::frame();
    let mut out = vec![];
    for (l, line) in lines.iter().enumerate() {
        let cs: Vec<char> = line.chars().collect();
        let marks = line_marks(l, &cs, &sp[l], &|k| lines.get(k).cloned(), Some(&get), &mut budget);
        out.extend(marks.into_iter().map(|r| cs[r].iter().collect::<String>()));
    }
    out
}

fn marked(text: &str) -> Vec<String> {
    doc_marks("a.md", text)
}

/// Every row of a table of `(what, document, the marks)`, at once.
fn rows(table: &[(&str, &str, &[&str])]) {
    let wrong: Vec<String> = table
        .iter()
        .filter_map(|(what, doc, want)| {
            let got = marked(doc);
            (got != *want)
                .then(|| format!("{what}: {doc:?}\n    marked {got:?}\n    wanted {want:?}"))
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "{} of {} rows differ:\n{}",
        wrong.len(),
        table.len(),
        wrong.join("\n")
    );
}

#[test]
fn md_10_an_inline_link_is_judged_in_every_form_of_its_target() {
    rows(&[
        ("plain", "[good.com](https://evil.com)", &["https://evil.com"]),
        ("angle with a blank", "[good.com](<https://evil.com >)", &["<https://evil.com >"]),
        ("with a title", "[good.com](https://evil.com \"t\")", &["https://evil.com"]),
        ("with a single-quoted title", "[good.com](https://evil.com 't')", &["https://evil.com"]),
        ("with blanks", "[good.com](  https://evil.com  )", &["https://evil.com"]),
        ("with a tab", "[good.com](\thttps://evil.com\t)", &["https://evil.com"]),
        ("with parentheses", "[good.com](https://evil.com/a_(b))", &["https://evil.com/a_(b)"]),
        ("an entity in the text", "[good&#46;com](https://evil.com)", &["https://evil.com"]),
        ("an escape in the text", "[good\\.com](https://evil.com)", &["https://evil.com"]),
        (
            "a backslash",
            "[good.com](https://evil.com\\@good.com/)",
            &["https://evil.com\\@good.com/"],
        ),
        ("a scheme-relative target", "[good.com](//evil.com/x)", &["//evil.com/x"]),
        ("no slashes after the scheme", "[good.com](https:evil.com)", &["https:evil.com"]),
        (
            "the text over lines",
            "[good.com\nofficial](https://evil.com)",
            &["[good.com", "https://evil.com"],
        ),
        (
            "the target on the next line",
            "[good.com](\nhttps://evil.com)",
            &["[good.com]", "https://evil.com"],
        ),
        ("a link that says what it is", "[good.com](https://good.com/x)", &[]),
        (
            "an image in a link is judged by its alt text",
            "[![good.com](logo.png)](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "an image in a link that says what it is",
            "[![Build](badge.svg)](https://ci.example.com/x)",
            &[],
        ),
        ("a link that is a word", "[docs](https://evil.com)", &[]),
        (
            "a link that is text over lines and honest",
            "[good.com\nofficial](https://good.com)",
            &[],
        ),
    ]);
}

#[test]
fn md_10_a_reference_link_is_judged_in_its_three_forms_and_wherever_its_definition_is() {
    rows(&[
        ("full", "[good.com][1]\n\n[1]: https://evil.com", &["[good.com]", "https://evil.com"]),
        (
            "collapsed",
            "[good.com][]\n\n[good.com]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "shortcut",
            "[good.com]\n\n[good.com]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        ("honest", "[good.com][1]\n\n[1]: https://good.com/x", &[]),
        (
            "a definition in a block quote",
            "[good.com][1]\n\n> [1]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "in a list item",
            "[good.com][1]\n\n- [1]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "in a starred item",
            "[good.com][1]\n\n* [1]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "in an ordered item",
            "[good.com][1]\n\n1. [1]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "in a numbered item with a parenthesis",
            "[good.com][1]\n\n10) [1]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "nested containers",
            "[good.com][1]\n\n> - > [1]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "indented",
            "[good.com][1]\n\n   [1]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "the destination on the next line",
            "[good.com][1]\n\n[1]:\n  https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "the destination on the next line, in a quote",
            "[good.com][1]\n\n> [1]:\n> https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "with a title",
            "[good.com][1]\n\n[1]: https://evil.com \"t\"",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "in angle brackets",
            "[good.com][1]\n\n[1]: <https://evil.com>",
            &["[good.com]", "<https://evil.com>"],
        ),
        (
            "the text over lines",
            "[good.com\nofficial][1]\n\n[1]: https://evil.com",
            &["[good.com", "official]", "https://evil.com"],
        ),
        (
            "a label as CommonMark matches it",
            "[good.com][The   Label]\n\n[THE label]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "no definition and none that could be one: no link in any renderer",
            "[good.com][nowhere]\n\nA [good.com] and [good.com][] here.",
            &[],
        ),
        (
            "no definition, but a line that could be one: not trusted",
            "A [good.com] and [good.com][] here.\n\nfoo]: /bar",
            &["[good.com]", "[good.com]"],
        ),
        (
            "no definition, but a label that sim does not fold",
            "[good.com][\u{df}]\n\n[good.com][SS]\n\n[\u{e9}]: /bar",
            &["[good.com]", "[good.com]"],
        ),
        ("no definition, no address", "A [docs][nowhere] and [docs].\n\nfoo]: /bar", &[]),
        (
            "a label that Unicode folds the same",
            "[good.com][ß]\n\n[SS]: https://evil.com",
            &["[good.com]"],
        ),
        ("an image's reference", "![good.com][1]\n\n[1]: https://evil.com", &[]),
        ("a footnote", "A note[^1].\n\n[^1]: https://evil.com", &[]),
        ("a task", "- [x] good.com\n\n[x]: https://evil.com", &[]),
    ]);
}

#[test]
fn md_10_a_link_of_raw_html_is_judged_inline_and_as_a_block() {
    rows(&[
        ("inline", "<a href=\"https://evil.com\">good.com</a>", &["https://evil.com"]),
        ("single quotes", "<a href='https://evil.com'>good.com</a>", &["https://evil.com"]),
        ("no quotes", "<a href=https://evil.com>good.com</a>", &["https://evil.com"]),
        ("upper case", "<A HREF=\"https://evil.com\">good.com</A>", &["https://evil.com"]),
        (
            "other attributes",
            "<a class=\"x\" data-href=\"https://good.com\" href=\"https://evil.com\" target=\"_blank\">good.com</a>",
            &["https://evil.com"],
        ),
        (
            "a block over lines",
            "<a\n  href='https://evil.com'\n>good.com</a>",
            &["https://evil.com"],
        ),
        (
            "the text over lines",
            "<a href=\"https://evil.com\">good.com\nofficial</a>",
            &["https://evil.com"],
        ),
        (
            "tags in the text",
            "<a href=\"https://evil.com\"><b>good.com</b></a>",
            &["https://evil.com"],
        ),
        (
            "an entity in the target",
            "<a href=\"&#104;ttps://evil.com\">good.com</a>",
            &["&#104;ttps://evil.com"],
        ),
        ("no end tag: not trusted", "<a href=\"https://evil.com\">good.com", &["https://evil.com"]),
        ("honest", "<a href=\"https://good.com/x\">good.com</a>", &[]),
        ("honest, a word", "<a href=\"https://evil.com\">docs</a>", &[]),
        ("an abbreviation is no link", "<abbr title=\"good.com\">good.com</abbr>", &[]),
    ]);
}

#[test]
fn md_10_a_link_is_judged_wherever_the_block_around_it_puts_it() {
    rows(&[
        (
            "a table cell",
            "| a | b |\n|---|---|\n| [good.com](https://evil.com) | x |",
            &["https://evil.com"],
        ),
        ("a heading", "# [good.com](https://evil.com)", &["https://evil.com"]),
        ("a setext heading", "[good.com](https://evil.com)\n===", &["https://evil.com"]),
        ("strong", "**[good.com](https://evil.com)**", &["https://evil.com"]),
        ("emphasis", "*[good.com](https://evil.com)*", &["https://evil.com"]),
        ("a block quote", "> [good.com](https://evil.com)", &["https://evil.com"]),
        ("a list item", "- [good.com](https://evil.com)", &["https://evil.com"]),
        (
            "a footnote's text",
            "A[^1].\n\n[^1]: See [good.com](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "two links on a line",
            "[a.com](https://a.com) [good.com](https://evil.com) [b.com](https://b.com)",
            &["https://evil.com"],
        ),
    ]);
}

#[test]
fn md_10_what_is_no_link_with_a_text_that_can_differ_is_out_of_scope() {
    rows(&[
        ("an autolink: its text is its target", "<https://evil.com>", &[]),
        ("a bare URL: its text is its target", "see https://evil.com and www.evil.com", &[]),
        (
            "an image in a link, with no alt text: its pixels are not in the file",
            "[![](pixels.png)](https://evil.com)",
            &[],
        ),
        ("an image", "![good.com](https://evil.com/x.png)", &[]),
        ("prose with brackets", "an array a[0] and (parentheses) and [1] here", &[]),
    ]);
}

#[test]
fn md_10_the_attempts_to_get_a_link_past_the_mark_that_a_renderer_would_send_to_evil_com() {
    let long_text = format!("[{} good.com](https://evil.com)", "a ".repeat(600));
    let long_dest = format!("[good.com](https://good.com/{})", "a".repeat(3000));
    let long_ref = format!("[{} good.com][1]\n\n[1]: https://evil.com", "a ".repeat(600));
    let got = marked(&long_ref);
    assert!(
        got.len() == 1 && got[0].ends_with("good.com]"),
        "a reference text that is too long to judge: {} marks",
        got.len()
    );
    rows(&[
        ("emphasis inside the host", "[go**od**.com](https://evil.com)", &["https://evil.com"]),
        ("emphasis around a part", "[**good**.com](https://evil.com)", &["https://evil.com"]),
        ("underscores", "[_good_.com](https://evil.com)", &["https://evil.com"]),
        ("code around it", "[`good.com`](https://evil.com)", &["https://evil.com"]),
        ("an empty tag in the host", "[good<b></b>.com](https://evil.com)", &["https://evil.com"]),
        ("tags around it", "[<b>good.com</b>](https://evil.com)", &["https://evil.com"]),
        ("a soft hyphen as an entity", "[good&shy;.com](https://evil.com)", &["https://evil.com"]),
        ("an entity that is not known", "[good&zwj;.com](https://evil.com)", &["https://evil.com"]),
        ("a zero-width space", "[good\u{200b}.com](https://evil.com)", &["https://evil.com"]),
        ("a soft hyphen", "[go\u{ad}od.com](https://evil.com)", &["https://evil.com"]),
        ("a variation selector", "[good.com\u{fe0f}](https://evil.com)", &["https://evil.com"]),
        ("upper case", "[good.com](HTTPS://EVIL.COM)", &["HTTPS://EVIL.COM"]),
        (
            "a title with a parenthesis",
            "[good.com](https://evil.com \"a ) b\")",
            &["https://evil.com"],
        ),
        ("a text that is too long to judge", &long_text, &["https://evil.com"]),
        ("a target that is too long to judge", &long_dest, &[&long_dest[11..11 + 2049]]),
        (
            "a text over five lines",
            "[a\nb\nc\nd\ngood.com](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "an href without its tag",
            "text\nhref=\"https://evil.com\">good.com</a>",
            &["https://evil.com"],
        ),
        (
            "a reference in a heading",
            "# [good.com][1]\n\n[1]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "a reference in a cell",
            "| [good.com][1] |\n|---|\n\n[1]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "a definition after text",
            "text\n[1]: https://evil.com\n\n[good.com][1]",
            &["https://evil.com", "[good.com]"],
        ),
        (
            "every definition of a label counts",
            "[good.com][1]\n\n[1]: https://good.com\n[1]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
    ]);
}

#[test]
fn md_10_what_a_highlighter_calls_code_is_judged_all_the_same() {
    // a stray backtick, a fence that the renderer does not open or has closed: the highlighter takes the
    // link for code and a renderer shows it; so code is not exempt (the differential fuzz found each row)
    rows(&[
        ("a stray backtick before it", "`[good.com](https://evil.com)", &["https://evil.com"]),
        ("a stray backtick in its text", "[good.com`](https://evil.com)", &["https://evil.com"]),
        (
            "a backtick on the line before that pairs with the first of this one",
            "`\n`a [good.com](https://evil.com) `",
            &["https://evil.com"],
        ),
        ("two backticks, then one", "``\n[good.com](https://evil.com)\n`", &["https://evil.com"]),
        (
            "a fence in a quote and a line outside it",
            "> ```\n[good.com](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "a backtick fence with a backtick in its info string",
            "``` a`b\n[good.com](https://evil.com)\n```",
            &["https://evil.com"],
        ),
        ("a fence", "```\n[good.com](https://evil.com)\n```", &["https://evil.com"]),
        ("a span of inline code", "`[good.com](https://evil.com)`", &["https://evil.com"]),
        ("an escaped bang is no image", "\\![good.com](https://evil.com)", &["https://evil.com"]),
    ]);
}

#[test]
fn md_10_a_use_whose_text_is_written_with_escapes_entities_or_emphasis_is_judged_as_it_reads() {
    rows(&[
        (
            "an escaped dot, a shortcut",
            "[good\\.com]\n\n[good\\.com]: https://evil.com",
            &["[good\\.com]", "https://evil.com"],
        ),
        (
            "emphasis in the host, a collapsed one",
            "[go**od**.com][]\n\n[go**od**.com]: https://evil.com",
            &["[go**od**.com]", "https://evil.com"],
        ),
        (
            "an entity, the definition in two quotes with its target on the next line",
            "[good.co&#109;][1]\n\n> > [1]:\n   https://evil.com",
            &["[good.co&#109;]", "https://evil.com"],
        ),
        (
            "a label with a letter that folds",
            "[good.com][\u{df}]\n\n[SS]: https://evil.com",
            &["[good.com]"],
        ),
        (
            "a definition with a letter that folds",
            "[good.com][SS]\n\n[\u{df}]: https://evil.com",
            &["[good.com]"],
        ),
    ]);
}

#[test]
fn md_10_what_a_reader_does_not_see_in_a_link_text_is_not_judged() {
    rows(&[
        (
            "a badge",
            "[![Build](https://img.shields.io/x.svg)](https://github.com/o/r/actions)",
            &[],
        ),
        (
            "a badge with no alt",
            "[![](https://img.shields.io/a.svg)](https://crates.io/crates/x)",
            &[],
        ),
        (
            "two badges",
            "[![](https://a.shields.io/a.svg) ![](https://b.shields.io/b.svg)](https://crates.io/c)",
            &[],
        ),
        (
            "a badge by reference",
            "[![Build][badge]][ci]\n\n[badge]: https://img.shields.io/x.svg\n[ci]: https://github.com/o/r",
            &[],
        ),
        (
            "an image of an address is its alt",
            "[![good.com](https://img.shields.io/a.svg)](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "a block link around an image",
            "<a href=\"https://verilib.org/cert/5132\">\n  <img src=\"https://img.shields.io/x.svg\" alt=\"x\">\n</a>",
            &[],
        ),
        (
            "an inline one",
            "<a href=\"https://github.com/o/r\"><img src=\"https://avatars.githubusercontent.com/u/1\" width=\"64\"></a>",
            &[],
        ),
        (
            "the alt of an image of raw HTML is its text",
            "<a href=\"https://evil.com\"><img src=\"https://img.x.io/y.png\" alt=\"good.com\"></a>",
            &["https://evil.com"],
        ),
        (
            "a relative target and a text over two lines",
            "Licensed under <a href=\"LICENSE-APACHE\">Apache License, Version\n2.0</a>",
            &[],
        ),
        (
            "a tag of the head that is no link",
            "<link rel=\"stylesheet\" href=\"https://fonts.googleapis.com/css2?family=Noto\">",
            &[],
        ),
        (
            "an array and a table in code",
            "```js\nroutes: [\"\", \"/index\"],\nconst x = a[pick.id];\n[plugins.example]\n```",
            &[],
        ),
        ("a footnote does not make a document loose", "[^1]: a note\n\ntext [good.com] text", &[]),
        (
            "a link inside brackets is no use of a reference, in a document that is loose",
            "* [[`0d8a6f1b19`](https://github.com/o/r/commit/0d8a6f1b19)] - a fix\n\nfoo]: /bar",
            &[],
        ),
        (
            "a line that could be a definition does",
            "text [good.com] text\n\nfoo]: /bar",
            &["[good.com]"],
        ),
    ]);
}

#[test]
fn md_10_a_word_that_follows_a_label_is_read_as_an_address_when_it_is_one() {
    rows(&[
        ("a site label", "[site:good.com](https://evil.com)", &["https://evil.com"]),
        ("a key", "[url=good.com](https://evil.com)", &["https://evil.com"]),
        ("an attribute", "[href=good.com](https://evil.com)", &["https://evil.com"]),
        ("a heading and a label", "[Note:example.com](https://evil.com)", &["https://evil.com"]),
        ("two colons", "[site::good.com](https://evil.com)", &["https://evil.com"]),
        ("a time", "[at 12:30](https://good.com)", &[]),
        ("a port", "[localhost:3000](http://localhost:3000)", &[]),
        ("a host and a port", "[good.com:443](https://good.com:443)", &[]),
        ("a path of a language", "[std::fs::File](https://good.com)", &[]),
        ("a query", "[https://good.com/?q=1](https://good.com/?q=1)", &[]),
        (
            "a query with a host in it",
            "[https://good.com/?to=x.com](https://good.com/?to=x.com)",
            &[],
        ),
    ]);
}

#[test]
fn md_10_the_brackets_pair_as_a_renderer_pairs_them_and_every_reading_is_judged() {
    rows(&[
        ("a bracket in a comment", "[good<!-- [ -->.com](https://evil.com)", &["https://evil.com"]),
        (
            "brackets in a comment",
            "[good.com<!-- [[[ -->](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "a bracket in an attribute",
            "[good.com<img alt=\"[\">](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "a bracket in the title of a tag",
            "[good<span title=\"[\">.com</span>](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "a bracket in an autolink",
            "[good.com <https://x[y]>](https://evil.com)",
            &["https://evil.com"],
        ),
        ("a bracket in code", "[`[`good.com](https://evil.com)", &["https://evil.com"]),
        ("a closing bracket in code", "[good.com`]`](https://evil.com)", &["https://evil.com"]),
        (
            "a closing bracket in code, and a link after it",
            "[good.com`]`](https://evil.com \"t\")[x](y)",
            &["https://evil.com"],
        ),
        (
            "an inner pair whose target is not one",
            "[good.com [b]((c)](\nhttps://evil.com)",
            &["[good.com [b]((c)]", "https://evil.com"],
        ),
        ("a comment that is honest", "[<!-- note -->good.com](https://good.com)", &[]),
        ("a tag that is honest", "[good.com <b>x</b>](https://good.com)", &[]),
    ]);
}

#[test]
fn md_10_what_the_real_files_showed_is_not_a_lie_and_what_hides_in_them_still_is() {
    // found by looking at every mark that 6,000 README and CHANGELOG files of one machine had
    rows(&[
        (
            "a name with underscores, as text and as target",
            "**[`tests/no_alloc.rs`](tests/no_alloc.rs)**",
            &[],
        ),
        (
            "a name with underscores, on the web",
            "[borsh/examples/serde_json_value.rs](https://github.com/o/r/blob/master/borsh/examples/serde_json_value.rs)",
            &[],
        ),
        (
            "emphasis with underscores still hides a host",
            "[_good_.com](https://evil.com)",
            &["https://evil.com"],
        ),
        ("a version is no path", "[HTTP/1.1](https://www.rfc-editor.org/rfc/rfc9110.html)", &[]),
        (
            "a repository's dot directory is no path",
            "See [`contributing.md` in `micromark/.github`][c].\n\n[c]: https://github.com/micromark/.github/blob/main/contributing.md",
            &[],
        ),
        (
            "a tilde is no home",
            "[Used by ~115,000 packages](https://www.npmjs.com/browse/depended/chalk)",
            &[],
        ),
        (
            "a home is one",
            "[~/notes.md](https://evil.com/other.md)",
            &["https://evil.com/other.md"],
        ),
        (
            "an emoji and a full stop",
            "[Give us a \u{2b50}\u{fe0f}.](https://github.com/aradzie/keybr.com)",
            &[],
        ),
        (
            "an invisible char that breaks an address still does",
            "[good\u{200b}.com](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "the colon of a sentence is no definition",
            "In Node.js, install with [npm][]:\n\n[npm]: https://www.npmjs.com\n\nan array [www.example.com] in code",
            &[],
        ),
        (
            "a label that goes over lines is a definition that sim did not read",
            "an array [www.example.com] in code\n\n[foo\nbar]: /baz",
            &["[www.example.com]"],
        ),
        (
            "the link of a block over many lines, whose end is further than a link's text goes",
            "<a href=\"https://vercel.com/\">\n  <picture>\n    <source media=\"(prefers-color-scheme: dark)\" srcset=\"a.png\">\n    <img src=\"b.png\" alt=\"Vercel\">\n  </picture>\n</a>",
            &[],
        ),
        (
            "and one that says an address at the end of it",
            "<a href=\"https://evil.com\">\n  <picture>\n    <source srcset=\"a.png\">\n    <img src=\"b.png\" alt=\"x\">\n  </picture>\n  good.com</a>",
            &["https://evil.com"],
        ),
        (
            "and one that has no end",
            "<a href=\"https://evil.com\">\n<p>\n\ntext",
            &["https://evil.com"],
        ),
    ]);
}

#[test]
fn md_10_what_a_reader_sees_that_a_text_does_not_say_is_judged_too() {
    // attempts of the second hour, from what GitHub and a browser show: strikethrough, the raw HTML that a
    // browser hides (a processing instruction, a declaration, CDATA, an empty comment), a mark of direction
    // that turns a reversed host around, a tag whose attribute has a `>` and one with a `/` for a blank
    rows(&[
        ("strikethrough", "[~~good.com~~](https://evil.com)", &["https://evil.com"]),
        ("a single tilde", "[~good.com~](https://evil.com)", &["https://evil.com"]),
        ("a processing instruction", "[good<?x?>.com](https://evil.com)", &["https://evil.com"]),
        ("a declaration", "[good<!X y>.com](https://evil.com)", &["https://evil.com"]),
        ("CDATA", "[good<![CDATA[x]]>.com](https://evil.com)", &["https://evil.com"]),
        ("an empty comment", "[good<!-->.com](https://evil.com)", &["https://evil.com"]),
        ("a right-to-left override", "[\u{202e}moc.doog](https://evil.com)", &["https://evil.com"]),
        ("an isolate", "[\u{2067}good.com](https://evil.com)", &["https://evil.com"]),
        (
            "a quoted `>` in a tag of raw HTML",
            "<a title=\"a>b\" href=\"https://evil.com\">good.com</a>",
            &["https://evil.com"],
        ),
        ("a slash for a blank", "<a/href=\"https://evil.com\">good.com</a>", &["https://evil.com"]),
        ("an ideographic full stop", "[good\u{3002}com](https://evil.com)", &["https://evil.com"]),
        ("a fullwidth full stop", "[good\u{ff0e}com](https://evil.com)", &["https://evil.com"]),
        (
            "a halfwidth ideographic full stop",
            "[good\u{ff61}com](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "a sentence of Japanese is no address",
            "[\u{7d42}\u{308f}\u{308a}\u{3002}\u{6b21}](https://good.com)",
            &[],
        ),
        (
            "a subdomain of the host in the text",
            "[nordaccount.com/manual](https://my.nordaccount.com/dashboard)",
            &[],
        ),
        (
            "a host that ends with the text's but is not a subdomain",
            "[good.com](https://evilgood.com)",
            &["https://evilgood.com"],
        ),
        ("the text is a subdomain", "[docs.good.com](https://evil.com)", &["https://evil.com"]),
        (
            "caseless letters need no folding: a definition of Japanese words is read",
            "[good.com]\n\n[\u{65e5}\u{672c}\u{8a9e}]: https://example.org/ja",
            &[],
        ),
        (
            "a path in brackets is not an address that no definition resolves, in a document that is loose",
            "if [ -d /tmp ]; then ls; fi\n\nfoo]: /bar",
            &[],
        ),
        ("an ampersand after an address", "[good.com&x](https://evil.com)", &["https://evil.com"]),
        (
            "an ampersand before an address",
            "[Tom&good.com](https://evil.com)",
            &["https://evil.com"],
        ),
        ("an ampersand in a name", "[AT&T](https://good.com)", &[]),
        (
            "an ampersand in the query of a URL",
            "[https://good.com/?a=1&b=evil.com](https://good.com/?a=1&b=evil.com)",
            &[],
        ),
        (
            "a method is no country",
            "[*dispatch*.on](https://github.com/d3/d3-dispatch#dispatch_on)",
            &[],
        ),
        ("a real country is one", "[good.kr](https://evil.com)", &["https://evil.com"]),
        (
            "a link in a comment, in a line that is read as a paragraph for its `href`",
            "<a name=\"d\" href=\"#d\">#</a> x [Source](./a.js)<!-- , [Examples](TBD) -->",
            &[],
        ),
        ("and in a code span", "<a name=\"d\" href=\"#d\">#</a> x `[Examples](TBD)` y", &[]),
        ("a member of a package is no file", "[`math/rand.Seed`](/pkg/math/rand/#Seed)", &[]),
        ("a file is one", "[`docs/guide.md`](docs/other.md)", &["docs/other.md"]),
        (
            "a shortcut image after an address",
            "[good.com![1]](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "an empty label before the dot of a subdomain",
            "[good.com](https://.good.com)",
            &["https://.good.com"],
        ),
        ("a home is not strikethrough", "[~/notes.md](notes.md)", &[]),
        (
            "a quoted `>` in an honest tag",
            "<a title=\"a>b\" href=\"https://good.com\">good.com</a>",
            &[],
        ),
    ]);
}

#[test]
fn md_10_an_image_is_no_link_in_a_reference_and_a_tag_a_browser_reads_is_not_text() {
    // found by the second differential fuzz: the text of a reference has an image whose alt is an address, and a
    // tag in a block of raw HTML that CommonMark does not read as one and a browser does
    rows(&[
        (
            "an image in the text of a reference",
            "[![good.com](x.png)][1]\n\n[1]: https://evil.com",
            &["[![good.com](x.png)]", "https://evil.com"],
        ),
        (
            "a badge by reference, honest",
            "[![Build](https://img.shields.io/x.svg)][1]\n\n[1]: https://github.com/o/r",
            &[],
        ),
        (
            "a link in the text of a reference is no use",
            "[good.com [b](c)][1]\n\n[1]: https://evil.com",
            &[],
        ),
        (
            "a tag with a junk attribute, in a block of HTML",
            "<p><a href=\"https://evil.com\">good<span title=\"]\".>.com</span></a></p>",
            &["https://evil.com"],
        ),
    ]);
}

#[test]
fn md_10_a_renderer_encodes_a_backslash_of_the_target_and_a_label_after_a_text_with_a_link_is_a_use()
 {
    // found by the third differential fuzz: cmark-gfm and markdown-it write `\` of a destination as `%5C`, so
    // `https://good.com\@evil.com/` is userinfo and evil.com in the href that a browser reads, and not good.com
    // with a path; and `[good.com [b](c)][good.com]` is a link `[b](c)` and then the shortcut `[good.com]`
    rows(&[
        (
            "a backslash before an at-sign",
            "[good.com](https://good.com\\\\@evil.com/)",
            &["https://good.com\\\\@evil.com/"],
        ),
        (
            "a backslash that CommonMark leaves",
            "[good.com](https://good.com\\evil.com/)",
            &["https://good.com\\evil.com/"],
        ),
        (
            "a host and a backslash path in a block of HTML, where a browser reads it as a slash",
            "<a href=\"https://good.com\\x\">good.com</a>",
            &[],
        ),
        (
            "a label after a text that has a link",
            "[good.com [b](c)][good.com]\n\n[good.com]: https://evil.com",
            &["[good.com]", "https://evil.com"],
        ),
        (
            "a label after a text with an image is still the label of a reference",
            "[![good.com](x.png)][1]\n\n[1]: https://good.com",
            &[],
        ),
    ]);
}

#[test]
fn md_10_a_tag_of_raw_html_that_goes_over_lines_has_its_target_marked_on_the_line_that_has_it() {
    // found by reading what the windows do: a line looks at its paragraph only when it has something that says
    // so, and a target on a line of the tag that has no `<a`, no `href` and no `>` said nothing
    rows(&[
        (
            "the value on the line after its name",
            "<a\nhref=\n\"https://evil.com\"\n>good.com</a>",
            &["https://evil.com"],
        ),
        (
            "an attribute before it, and a close tag that is no close tag",
            "<a\n class=\"x\"\n href=\n\"https://evil.com\"\n>good.com</ a>",
            &["https://evil.com"],
        ),
        ("honest", "<a\nhref=\n\"https://good.com\"\n>good.com</a>", &[]),
        (
            "two tags, one after the other",
            "<a\nhref=\n\"https://good.com\"\n>good.com</a>\n<a\nhref=\n\"https://evil.com\"\n>good.com</a>",
            &["https://evil.com"],
        ),
    ]);
    // and for the first line that a frame draws, with the tag above it
    let doc = ["<a", "href=", "\"https://evil.com\"", ">good.com</a>"].map(String::from);
    let cs: Vec<char> = doc[2].chars().collect();
    let marks = line_marks(
        2,
        &cs,
        &[],
        &|k| doc.get(k).cloned(),
        Some(&|| Arc::new(Refs::new(doc.len(), &|k| doc.get(k).cloned()))),
        &mut Budget::frame(),
    );
    assert_eq!(marks, vec![1..cs.len() - 1], "{marks:?}");
}

#[test]
fn md_10_a_target_with_a_scheme_is_never_a_relative_path() {
    // the review of e1ee30b: `search-ms:` was read as a relative path that ends with the text, and Explorer
    // opens a WebDAV share on the host in it
    rows(&[
        (
            "search-ms with a share of the host in the text",
            r"[good.com](search-ms:query=x&crumb=location:\\evil.com@SSL\DavWWWRoot\good.com)",
            &[r"search-ms:query=x&crumb=location:\\evil.com@SSL\DavWWWRoot\good.com"],
        ),
        ("ms-msdt", "[good.com](ms-msdt:/id)", &["ms-msdt:/id"]),
        ("a scheme that nobody knows", "[good.com](zz-app:good.com)", &["zz-app:good.com"]),
        ("a drive", r"[good.com](C:\good.com)", &[r"C:\good.com"]),
        ("a path of the text", "[/etc/hosts](vscode:file/etc/hosts)", &["vscode:file/etc/hosts"]),
        ("a phone", "[good.com](tel:+123)", &["tel:+123"]),
        ("a message", "[good.com](sms:+123)", &["sms:+123"]),
        ("a phone for a path", "[./a/b.md](tel:+123)", &["tel:+123"]),
        ("a text that names nothing, to an app", "[Open in VS Code](vscode:extension/x)", &[]),
        ("a text that names nothing, to a phone", "[Call us](tel:+123)", &[]),
        ("a relative path that ends with the text is honest", "[good.com](docs/good.com)", &[]),
        ("a file of the machine", "[docs](file:///usr/share/doc)", &[]),
        ("a file of the machine, by its path", "[/etc/hosts](file:///etc/hosts)", &[]),
        (
            "a path with an at-sign is no authority",
            "[good.com](file:@good.com:99999/x)",
            &["file:@good.com:99999/x"],
        ),
        ("one slash is no authority", "[good.com](file:/@good.com/x)", &["file:/@good.com/x"]),
        (
            "a share of another host",
            "[good.com](file://evil.com/share)",
            &["file://evil.com/share"],
        ),
        (
            "a share by four slashes",
            "[good.com](file:////evil.com/share)",
            &["file:////evil.com/share"],
        ),
        (
            "a share by backslashes",
            r"[good.com](file:\\evil.com\share)",
            &[r"file:\\evil.com\share"],
        ),
    ]);
    // a post box goes to the host after the last `@` of each recipient, in the address and in `to`, `cc`
    // and `bcc`
    rows(&[
        ("the box of the text", "[support@good.com](mailto:support@good.com)", &[]),
        ("with a subject", "[support@good.com](mailto:support@good.com?subject=x)", &[]),
        ("a host with a box of it", "[good.com](mailto:sales@good.com)", &[]),
        ("another host", "[good.com](mailto:a@evil.com)", &["mailto:a@evil.com"]),
        (
            "a second recipient",
            "[support@good.com](mailto:support@good.com,x@evil.com)",
            &["mailto:support@good.com,x@evil.com"],
        ),
        (
            "a copy",
            "[support@good.com](mailto:support@good.com?cc=x@evil.com)",
            &["mailto:support@good.com?cc=x@evil.com"],
        ),
        (
            "a blind copy, a header written in capitals",
            "[support@good.com](mailto:support@good.com?BCC=x@evil.com)",
            &["mailto:support@good.com?BCC=x@evil.com"],
        ),
        (
            "an at-sign that is percent-encoded",
            "[support@good.com](mailto:support%40evil.com)",
            &["mailto:support%40evil.com"],
        ),
        (
            "the host of the text again in every box",
            "[support@good.com](mailto:a@good.com?cc=b@good.com&bcc=c@good.com)",
            &[],
        ),
        (
            "a header that a line break in its value starts",
            "[support@good.com](mailto:support@good.com?subject=x%0d%0aBcc:b@evil.com)",
            &["mailto:support@good.com?subject=x%0d%0aBcc:b@evil.com"],
        ),
        (
            "a body has lines",
            "[support@good.com](mailto:support@good.com?body=Hello%0AThanks)",
            &[],
        ),
        ("no recipient", "[good.com](mailto:)", &["mailto:"]),
        ("no host in the box", "[good.com](mailto:root)", &["mailto:root"]),
    ]);
    // a special scheme with fewer than two slashes goes to the document's own host, or to the next one when
    // there is no base: it is not known
    rows(&[
        ("no slash", "[good.com](https:good.com)", &["https:good.com"]),
        ("one slash", "[good.com](https:/good.com)", &["https:/good.com"]),
        ("one backslash", r"[good.com](https:\good.com)", &[r"https:\good.com"]),
        ("two slashes", "[good.com](https://good.com)", &[]),
        // (a Markdown target reads `\\` as an escaped backslash, and a renderer writes it `%5C`)
        ("two backslashes", r"[good.com](HTTPS:\\good.com)", &[r"HTTPS:\\good.com"]),
        ("three slashes", "[good.com](https:///good.com)", &[]),
    ]);
}

#[test]
fn md_10_a_subdomain_is_trusted_unless_the_host_in_the_text_is_a_suffix_that_many_share() {
    rows(&[
        ("a platform", "[github.io](https://evil.github.io)", &["https://evil.github.io"]),
        (
            "a platform of containers",
            "[herokuapp.com](https://x.herokuapp.com)",
            &["https://x.herokuapp.com"],
        ),
        ("a storage", "[amazonaws.com](https://x.amazonaws.com)", &["https://x.amazonaws.com"]),
        (
            "a region of it",
            "[s3.amazonaws.com](https://evil.s3.amazonaws.com)",
            &["https://evil.s3.amazonaws.com"],
        ),
        ("a country's second level", "[co.uk](https://evil.co.uk)", &["https://evil.co.uk"]),
        ("another", "[or.jp](https://evil.or.jp)", &["https://evil.or.jp"]),
        ("another", "[com.au](https://evil.com.au)", &["https://evil.com.au"]),
        ("the host itself", "[github.io](https://github.io)", &[]),
        ("a page of an owner", "[user.github.io](https://docs.user.github.io)", &[]),
        (
            "an owner is not another",
            "[user.github.io](https://evil.github.io)",
            &["https://evil.github.io"],
        ),
        ("a docs host of an owner", "[python.org](https://docs.python.org/3/)", &[]),
        ("another", "[github.com](https://docs.github.com/x)", &[]),
        ("a company of a country", "[example.co.uk](https://www.example.co.uk)", &[]),
        ("its subdomain", "[example.co.uk](https://docs.example.co.uk)", &[]),
    ]);
    for suffix in [
        "github.io",
        "githubusercontent.com",
        "gitlab.io",
        "vercel.app",
        "netlify.app",
        "herokuapp.com",
        "pages.dev",
        "workers.dev",
        "web.app",
        "firebaseapp.com",
        "appspot.com",
        "blogspot.com",
        "azurewebsites.net",
        "cloudfront.net",
        "readthedocs.io",
        "wordpress.com",
        "ngrok.io",
        "ngrok-free.app",
        "onrender.com",
        "fly.dev",
        "glitch.me",
        "surge.sh",
        "bitbucket.io",
        "translate.goog",
        "myshopify.com",
        "wixsite.com",
        "notion.site",
        "substack.com",
    ] {
        let doc = format!("[{suffix}](https://evil.{suffix})");
        assert_eq!(marked(&doc), [format!("https://evil.{suffix}")], "{suffix}");
    }
}

#[test]
fn md_10_a_host_that_has_letters_that_are_not_ascii_is_an_address_and_is_judged() {
    rows(&[
        ("a Cyrillic o", "[g\u{43e}\u{43e}gle.com](https://evil.com)", &["https://evil.com"]),
        (
            "a Cyrillic o, and the real host",
            "[g\u{43e}\u{43e}gle.com](https://google.com)",
            &["https://google.com"],
        ),
        ("a Cyrillic a", "[ex\u{430}mple.com](https://example.com)", &["https://example.com"]),
        (
            "fullwidth",
            "[\u{ff47}\u{ff4f}\u{ff4f}\u{ff47}\u{ff4c}\u{ff45}\u{ff0e}\u{ff43}\u{ff4f}\u{ff4d}](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "fullwidth, and the real host",
            "[\u{ff27}\u{ff4f}\u{ff4f}\u{ff47}\u{ff4c}\u{ff45}\u{ff0e}\u{ff43}\u{ff4f}\u{ff4d}](https://google.com)",
            &[],
        ),
        ("a name, as written", "[例え.jp](https://例え.jp)", &[]),
        ("a name, as punycode", "[例え.jp](https://xn--r8jz45g.jp)", &[]),
        ("a name, another host", "[例え.jp](https://evil.jp)", &["https://evil.jp"]),
        (
            "a name, with a host after it",
            "[例え.jp](https://xn--r8jz45g.jp.evil.com)",
            &["https://xn--r8jz45g.jp.evil.com"],
        ),
        (
            "a name, as written with another host after it",
            "[例え.jp](https://例え.jp.evil.com)",
            &["https://例え.jp.evil.com"],
        ),
        ("a name with a u umlaut", "[bücher.de](https://xn--bcher-kva.de)", &[]),
        ("another", "[münchen.de](https://xn--mnchen-3ya.de/)", &[]),
        ("a name of capitals", "[MÜNCHEN.de](https://xn--mnchen-3ya.de/)", &[]),
        ("a name with a www", "[www.例え.jp](https://xn--r8jz45g.jp)", &[]),
        ("a box", "[me@例え.jp](mailto:me@xn--r8jz45g.jp)", &[]),
        ("a box of another host", "[me@例え.jp](mailto:me@evil.jp)", &["mailto:me@evil.jp"]),
        (
            "a Cyrillic o in the top-level domain",
            "[google.c\u{43e}m](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "a Cyrillic o in the domain, and the real host",
            "[google.c\u{43e}m](https://google.com)",
            &["https://google.com"],
        ),
        (
            "a Greek omicron in the domain",
            "[google.c\u{3bf}m](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "a one-dot leader for the dot",
            "[google\u{2024}com](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "a small full stop for the dot",
            "[google\u{fe52}com](https://evil.com)",
            &["https://evil.com"],
        ),
        (
            "an address of numbers has no subdomain",
            "[1.2.3.4](https://evil.1.2.3.4)",
            &["https://evil.1.2.3.4"],
        ),
        ("a sentence is no host", "[例えば、これはテスト](https://good.com)", &[]),
        ("a word with a dot that is no host", "[例え.テスト](https://good.com)", &[]),
    ]);
    // the punycode of the labels
    for (label, code) in
        [("例え", "xn--r8jz45g"), ("bücher", "xn--bcher-kva"), ("münchen", "xn--mnchen-3ya")]
    {
        assert!(!deceptive(&format!("{label}.jp"), &format!("https://{code}.jp")), "{label}");
        assert!(deceptive(&format!("{label}.jp"), &format!("https://{code}x.jp")), "{label}");
    }
}

/// MD-10: the table of URL and host forms (SPEC): a text and a target, and whether the walk marks the target
/// (a lie, or a form that fails closed) or leaves it (honest, or out of scope). By group: the schemes (special,
/// special without `//`, opaque known, opaque unknown, none, scheme-relative), the authority (userinfo, ports,
/// `\\`, `%`, a trailing dot, case, `www.`), IPv4 and IPv6, the names (IDN, punycode, fullwidth, mixed
/// scripts), and the shared suffixes.
const FORMS: &[(&str, &str, bool)] = &[
    ("good.com", "https://evil.com", true),
    ("good.com", "http://evil.com", true),
    ("good.com", "ws://evil.com", true),
    ("good.com", "wss://evil.com", true),
    ("good.com", "ftp://evil.com", true),
    ("good.com", "HTTPS://EVIL.COM", true),
    ("good.com", "https://good.com", false),
    ("good.com", "HTTP://GOOD.COM/x", false),
    ("good.com", "ftp://good.com/f", false),
    ("good.com", "https:evil.com", true),
    ("good.com", "https:/evil.com", true),
    ("good.com", "https:good.com", true),
    ("good.com", "http:/good.com", true),
    ("good.com", "wss:good.com", true),
    ("good.com", "https:\\good.com", true),
    ("good.com", "https:///good.com", false),
    ("good.com", "https:////good.com", false),
    ("good.com", "mailto:a@evil.com", true),
    ("good.com", "mailto:a@good.com", false),
    ("a@good.com", "mailto:a@good.com", false),
    ("a@good.com", "mailto:a@good.com?cc=b@evil.com", true),
    ("a@good.com", "MAILTO:a@good.com", false),
    ("good.com", "tel:123", true),
    ("good.com", "TEL:123", true),
    ("good.com", "sms:123", true),
    ("good.com", "javascript:alert(1)", true),
    ("good.com", "data:text/html,x", true),
    ("good.com", "vbscript:x", true),
    ("good.com", "blob:https://evil.com/u", true),
    ("good.com", "view-source:https://evil.com", true),
    ("good.com", "about:blank", true),
    ("good.com", "file://evil.com/s", true),
    ("good.com", "file:///etc/x", true),
    ("good.com", "file://localhost/x", true),
    ("Docs", "file:///etc/x", false),
    ("Docs", "file://localhost/x", false),
    ("good.com", "search-ms:query=x&crumb=location:\\\\evil.com@SSL\\DavWWWRoot\\good.com", true),
    ("good.com", "ms-msdt:/id", true),
    ("good.com", "ms-excel:ofe|u|https://evil.com/x.xlsx", true),
    ("good.com", "zoommtg://evil.com/j", true),
    ("good.com", "intent://evil.com#Intent;scheme=https;end", true),
    ("good.com", "c:\\good.com", true),
    ("good.com", "a:good.com", true),
    ("Open it", "search-ms:query=x", false),
    ("Open it", "ms-msdt:/id", false),
    ("good.com", "docs/good.com", false),
    ("good.com", "good.com", false),
    ("good.com", "./good.com", false),
    ("good.com", "docs/other", true),
    ("good.com", "//evil.com", true),
    ("good.com", "\\\\evil.com", true),
    ("good.com", "/\\evil.com", true),
    ("good.com", "\\/evil.com", true),
    ("good.com", "//good.com", false),
    ("good.com", "/redirect?to=evil", true),
    ("good.com", "#frag", true),
    ("good.com", "https://good.com@evil.com", true),
    ("good.com", "https://good.com:pw@evil.com", true),
    ("good.com", "https://a@good.com@evil.com", true),
    ("good.com", "https://evil.com@good.com", false),
    ("good.com", "https://good.com:80@evil.com:81/", true),
    ("good.com", "https://good.com:/", false),
    ("good.com", "https://good.com:80/", false),
    ("good.com", "https://good.com:80x/", true),
    ("good.com", "https://good.com:0x50/", true),
    ("good.com", "https://good.com\\@evil.com/", true),
    ("good.com", "https://good.com%2eevil.com/", true),
    ("good.com", "https://good.com%40evil.com/", true),
    ("good.com", "https://good.com./", false),
    ("good.com", "https://good.com../", true),
    ("good.com", "https://GOOD.com/", false),
    ("good.com", "https://www.good.com/", false),
    ("www.good.com", "https://good.com/", false),
    ("good.com", "https:///", true),
    ("good.com", "https://@good.com", false),
    ("good.com", "https://good.com#@evil.com", false),
    ("good.com", "https://good.com?@evil.com", false),
    ("good.com", "https://evil.com#@good.com", true),
    ("good.com", "https://good.com\u{3002}evil.com/", true),
    ("good.com", "https://good.com\u{ff20}evil.com/", true),
    ("good.com", "https://good.com\u{200b}.evil.com/", true),
    ("good.com", "https://good.com.evil.com/", true),
    ("good.com", "https://evilgood.com/", true),
    ("good.com", "https://good.com-evil.com/", true),
    ("good.com", "https://good_com.evil.com", true),
    ("127.0.0.1", "http://127.0.0.1/", false),
    ("127.0.0.1", "http://127.0.0.1:8080/x", false),
    ("127.0.0.1", "http://2130706433/", true),
    ("127.0.0.1", "http://0x7f000001/", true),
    ("127.0.0.1", "http://0177.0.0.1/", true),
    ("127.0.0.1", "http://127.1/", true),
    ("127.0.0.1", "http://127.0.0.2/", true),
    ("127.0.0.1", "http://[::1]/", true),
    ("127.0.0.1", "http://127.0.0.1./", false),
    ("good.com", "http://127.0.0.1/", true),
    ("good.com", "http://2130706433/", true),
    ("good.com", "http://[::ffff:127.0.0.1]/", true),
    ("localhost", "http://localhost:3000/", false),
    ("localhost", "http://127.0.0.1:3000/", true),
    ("g\u{43e}\u{43e}gle.com", "https://evil.com", true),
    ("g\u{43e}\u{43e}gle.com", "https://google.com", true),
    ("g\u{43e}\u{43e}gle.com", "https://g\u{43e}\u{43e}gle.com", false),
    ("g\u{43e}\u{43e}gle.com", "https://xn--ggle-55da.com", false),
    ("example.com", "https://ex\u{430}mple.com", true),
    (
        "\u{ff47}\u{ff4f}\u{ff4f}\u{ff47}\u{ff4c}\u{ff45}\u{ff0e}\u{ff43}\u{ff4f}\u{ff4d}",
        "https://evil.com",
        true,
    ),
    (
        "\u{ff47}\u{ff4f}\u{ff4f}\u{ff47}\u{ff4c}\u{ff45}\u{ff0e}\u{ff43}\u{ff4f}\u{ff4d}",
        "https://google.com",
        false,
    ),
    (
        "\u{ff27}\u{ff4f}\u{ff4f}\u{ff47}\u{ff4c}\u{ff45}\u{ff0e}\u{ff43}\u{ff4f}\u{ff4d}",
        "https://google.com",
        false,
    ),
    ("good.com", "https://\u{ff47}\u{ff4f}\u{ff4f}\u{ff44}\u{ff0e}\u{ff43}\u{ff4f}\u{ff4d}", true),
    ("\u{4f8b}\u{3048}.jp", "https://\u{4f8b}\u{3048}.jp", false),
    ("\u{4f8b}\u{3048}.jp", "https://xn--r8jz45g.jp", false),
    ("\u{4f8b}\u{3048}.jp", "https://evil.jp", true),
    ("\u{4f8b}\u{3048}.jp", "https://xn--r8jz45g.jp.evil.com", true),
    ("xn--r8jz45g.jp", "https://\u{4f8b}\u{3048}.jp", true),
    ("\u{4f8b}\u{3048}.jp", "https://\u{4f8b}\u{3048}.jp.evil.com", true),
    ("\u{dc}BER.de", "https://xn--ber-goa.de", false),
    ("u\u{308}ber.de", "https://xn--ber-goa.de", true),
    ("\u{4f8b}\u{3048}.\u{307f}\u{3093}\u{306a}", "https://evil.com", false),
    ("good.com", "https://\u{fb01}sh.com", true),
    ("fish.com", "https://\u{fb01}sh.com", true),
    ("paypal.com", "https://paypa1.com", true),
    ("paypal.com", "https://paypaI.com", true),
    ("google.c\u{43e}m", "https://google.c\u{43e}m", false),
    ("google.c\u{43e}m", "https://xn--google.cm-xyz", true),
    ("google\u{2024}com", "https://evil.com", true),
    ("google\u{fe52}com", "https://evil.com", true),
    ("google\u{30fb}com", "https://evil.com", false),
    ("1.2.3.4", "https://evil.1.2.3.4", true),
    ("1.2.3.4", "https://1.2.3.4/", false),
    ("good.com", "https://good.com%E3%80%82evil.com", true),
    ("good.com", "https://xn--good-xyz.com", true),
    ("github.io", "https://evil.github.io", true),
    ("github.io", "https://github.io", false),
    ("user.github.io", "https://docs.user.github.io", false),
    ("user.github.io", "https://evil.github.io", true),
    ("python.org", "https://docs.python.org/3/", false),
    ("github.com", "https://docs.github.com/x", false),
    ("co.uk", "https://evil.co.uk", true),
    ("example.co.uk", "https://docs.example.co.uk", false),
    ("amazonaws.com", "https://x.amazonaws.com", true),
    ("s3.amazonaws.com", "https://b.s3.amazonaws.com", true),
    ("surge.sh", "https://evil.surge.sh", true),
    ("x.surge.sh", "https://y.surge.sh", true),
    ("www.github.io", "https://evil.github.io", true),
    ("herokuapp.com", "https://evil.herokuapp.com", true),
    ("evil.herokuapp.com", "https://herokuapp.com", true),
    // the second pass: a post box, a share, a platform, a look-alike in capitals or in a path, an IDN top-level domain
    ("good.com", "http://good.com:80@evil.com", true),
    ("good.com", "http://evil.com\\.good.com", true),
    ("good.com", "https://good.com/%2e%2e/", false),
    ("good.com", "https://evil.com/..;/good.com", true),
    ("a@good.com", "mailto:a@good.com?subject=x%0d%0aBcc:b@evil.com", true),
    ("a@good.com", "mailto:a@good.com?body=Hello%0AThanks", false),
    ("a@good.com", "mailto:a@good.com;b@evil.com", true),
    ("a@good.com", "mailto:a@good.com,b@good.com", false),
    ("a@good.com", "mailto:a%2540evil.com", true),
    ("a@good.com", "mailto:a@good.com%0Abcc:x@evil.com", true),
    ("a@good.com", "mailto:\"a@good.com\"@evil.com", true),
    ("a@good.com", "mailto:a@www.good.com", false),
    ("a@good.com", "mailto:A@GOOD.COM", false),
    ("a@good.com", "mailto:a@good.com#frag", false),
    ("127.0.0.1", "http://127.0.0.1@evil.com", true),
    ("good.com", "foo://good.com@evil.com", true),
    ("good.com", "tel://evil.com", true),
    ("good.com", "\\\\evil.com\\share\\good.com", true),
    ("good.com", "file://evil.com/good.com", true),
    ("good.com", "file:////evil.com/share/good.com", true),
    ("good.com", "file:\\\\\\evil.com\\share", true),
    ("good.com", "smb://evil.com/share", true),
    ("good.com", "ms-appinstaller:?source=https://evil.com/x.appx", true),
    ("github.io", "https://evil.github.io.", true),
    ("GitHub.io", "https://evil.github.io", true),
    ("github.io/user", "https://evil.github.io", true),
    ("evil.github.io", "https://github.io", true),
    ("a.github.io", "https://a.github.io.evil.com", true),
    ("vercel.app", "https://x.vercel.app", true),
    ("githubusercontent.com", "https://raw.githubusercontent.com/x", true),
    ("raw.githubusercontent.com", "https://raw.githubusercontent.com/x", false),
    ("fly.io", "https://x.fly.io", false),
    ("8.8.8.8", "https://x.8.8.8.8", true),
    ("amazonaws.com.cn", "https://x.amazonaws.com.cn", false),
    ("g\u{43e}\u{43e}gle.com/login", "https://evil.com", true),
    ("g\u{43e}\u{43e}gle.com", "https://g\u{43e}\u{43e}gle.com/login", false),
    ("https://g\u{43e}\u{43e}gle.com/x", "https://evil.com", true),
    ("https://g\u{43e}\u{43e}gle.com/x", "https://g\u{43e}\u{43e}gle.com/y", false),
    ("\u{413}\u{41e}\u{41e}\u{413}\u{41b}\u{415}.com", "https://evil.com", true),
    ("caf\u{e9}.fr", "https://xn--caf-dma.fr", false),
    ("caf\u{e9}.fr", "https://evil.fr", true),
    ("g\u{43e}\u{43e}gle.\u{440}\u{444}", "https://evil.com", false),
    (
        "\u{ff48}\u{ff54}\u{ff54}\u{ff50}\u{ff53}\u{ff1a}\u{ff0f}\u{ff0f}good.com",
        "https://evil.com",
        true,
    ),
    ("good.com", "\u{ff48}\u{ff54}\u{ff54}\u{ff50}\u{ff53}://evil.com/good.com", false),
    ("good.com", "https\u{ff1a}//evil.com/good.com", false),
    ("good.com", "https://good.com:443", false),
    ("good.com", "https://good.com:443/", false),
];

#[test]
fn md_10_every_form_of_a_url_and_of_a_host_is_judged_fails_closed_or_is_out_of_scope() {
    let wrong: Vec<String> = FORMS
        .iter()
        .filter_map(|(text, target, lie)| {
            let got = marked(&format!("[{text}]({target})"));
            let want: Vec<String> = if *lie { vec![target.to_string()] } else { vec![] };
            (got != want).then(|| format!("[{text}]({target}): marked {got:?}, wanted {want:?}"))
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "{} of {} forms differ:\n{}",
        wrong.len(),
        FORMS.len(),
        wrong.join("\n")
    );
}

// ---- the files the highlighter calls Markdown ----

#[test]
fn md_10_every_name_that_the_highlighter_calls_markdown_has_the_marks_and_no_other_does() {
    // the names of the table of the highlighter's Markdown syntax (bat's: md, mdown, markdown, markdn)
    for name in ["a.md", "a.markdown", "a.mdown", "a.markdn", "A.MD", "dir/readme.markdown"] {
        assert!(syn::is_markdown(name), "{name}: the highlighter's Markdown");
        assert_eq!(doc_marks(name, "[good.com](https://evil.com)"), ["https://evil.com"], "{name}");
    }
    // a name that the table does not claim has no syntax, so no spans, so (as before) no marks
    for name in ["a.rs", "a.txt", "a", "a.py", "a.html", "a.json", "a.mkd", "a.ronn", "a.workbook"]
    {
        assert!(!syn::is_markdown(name), "{name}");
    }
}

#[test]
fn md_10_a_file_that_is_no_markdown_by_its_name_has_the_marks_where_the_highlighter_finds_links() {
    // the span-based trigger: a line the highlighter gives a link or a URL is looked at wherever it is
    let line = "[good.com](https://evil.com)";
    let lines = [line.to_string()];
    let mut syn = Syn::new("a.md", "").unwrap(); // (the spans of a Markdown file, for a file named otherwise)
    let sp = syn.spans(&|i| lines.get(i).cloned(), 0, 1);
    let cs: Vec<char> = line.chars().collect();
    let marks = line_marks(0, &cs, &sp[0], &|_| None, None, &mut Budget::frame());
    assert_eq!(
        marks.iter().map(|r| cs[r.clone()].iter().collect::<String>()).collect::<Vec<_>>(),
        ["https://evil.com"]
    );
    // and a line with no link or URL span is not looked at, in a file that is not Markdown
    let plain: Vec<char> = "let a = b[0](c);".chars().collect();
    assert!(line_marks(0, &plain, &[], &|_| None, None, &mut Budget::frame()).is_empty());
}

// ---- no hang, and fail closed past the caps ----

const N: usize = 500_000;

/// The shapes of a line that a detector with a linear look-up in a loop would take minutes on (their
/// time is quadratic in the length).
fn shapes() -> Vec<(&'static str, String)> {
    vec![
        ("[a] many times", "[a]".repeat(N / 3)),
        ("[ then ]", format!("{}{}", "[".repeat(N / 2), "]".repeat(N / 2))),
        ("](", "](".repeat(N / 2)),
        ("[a]( many times", "[a](".repeat(N / 4)),
        ("[a][b] many times", "[a][b]".repeat(N / 6)),
        ("[ then ](x)", format!("{}{}", "[".repeat(N / 5), "](x)".repeat(N / 5))),
        ("[good.com][1] many times", "[good.com][1]".repeat(N / 13)),
        ("[good.com](https://evil.com) many times", "[good.com](https://evil.com) ".repeat(N / 29)),
        ("<a href= many times", "<a href=".repeat(N / 8)),
        ("<a href=x> many times", "<a href=x>".repeat(N / 10)),
        ("backslashes", "\\".repeat(N)),
        ("backslashes before brackets", "\\\\\\[".repeat(N / 4)),
        ("entities", "&#46;".repeat(N / 5)),
        ("(", "(".repeat(N)),
        ("<", "<".repeat(N)),
        ("](<", "](<".repeat(N / 3)),
        ("![", "![".repeat(N / 2)),
        ("[ ![ ](", "[![](".repeat(N / 5)),
        ("blanks after (", format!("[a]({}", " ".repeat(N))),
        ("> many times", format!("{}[1]: x", "> ".repeat(N / 2))),
        ("[1]: many times", "[1]: x ".repeat(N / 7)),
        ("uses of a text of tags", format!("[{}][1]", "<a".repeat(400)).repeat(N / 804)),
        ("a long word of a text", format!("[{}](x)", "good.".repeat(N / 5))),
        (
            "a host of 990 distinct letters that are not ASCII (its punycode is quadratic)",
            format!(
                "[{}.com](https://x.com) ",
                (0x4e00..0x4e00 + 990).filter_map(char::from_u32).collect::<String>()
            )
            .repeat(N / 1_000),
        ),
        (
            "mailboxes in many",
            format!("[a@good.com](mailto:{}) ", "x@good.com,".repeat(80)).repeat(N / 900),
        ),
        ("a long target", format!("[good.com]({})", "a.".repeat(N / 2))),
        (
            "nested and long",
            format!("{}good.com{}(https://evil.com)", "[".repeat(N / 4), "]".repeat(N / 4)),
        ),
    ]
}

/// Whether `f` takes at most `limit`: the time of a debug build, with room for a loaded machine: a
/// quadratic look-up at these sizes takes minutes.
fn within(what: &str, limit: Duration, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < limit, "{what}: {took:?}");
}

#[test]
fn md_10_no_shape_of_a_line_makes_the_walk_hang() {
    for (what, line) in shapes() {
        let cs: Vec<char> = line.chars().collect();
        let doc = [line.clone(), "[1]: https://evil.com".to_string()];
        let refs = Arc::new(Refs::new(doc.len(), &|l| doc.get(l).cloned()));
        let get = || refs.clone();
        within(what, Duration::from_secs(4), || {
            let marks =
                line_marks(0, &cs, &[], &|l| doc.get(l).cloned(), Some(&get), &mut Budget::frame());
            std::hint::black_box(marks);
        });
    }
}

#[test]
fn md_10_no_shape_of_a_document_makes_the_scan_for_references_hang() {
    for (what, line) in shapes() {
        // many lines of the shape, each as long as a line is read (10,000 chars), and the lines before them
        let one: String = line.chars().take(10_000).collect();
        let doc: Vec<String> = (0..2_500)
            .map(|i| if i % 7 == 0 { format!("[{i}]: https://evil.com") } else { one.clone() })
            .collect();
        within(what, Duration::from_secs(4), || {
            std::hint::black_box(Refs::new(doc.len(), &|l| doc.get(l).cloned()));
        });
    }
}

#[test]
fn md_10_no_shape_of_a_frame_of_long_lines_makes_the_walk_hang() {
    // a screen of lines that are as long as a line is read, each of the shape and each with a paragraph around
    // it that the walk has to read (a bracket that closes nothing sends it there): the work of the window is
    // in the budget of the frame too
    for (what, line) in shapes() {
        let one: String = line.chars().take(10_000).collect();
        let doc: Vec<String> = (0..2_500)
            .map(|i| if i % 7 == 0 { format!("[{i}]: https://evil.com") } else { one.clone() })
            .collect();
        let refs = Arc::new(Refs::new(doc.len(), &|l| doc.get(l).cloned()));
        let get = || refs.clone();
        let mut budget = Budget::frame();
        within(what, Duration::from_secs(4), || {
            for l in 0..60 {
                let cs: Vec<char> = doc[l].chars().collect();
                let marks =
                    line_marks(l, &cs, &[], &|k| doc.get(k).cloned(), Some(&get), &mut budget);
                std::hint::black_box(marks);
            }
        });
    }
}

#[test]
fn md_10_a_paragraph_that_the_budget_cannot_read_marks_its_line() {
    // a bracket that closes nothing means a link that may go over lines; with a budget that cannot read the
    // paragraph the line is not trusted, and with one that can it is judged (and the link is found)
    let doc = ["see [good.com", "](https://evil.com)"].map(String::from);
    let cs: Vec<char> = doc[0].chars().collect();
    let marks = |b: &mut Budget| {
        line_marks(
            0,
            &cs,
            &[],
            &|k| doc.get(k).cloned(),
            Some(&|| Arc::new(Refs::new(0, &|_| None))),
            b,
        )
    };
    assert!(!marks(&mut Budget::new(512, 200_000)).is_empty(), "the link over the lines lies");
    assert_eq!(marks(&mut Budget::new(512, 10)), vec![0..cs.len()], "the paragraph is not read");
}

#[test]
fn md_10_a_line_that_is_longer_than_it_is_read_has_what_is_past_the_cap_marked_if_it_has_a_bracket()
{
    let cap = syn::MAX_LINE;
    let mut line = "x".repeat(cap + 100);
    line.push_str("[good.com](https://evil.com)");
    let cs: Vec<char> = line.chars().collect();
    let marks = line_marks(
        0,
        &cs,
        &[],
        &|_| None,
        Some(&|| Arc::new(Refs::new(0, &|_| None))),
        &mut Budget::frame(),
    );
    assert!(
        marks.iter().any(|r| r.start == cap && r.end == cs.len()),
        "the part past the cap: {marks:?}"
    );
    // a link that starts before the cap and ends past it
    let mut line = "x".repeat(cap - 3);
    line.push_str("[good.com](https://evil.com)");
    let cs: Vec<char> = line.chars().collect();
    let marks = line_marks(
        0,
        &cs,
        &[],
        &|_| None,
        Some(&|| Arc::new(Refs::new(0, &|_| None))),
        &mut Budget::frame(),
    );
    assert!(marks.iter().any(|r| r.end == cs.len()), "{marks:?}");
    // a line that is long and has no bracket past the cap has no mark (a data URI of an image is one)
    let mut line = "x".repeat(cap + 100);
    line.push_str(&"A".repeat(100_000));
    let cs: Vec<char> = line.chars().collect();
    assert!(
        line_marks(
            0,
            &cs,
            &[],
            &|_| None,
            Some(&|| Arc::new(Refs::new(0, &|_| None))),
            &mut Budget::frame()
        )
        .is_empty()
    );
}

#[test]
fn md_10_a_frame_that_has_more_to_judge_than_its_budget_marks_the_rest_and_does_not_slow_down() {
    // 3,000 honest links on lines that a frame is asked about: the budget of a frame judges some
    // and the rest are marked, not trusted; and the time is bounded
    let line = "[a.com](https://a.com) ".repeat(40);
    let cs: Vec<char> = line.chars().collect();
    let mut budget = Budget::frame();
    let mut marked = 0;
    within("a frame of links", Duration::from_secs(3), || {
        for l in 0..75 {
            let marks = line_marks(
                l,
                &cs,
                &[],
                &|_| None,
                Some(&|| Arc::new(Refs::new(0, &|_| None))),
                &mut budget,
            );
            marked += marks.len();
        }
    });
    assert!(marked > 0, "past the budget a link is marked, not trusted");
    // one line of such links is judged whole in a fresh frame
    let mut budget = Budget::frame();
    assert!(
        line_marks(
            0,
            &cs,
            &[],
            &|_| None,
            Some(&|| Arc::new(Refs::new(0, &|_| None))),
            &mut budget
        )
        .is_empty()
    );
}

// ---- release ----

/// MD-10: a line of a megabyte of each shape, and 2,000 lines of 10,000 chars, in a release build.
///
///   cargo test --release -p sim-editor --test md_link_class -- --ignored md_10_bench --nocapture
#[test]
#[ignore]
fn md_10_bench_a_line_of_a_megabyte_and_a_document_of_long_lines() {
    release_only();
    let mut worst = Duration::ZERO;
    for (what, line) in shapes() {
        let big: String = line.chars().cycle().take(1 << 20).collect();
        let cs: Vec<char> = big.chars().collect();
        let doc = [big.clone(), "[1]: https://evil.com".to_string()];
        let refs = Arc::new(Refs::new(doc.len(), &|l| doc.get(l).cloned()));
        let get = || refs.clone();
        let t0 = Instant::now();
        std::hint::black_box(line_marks(
            0,
            &cs,
            &[],
            &|l| doc.get(l).cloned(),
            Some(&get),
            &mut Budget::frame(),
        ));
        let line_time = t0.elapsed();
        let one: String = big.chars().take(10_000).collect();
        let docs: Vec<String> = (0..2_000)
            .map(|i| if i % 7 == 0 { format!("[{i}]: https://evil.com") } else { one.clone() })
            .collect();
        let t0 = Instant::now();
        let refs = Arc::new(Refs::new(docs.len(), &|l| docs.get(l).cloned()));
        let doc_time = t0.elapsed();
        // the frame of that document: 60 visible lines, one budget
        let get = || refs.clone();
        let mut budget = Budget::frame();
        let t0 = Instant::now();
        for l in 0..60 {
            let cs: Vec<char> = docs[l].chars().collect();
            std::hint::black_box(line_marks(
                l,
                &cs,
                &[],
                &|l| docs.get(l).cloned(),
                Some(&get),
                &mut budget,
            ));
        }
        let frame_time = t0.elapsed();
        // and a screen of ten lines of a megabyte
        let mega: Vec<String> = (0..12).map(|_| big.clone()).collect();
        let mut budget = Budget::frame();
        let t0 = Instant::now();
        for l in 0..10 {
            let cs: Vec<char> = mega[l].chars().collect();
            let marks = line_marks(l, &cs, &[], &|k| mega.get(k).cloned(), Some(&get), &mut budget);
            std::hint::black_box(marks);
        }
        let mega_time = t0.elapsed();
        eprintln!(
            "MD-10 {what:<40} 1 MB line {line_time:>9.2?}  10 of them {mega_time:>9.2?}  2,000 lines of 10,000 chars: scan {doc_time:>9.2?}, frame of 60 {frame_time:>9.2?}"
        );
        worst = worst.max(line_time).max(doc_time).max(frame_time).max(mega_time);
    }
    eprintln!("MD-10 worst {worst:?}");
    assert!(worst < Duration::from_millis(50), "{worst:?}");
}

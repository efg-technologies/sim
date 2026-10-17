//! MD-10: a link whose text looks like an address that its target is not is marked. The judgment
//! (pure), which links of a line it is made of, and the screens: the target is the alert's block in
//! a line of the read view and in a raw one, inverse and bold with no color, dim and on the screen
//! in calm too (as a hidden char is), and nothing of the link is ever left out. The bypasses a parser
//! of another reading could find: the authority as a browser reads it, the text as CommonMark finds it,
//! every word of a text, a reference link, a country code.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Syntax, Tones};
use sim::layout::Skin;
use sim::lie::{Budget, Refs, deceptive, deceptive_targets, line_marks};
use sim::paint::{Painter, Screen};
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};
use std::sync::Arc;

const BG: Rgb = (0x1e, 0x1e, 0x1e);

#[test]
fn md_10_a_text_that_is_an_address_other_than_the_target_is_deceptive() {
    let lies = [
        ("https://good.example/docs", "https://evil.example/login"),
        ("good.example", "https://evil.example"),
        ("www.good.example", "https://evil.example"),
        ("docs.example.org/guide", "https://example.org/guide"),
        ("example.com", "https://example.com.evil.example/"),
        ("https://good.example", "https://good.example@evil.example/"),
        ("https://good.example", "//evil.example"),
        ("example.com", "/redirect?to=evil"),
        ("https://good.example", "javascript:alert(1)"),
        ("good.example", "data:text/html;base64,AAAA"),
        ("good.example", "VBScript:run"),
        ("/etc/hosts", "https://evil.example/etc/passwd"),
        ("/etc/passwd", "https://evil.example/passwd"),
        ("~/notes.md", "https://evil.example/home/notes.md"),
        ("docs/guide/intro.md", "https://example.com/intro.md"),
        ("./src/main.rs", "../other/file.rs"),
        ("src/main.rs", "https://github.com/x/y/blob/main/src/lib.rs"),
        ("**https://good.example**", "https://evil.example"),
        ("`good.example`", "https://evil.example"),
        ("HTTPS://GOOD.EXAMPLE", "https://evil.example"),
    ];
    for (text, target) in lies {
        assert!(deceptive(text, target), "[{text}]({target}) is deceptive");
    }
}

#[test]
fn md_10_the_same_address_a_name_and_a_phrase_are_not() {
    let honest = [
        ("docs", "https://example.com/docs"),
        ("click here", "https://evil.example"),
        ("Node.js", "https://nodejs.org"),
        ("main.rs", "https://example.com/src/main.rs"),
        ("README.md", "README.md"),
        ("Cargo.toml", "https://github.com/x/y/blob/main/Cargo.toml"),
        ("v1.2.3", "https://example.com/releases/1.2.3"),
        ("and/or", "https://example.com/logic"),
        ("e.g.", "https://example.com/eg"),
        ("example.com", "https://example.com/"),
        ("example.com", "https://www.example.com"),
        ("www.example.com", "http://example.com:8080/x"),
        ("https://example.com/a", "https://example.com/b"),
        ("https://example.com", "https://user@example.com/"),
        ("example.com/docs", "https://example.com/docs/intro"),
        ("src/main.rs", "../src/main.rs"),
        ("src/main.rs", "https://github.com/x/y/blob/main/src/main.rs"),
        ("./src/main.rs", "src/main.rs"),
        ("/etc/hosts", "/etc/hosts"),
        ("~/notes.md", "notes.md"),
        ("mail me", "mailto:me@example.com"),
        ("me@example.com", "mailto:me@example.com"),
        ("", "https://example.com"),
        ("https://example.com", ""),
    ];
    for (text, target) in honest {
        assert!(!deceptive(text, target), "[{text}]({target}) is not deceptive");
    }
}

/// The targets of the line that are marked, as text.
fn marked(line: &str) -> Vec<String> {
    let cs: Vec<char> = line.chars().collect();
    deceptive_targets(&cs).into_iter().map(|r| cs[r].iter().collect()).collect()
}

#[test]
fn md_10_the_targets_of_the_inline_links_of_a_line_that_are_deceptive_are_found() {
    let line = "see [https://a.example/x](https://b.example/y) and [docs](https://c.example) and \
                [d.example](https://e.example)";
    assert_eq!(marked(line), ["https://b.example/y", "https://e.example"]);
    // markers and code in the text; a title after the target
    assert_eq!(
        marked("[**https://a.example**](https://b.example \"t\")"),
        ["https://b.example"],
        "the markers of the text are not the text, the title is not the target"
    );
    assert_eq!(marked("[`a.example`](https://b.example)"), ["https://b.example"]);
    assert_eq!(marked("[https://a.example](<https://b.example>)"), ["<https://b.example>"]);
    assert_eq!(
        marked("[a.example](https://b.example/x_(y)) then [c.example](https://d.example)"),
        ["https://b.example/x_(y)", "https://d.example"],
        "a parenthesis in the target is part of it, and the next link is found"
    );
    // an image's text is not clicked, a reference's target is a line of its own, an autolink has no text
    assert_eq!(marked("![https://a.example](https://b.example/i.png)"), Vec::<String>::new());
    assert_eq!(marked("[https://a.example][ref] <https://b.example>"), Vec::<String>::new());
    assert_eq!(
        marked("no link here, only https://a.example and (https://b.example)"),
        Vec::<String>::new()
    );
    assert_eq!(marked(""), Vec::<String>::new());
}

/// Every pair of the table that is judged the other way, at once.
fn judged(table: &[(&str, &str)], want: bool) {
    let wrong: Vec<String> = table
        .iter()
        .filter(|(text, target)| deceptive(text, target) != want)
        .map(|(text, target)| format!("[{text}]({target:?})"))
        .collect();
    let what = if want { "deceptive" } else { "honest" };
    assert!(wrong.is_empty(), "judged not {what}:\n{}", wrong.join("\n"));
}

fn lies(table: &[(&str, &str)]) {
    judged(table, true);
}

fn honest(table: &[(&str, &str)]) {
    judged(table, false);
}

#[test]
fn md_10_the_authority_is_read_as_a_browser_reads_it_for_the_special_schemes() {
    lies(&[
        // a backslash ends the authority (WHATWG), so the host is evil.com, whatever follows the `@`
        ("good.com", "https://evil.com\\@good.com/"),
        ("good.com", "https://evil.com\\.good.com/"),
        ("https://good.com", "https://evil.com\\@good.com"),
        // slashes after the scheme are any number of `/` and `\`, or none
        ("good.com", "https:/\\evil.com"),
        ("good.com", "https:\\\\evil.com"),
        ("good.com", "https:evil.com"),
        // fewer than two slashes after a special scheme is resolved against a base: the document's own host,
        // or this one where there is none; not known, so not honest even for the host in the text
        ("good.com", "https:good.com"),
        ("good.com", "https:/good.com"),
        // (in a Markdown target `\\` is an escaped backslash: `https:\good.com`, one slash)
        ("good.com", "https:\\\\good.com\\path"),
        ("good.com", "https:///evil.com"),
        ("good.com", "http:/evil.com/x"),
        ("good.com", "wss:evil.com"),
        ("good.com", "ftp:\\\\evil.com"),
        ("good.com", "HTTPS:EVIL.COM"),
        // no scheme: a scheme-relative target, with either slash
        ("good.com", "//evil.com"),
        ("good.com", "\\\\evil.com"),
        ("good.com", "/\\evil.com"),
        ("good.com", "\\/evil.com"),
        // the `#` and the `?` end the authority too
        ("good.com", "https://evil.com#@good.com"),
        ("good.com", "https://evil.com?@good.com"),
        // tab and newline are dropped from a URL before it is read
        ("good.com", "https://evil\t.com"),
        ("good.com", "https://good.com\n.evil.com"),
        ("good.com", "ht\ttps://evil.com"),
    ]);
    honest(&[
        ("good.com", "https://user:pw@good.com:8080/x?y#z"),
        ("good.com", "https://evil.com:80@good.com"),
        ("good.com", "https://good.com\\path"),
        ("good.com", "HTTPS://WWW.GOOD.COM./"),
        ("good.com", "https://go\tod.com"),
        ("good.com", "//good.com/x"),
        ("good.com", "\\\\good.com"),
        ("good.com", "https://@good.com"),
    ]);
}

#[test]
fn md_10_anything_odd_in_a_hosts_authority_is_deceptive_when_the_text_names_a_host() {
    lies(&[
        ("good.com", "https://good.com%2f.evil.com/"),
        ("good.com", "https://good%2ecom.evil.com/"),
        ("good.com", "https://good.com\u{ff0e}evil.com/"),
        ("good.com", "https://g\u{43e}od.com/"),
        ("good.com", "https://good.com\u{0}.evil.com/"),
        ("good.com", "https://good.com\u{7f}/"),
        ("good.com", "https://good.com .evil.com/"),
        ("good.com", "https://good.com\u{a0}.evil.com/"),
        ("good.com", "https://good.com&#46;evil.com/"),
        ("good.com", "https://good.com:80x/"),
        ("good.com", "https://good.com:x/"),
        ("good.com", "https://[::1]/"),
        ("good.com", "foo://good.com\\@x/"),
        ("good.com", "https://xn--gd-9ka.com/"),
        ("good.com", "https://\u{ff47}ood.com/"),
        ("good.com", "&#104;ttps://evil.com/"),
        ("good.com", "https&colon;//evil.com"),
        ("good.com", "https:&#47;&#47;evil.com"),
    ]);
    // a path is judged by its own rule: an odd authority does not make a path text lie by itself
    honest(&[("src/main.rs", "../src/main.rs")]);
}

#[test]
fn md_10_a_target_that_runs_code_is_found_through_tabs_blanks_entities_and_case() {
    lies(&[
        ("click here", "javascript:alert(1)"),
        ("click here", "java\tscript:alert(1)"),
        ("click here", "java\nscript:alert(1)"),
        ("click here", " \u{1}javascript:alert(1)"),
        ("click here", "JaVaScRiPt:alert(1)"),
        ("click here", "javascript&colon;alert(1)"),
        ("click here", "&#106;avascript:alert(1)"),
        ("click here", "&#x6A;avascript:alert(1)"),
        ("click here", "data&colon;text/html,x"),
        ("click here", "vbscript&Tab;:x"),
    ]);
    honest(&[
        ("docs", "a?x=1&y=2;z"),
        ("docs", "https://example.com/a?x=1&y=2;z#h"),
        ("docs", "notes&more.md"),
        // not a scheme to a browser either: a relative path with an odd name
        ("click here", "javascript%3Aalert(1)"),
        ("click here", "java\\script:alert(1)"),
    ]);
}

#[test]
fn md_10_every_address_of_a_text_is_judged_and_the_words_are_not() {
    lies(&[
        ("Visit good.com", "https://evil.com"),
        ("good.com (official)", "https://evil.com"),
        ("Visit https://good.com now", "https://evil.com"),
        ("the site, good.com, is down", "https://evil.com"),
        ("Download **good.com/setup** now", "https://evil.com/setup"),
        ("good.com or evil.com", "https://evil.com"),
        ("see src/main.rs", "https://evil.com/x.rs"),
        ("open /etc/hosts", "https://evil.com/hosts"),
        ("on the web at www.good.com.", "https://evil.com"),
    ]);
    honest(&[
        ("Visit good.com", "https://good.com/start"),
        ("good.com (official)", "https://www.good.com"),
        ("the docs at docs.python.org", "https://docs.python.org/3/"),
        ("good.com or www.good.com", "https://good.com"),
        ("see src/main.rs for details", "https://github.com/x/y/blob/main/src/main.rs"),
        ("Click here for the docs", "https://example.com/docs"),
        ("version 1.2.3 and e.g. this", "https://example.com/v"),
        ("Node.js and Vue.js", "https://nodejs.org"),
    ]);
}

#[test]
fn md_10_a_text_is_read_as_a_reader_reads_it_and_not_as_a_tokenizer_splits_it() {
    lies(&[
        // a port, an email, an IP address, localhost, the generic domains that are not on a short list
        ("good.com:8080", "https://evil.com"),
        ("good.com:8080/x", "https://evil.com"),
        ("a@good.com", "https://evil.com"),
        ("mailto:a@good.com", "https://evil.com"),
        ("a@good.com", "mailto:a@evil.com"),
        ("192.168.1.1", "https://evil.com"),
        ("192.168.1.1:8080/x", "https://evil.com"),
        ("localhost:3000", "https://evil.com"),
        ("good.page", "https://evil.com"),
        ("good.cloud", "https://evil.com"),
        ("good.online", "https://evil.com"),
        ("good.onion", "https://evil.com"),
        ("good.local", "https://evil.com"),
        // punctuation that is not ASCII, and ASCII that is no part of an address, next to it
        ("\u{201c}good.com\u{201d}", "https://evil.com"),
        ("\u{ab}good.com\u{bb}", "https://evil.com"),
        ("\u{ff08}good.com\u{ff09}", "https://evil.com"),
        ("good.com\u{2014}official", "https://evil.com"),
        ("good.com\u{2026}", "https://evil.com"),
        ("good.com|official", "https://evil.com"),
        ("official;good.com", "https://evil.com"),
        ("good.com,evil.com", "https://evil.com"),
        ("(good.com)", "https://evil.com"),
    ]);
    honest(&[
        ("a@good.com", "mailto:a@good.com"),
        ("mailto:a@good.com", "mailto:a@good.com"),
        ("good.com:8080", "https://good.com:8080/x"),
        ("localhost:3000", "http://localhost:3000/x"),
        ("192.168.1.1", "http://192.168.1.1/x"),
        ("192.168.1.1:8080", "http://192.168.1.1:8080"),
        ("\u{201c}good.com\u{201d}", "https://good.com"),
        ("good.com\u{2014}official", "https://good.com/x"),
        ("version 1.2.3.4.5", "https://example.com"),
        ("an email me@ and a@b", "https://example.com"),
    ]);
}

#[test]
fn md_10_a_country_code_names_a_host_and_a_file_name_does_not() {
    lies(&[
        ("paypal.com.mx", "https://evil.example"),
        ("example.kr", "https://evil.example"),
        ("shop.co.uk", "https://evil.example"),
        ("bank.ch", "https://evil.example"),
        ("www.good.zz", "https://evil.example"),
        ("good.de/login", "https://evil.example/login"),
        ("example.kr", "https://evil.example/example.kr"),
        ("example.kr", "docs/other.md"),
    ]);
    honest(&[
        ("main.rs", "src/main.rs"),
        ("README.md", "https://github.com/x/y/blob/main/README.md"),
        ("README.md", "../docs/README.md"),
        ("Makefile.am", "build/Makefile.am"),
        ("configure.ac", "https://example.com/configure.ac"),
        ("Node.js", "https://nodejs.org"),
        ("script.py", "https://example.com/dl/script.py"),
        ("setup.sh", "https://example.com/setup.sh"),
        ("main.cc", "src/main.cc"),
        ("a.so", "lib/a.so"),
        ("parse.pl", "tools/parse.pl"),
        ("lib.ml", "src/lib.ml"),
        ("config.h.in", "build/config.h.in"),
        // a file of the repository that a relative target names is honest whatever its last label
        ("example.kr", "docs/example.kr"),
        ("v1.2", "releases/v1.2"),
        ("e.g.", "https://example.com/eg"),
    ]);
}

#[test]
fn md_10_the_text_of_a_link_is_found_with_its_brackets_nested_and_escaped() {
    assert_eq!(marked("[good.com [x]](https://evil.com)"), ["https://evil.com"]);
    assert_eq!(marked("[[good.com]](https://evil.com)"), ["https://evil.com"]);
    assert_eq!(marked("[good.com [a [b]] c](https://evil.com)"), ["https://evil.com"]);
    assert_eq!(marked("[good.com \\[x\\]](https://evil.com)"), ["https://evil.com"]);
    assert_eq!(marked("[good.com \\](https://evil.com)"), Vec::<String>::new(), "no `](` here");
    assert_eq!(
        marked("[a [x]](https://good.com) [good.com [y]](https://good.com)"),
        Vec::<String>::new()
    );
    assert_eq!(
        marked("see [one [x]](u) and [good.com [y]](https://evil.com) and [two](v)"),
        ["https://evil.com"]
    );
    assert_eq!(marked("[Visit good.com](https://evil.com)"), ["https://evil.com"]);
    assert_eq!(
        marked("[good.com](https://evil.com\\@good.com/)"),
        ["https://evil.com\\@good.com/"]
    );
}

#[test]
fn md_10_an_angle_target_may_hold_blanks_and_tabs_and_is_read_whole() {
    assert_eq!(marked("[good.com](<https://evil.com >)"), ["<https://evil.com >"]);
    // (the tab goes: the host is good.com.evil.com; the other way round it is a subdomain of good.com)
    assert_eq!(
        marked("[good.com](<https://good.com\t.evil.com>)"),
        ["<https://good.com\t.evil.com>"]
    );
    assert_eq!(marked("[good.com](<https://evil.com\t.good.com>)"), Vec::<String>::new());
    assert_eq!(marked("[good.com](<https://good.com/a b>)"), Vec::<String>::new());
    assert_eq!(
        marked("[good.com](<https://good.com\t/a>) [x.com](<https://evil.com>)"),
        ["<https://evil.com>"]
    );
}

#[test]
fn md_10_a_document_that_is_longer_than_the_scan_marks_what_cannot_be_resolved() {
    use sim::lie::REF_LINES;
    let mut lines = vec!["A [good.com][far] and [docs][far] and [good.com].".to_string()];
    lines.extend((1..REF_LINES + 20).map(|i| format!("filler {i}")));
    lines.push("[far]: https://evil.com".into());
    lines.insert(1, "[near]: https://good.com".into());
    let refs = Arc::new(Refs::new(lines.len(), &|l| lines.get(l).cloned()));
    let get = || refs.clone();
    let cs: Vec<char> = lines[0].chars().collect();
    let text = |l: usize| lines.get(l).cloned();
    let marks = line_marks(0, &cs, &[], &text, Some(&get), &mut Budget::frame());
    let said: Vec<String> = marks.into_iter().map(|r| cs[r].iter().collect()).collect();
    assert_eq!(
        said,
        ["[good.com]", "[good.com]"],
        "past the scan a reference is unresolved, and an address that is not resolved is marked"
    );
    // a definition that no use of the part that was looked through makes lie is not marked (a CHANGELOG of
    // thousands of lines has hundreds): the use that a definition past the scan could make lie is marked
    // where it is drawn, as above
    let near: Vec<char> = lines[1].chars().collect();
    let marks = line_marks(1, &near, &[], &text, Some(&get), &mut Budget::frame());
    assert!(marks.is_empty(), "{marks:?}");
}

#[test]
fn md_10_the_text_and_the_target_are_judged_as_they_read_after_escapes_and_entities() {
    lies(&[
        // what a reader sees in the text: a backslash escape and an entity are decoded
        ("good&#46;com", "https://evil.com"),
        ("good&#x2e;com", "https://evil.com"),
        ("good&period;com", "https://evil.com"),
        ("good\\.com", "https://evil.com"),
        ("https&colon;//good.com", "https://evil.com"),
        // what a renderer makes of the target: `\@` is `@`, so the host is what follows it
        ("good.com", "https://good.com\\@evil.com/"),
        ("good.com", "https://good.com&#64;evil.com/"),
        ("good&#46com", "https://evil.com"),
        ("good&#x2ecom", "https://evil.com"),
        ("good&nbsp;.com", "https://evil.com"),
        ("good.com", "https://evil.com\\.good.com/"),
    ]);
    honest(&[
        ("Q&A about it", "https://example.com"),
        ("R&D docs &copy; 2024", "https://example.com"),
        ("good.com", "https://good.com/a\\_b"),
        ("good.com", "https://good.com/a&amp;b"),
    ]);
}

#[test]
fn md_10_blanks_after_the_opening_parenthesis_do_not_hide_the_target() {
    assert_eq!(marked("[good.com]( https://evil.com )"), ["https://evil.com"]);
    assert_eq!(marked("[good.com](\thttps://evil.com\t\"title\")"), ["https://evil.com"]);
    assert_eq!(marked("[good.com]( <https://evil.com> )"), ["<https://evil.com>"]);
    assert_eq!(marked("[good.com]( https://good.com )"), Vec::<String>::new());
}

#[test]
fn md_10_a_wrapped_link_that_lies_is_marked_on_the_screen_where_it_is_drawn() {
    let text = "Intro line.\n\nSee [good.example\nofficial](https://evil.example/one) now.\n\nAnd [good.example](\nhttps://evil.example/two) too.\n\nEnd.\n";
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md10-wrap", skin, false, text);
        let out = draw(r.shot.clone());
        for url in ["https://evil.example/one", "https://evil.example/two"] {
            assert_eq!(seq(&out, url), block(), "{skin:?}: {url}");
        }
        let y =
            r.shot.frame.rows.iter().position(|x| x.text.contains("And [good.example](")).unwrap();
        let x = r.shot.frame.rows[y].text.find("[good.example]").unwrap() as u16;
        let (back, on) = Tones::new(&palette(BG)).alert_block();
        let sealed = r.shot.paints[y].iter().any(|(c, p)| {
            c.contains(&(x + 1)) && p.sealed && p.bg == Some(back) && p.fg == Some(on)
        });
        assert!(sealed, "{skin:?}: the text, where the target is on the next line");
    }
    let r = shown("md10-wrap-calm", Skin::Calm, false, text);
    let out = draw(r.shot.clone());
    assert_eq!(seq(&out, "https://evil.example/one"), calm_token());
}

#[test]
fn md_10_no_line_of_pieces_that_a_parser_would_trip_on_makes_the_judgment_panic() {
    const PIECES: [&str; 44] = [
        "[",
        "]",
        "](",
        "[]",
        "](<",
        ">)",
        "<",
        ">",
        "(",
        ")",
        "\\",
        "\\@",
        "@",
        ":",
        "://",
        "//",
        "/\\",
        "https",
        "javascript",
        "&",
        "&#46;",
        "&colon;",
        "&shy;",
        "%",
        "%2f",
        ".",
        "good.com",
        "evil.kr",
        "www.",
        " ",
        "\t",
        "\n",
        "!",
        "^",
        "日本",
        "\u{202e}",
        "\u{ff0e}",
        "\u{0}",
        "x",
        "1",
        "#",
        "<a ",
        "href=",
        "</a>",
    ];
    let mut rng = Rng(0x9e37_79b9);
    for _ in 0..4000 {
        let n = rng.below(16) + 1;
        let text: String = (0..n).map(|_| PIECES[rng.below(PIECES.len())]).collect();
        let (a, b) = text.split_at(text.char_indices().nth(rng.below(n)).map_or(0, |p| p.0));
        let _ = (deceptive(a, b), deceptive(b, a));
        let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        let refs = Arc::new(Refs::new(lines.len(), &|l| lines.get(l).cloned()));
        let get = || refs.clone();
        let mut budget = Budget::frame();
        for (l, line) in lines.iter().enumerate() {
            let cs: Vec<char> = line.chars().collect();
            let _ = deceptive_targets(&cs);
            let _ = line_marks(l, &cs, &[], &|k| lines.get(k).cloned(), Some(&get), &mut budget);
        }
    }
}

fn draw(s: Screen) -> String {
    let mut p = Painter::new(Vec::new());
    p.draw(s).unwrap();
    String::from_utf8(p.out).unwrap()
}

/// The SGR sequence in force at the first `what` of `out`.
fn seq(out: &str, what: &str) -> String {
    assert!(out.contains(what), "{what:?} is not drawn in {out:?}");
    let i = out.find(what).unwrap();
    format!("\x1b[{}", out[..i].rsplit("\x1b[").next().unwrap())
}

fn shown(name: &str, skin: Skin, mono: bool, text: &str) -> Rig {
    let mut r = rig_env(name, &[&format!("a.md={text}")], &["a.md"], (80, 12), |e| {
        e.skin = skin;
        e.chrome = true;
        e.md = true;
        e.syn = if mono { Syntax::mono() } else { Syntax::new(true) };
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r.deliver_spans();
    r
}

/// A file whose first line is where the cursor is (so it is raw) and whose third line is read.
const LINKS: &str = "[https://good.example/docs](https://evil.example/login) raw\n\n\
                     [https://good.example/docs](https://evil.example/login) **read**\n\
                     [docs](https://fine.example/docs) and [fine.example](https://fine.example/x)\n";

/// The alert's block as flat and depth write it: its own text on its surface, nothing else.
fn block() -> String {
    let t = Tones::new(&palette(BG));
    let (back, text) = t.alert_block();
    format!("\x1b[0;38;2;{};{};{};48;2;{};{};{}m", text.0, text.1, text.2, back.0, back.1, back.2)
}

#[test]
fn md_10_the_target_is_the_alert_s_block_in_a_raw_line_and_in_a_read_one() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md10-block", skin, false, LINKS);
        let out = draw(r.shot.clone());
        let hits: Vec<_> = out.match_indices("https://evil.example/login").collect();
        assert_eq!(hits.len(), 2, "{skin:?}: the raw line and the read one");
        for (i, _) in hits {
            let sgr = format!("\x1b[{}", out[..i].rsplit("\x1b[").next().unwrap());
            assert_eq!(sgr, block(), "{skin:?}: the target of a link that lies");
        }
        // what is honest stays as UI-8 draws it: not the alert's
        for fine in ["https://fine.example/docs", "https://fine.example/x"] {
            assert_ne!(seq(&out, fine), block(), "{skin:?}: {fine}");
        }
    }
}

#[test]
fn md_10_the_read_view_hides_no_char_of_such_a_link() {
    let r = shown("md10-whole", Skin::Depth, false, LINKS);
    let rows = &r.shot.frame.rows;
    let line = "[https://good.example/docs](https://evil.example/login) read";
    assert!(rows.iter().any(|x| x.text.contains(line)), "{:#?}", r.editor());
    assert!(
        rows.iter().any(|x| x
            .text
            .contains("[https://good.example/docs](https://evil.example/login) raw"))
    );
}

#[test]
fn md_10_without_color_the_target_is_inverse_and_bold() {
    let r = shown("md10-mono", Skin::Depth, true, LINKS);
    let out = draw(r.shot.clone());
    let sgr = seq(&out, "https://evil.example/login");
    assert!(sgr.contains(";7") && sgr.contains(";1"), "inverse and bold: {sgr:?}");
    let fine = seq(&out, "https://fine.example/docs");
    assert!(!fine.contains(";7"), "an honest target is not: {fine:?}");
    assert!(!out.contains("38;") && !out.contains("48;"), "and no color at all: {out:?}");
}

/// What the Painter writes for a hidden char in calm: inverse, the primary gray.
fn calm_token() -> String {
    let p = palette(BG).primary;
    format!("\x1b[0;7;38;2;{};{};{}m", p.0, p.1, p.2)
}

#[test]
fn md_10_calm_marks_the_target_as_it_marks_a_hidden_char_and_the_link_is_whole() {
    let r = shown("md10-calm", Skin::Calm, false, LINKS);
    let out = draw(r.shot.clone());
    assert!(out.contains("https://evil.example/login"), "{out:?}");
    assert_eq!(
        seq(&out, "https://evil.example/login"),
        calm_token(),
        "the target of a link that lies"
    );
    // the raw line and the read one, and not the honest ones
    assert_eq!(out.matches(&format!("{}https://evil.example/login", calm_token())).count(), 2);
    for fine in ["https://fine.example/docs", "https://fine.example/x"] {
        assert_ne!(seq(&out, fine), calm_token(), "{fine}");
    }
    let rows = &r.shot.frame.rows;
    assert!(
        rows.iter().any(|x| x
            .text
            .contains("[https://good.example/docs](https://evil.example/login) read"))
    );
}

const REF_DOC: &str = "Go to [good.example][1] now, or [good.example/docs][docs].\n\n[1]: https://evil.example/login\n[docs]: https://good.example/docs\nfiller\n";

#[test]
fn md_10_a_reference_link_has_its_text_and_its_definition_marked_where_each_is_drawn() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md10-ref", skin, false, &format!("{REF_DOC}\nlast\n"));
        let out = draw(r.shot.clone());
        // the definition's target: the alert's block; the one that says what it is: not
        assert_eq!(seq(&out, "https://evil.example/login"), block(), "{skin:?}: the target");
        assert_ne!(seq(&out, "https://good.example/docs"), block(), "{skin:?}: an honest one");
        // the use: its text and brackets, `[good.example]`, on the line that has the cursor too (it is raw)
        let line = r
            .shot
            .frame
            .rows
            .iter()
            .position(|x| x.text.contains("Go to [good.example][1]"))
            .unwrap();
        let col = |x: &str| {
            r.shot.frame.rows[line].text[..r.shot.frame.rows[line].text.find(x).unwrap()]
                .chars()
                .count() as u16
        };
        let (back, text) = Tones::new(&palette(BG)).alert_block();
        let sealed = |x: u16| {
            r.shot.paints[line].iter().any(|(c, p)| {
                c.contains(&x) && p.sealed && p.bg == Some(back) && p.fg == Some(text)
            })
        };
        assert!(
            sealed(col("[good.example]")) && sealed(col("[good.example]") + 13),
            "{skin:?}: the use's text and brackets"
        );
        assert!(!sealed(col("[good.example/docs]")), "{skin:?}: the honest use");
    }
    // calm marks both too
    let r = shown("md10-ref-calm", Skin::Calm, false, REF_DOC);
    let out = draw(r.shot.clone());
    assert_eq!(seq(&out, "https://evil.example/login"), calm_token());
    assert_eq!(seq(&out, "[good.example]"), calm_token());
    // and with no color: inverse and bold
    let r = shown("md10-ref-mono", Skin::Depth, true, REF_DOC);
    let out = draw(r.shot.clone());
    for what in ["https://evil.example/login", "[good.example]"] {
        let sgr = seq(&out, what);
        assert!(sgr.contains(";7") && sgr.contains(";1"), "{what}: {sgr:?}");
    }
}

#[test]
fn md_10_the_definition_far_from_the_use_marks_the_use_when_the_use_is_on_the_screen() {
    let mut text = String::from("Go to [good.example][far] now.\n");
    text.extend((0..40).map(|i| format!("\nfiller {i}\n")));
    text += "\n[far]: https://evil.example/login\n";
    let r = shown("md10-far", Skin::Depth, false, &text);
    let out = draw(r.shot.clone());
    assert_eq!(
        seq(&out, "[good.example]"),
        block(),
        "the definition is off the screen, the use is marked"
    );
    assert!(!out.contains("evil.example"), "and the definition is not on it");
}

#[test]
fn md_10_a_link_that_says_what_it_is_is_not_marked() {
    let honest = "[docs](https://one.example/docs) [two.example](https://two.example) x\n";
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md10-honest", skin, false, honest);
        let out = draw(r.shot.clone());
        for what in ["https://one.example/docs", "https://two.example"] {
            assert_ne!(seq(&out, what), block(), "{skin:?}: {what}");
        }
    }
}

#[test]
fn md_10_a_char_shown_in_another_form_in_such_a_target_is_still_shown_as_itself() {
    let line = "[https://good.example](https://evil\u{202e}.example) end\n";
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md10-hostile", skin, false, line);
        let rows: String = r.shot.frame.rows.iter().map(|x| x.text.clone()).collect();
        assert!(rows.contains("<U+202E>") && !rows.contains('\u{202e}'), "{skin:?}: {rows:?}");
    }
}

/// MD-10: looking through a document for its reference links costs what the cap allows: the first
/// `REF_LINES` lines, with a definition and a use of an address in every tenth, take well under the
/// millisecond that a key may take, whatever the length of the file.
///
///   cargo test --release -p sim-editor --test md_link -- --ignored md_10_bench --nocapture
#[test]
#[ignore]
fn md_10_bench_the_scan_for_reference_links_takes_well_under_a_key() {
    release_only();
    let line = |i: usize| match i % 10 {
        0 => format!("A site, [docs.example.org][r{i}], and {i} more words of prose in the line."),
        1 => format!("[r{}]: https://docs.example.org/{i}", i - 1),
        2 => format!(
            "- [x] a task and a [link](https://example.com/{i}) with some **bold** text {i}"
        ),
        _ => format!("Some prose for the line {i}, long enough to be a line of a README."),
    };
    for n in [300, 2_000, 20_000] {
        let lines: Vec<String> = (0..n).map(line).collect();
        let mut t: Vec<std::time::Duration> = (0..30)
            .map(|_| {
                let t0 = std::time::Instant::now();
                std::hint::black_box(Refs::new(n, &|l| lines.get(l).cloned()));
                t0.elapsed()
            })
            .collect();
        t.sort();
        eprintln!("MD-10 scan of {n} lines: median {:?}, worst {:?}", t[15], t[29]);
        assert!(t[15] < std::time::Duration::from_micros(600), "{n} lines: {:?}", t[15]);
    }
}

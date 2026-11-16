//! OPEN-2: `gx` hands the http or https address under the cursor (or a Markdown link's target) to the system's
//! opener, never one that MD-10 marks, never in plain mode, and only as one argument of a program found on an
//! absolute entry of the `PATH`, with no shell.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::io;
use sim::sim::SimEffect;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

/// What `gx` handed to the opener, in order.
fn handed(r: &Rig) -> Vec<String> {
    r.log
        .iter()
        .filter_map(|e| if let SimEffect::Browse(u) = e { Some(u.clone()) } else { None })
        .collect()
}

#[test]
fn open_2_gx_hands_the_address_under_the_cursor_to_the_opener() {
    let text = "see https://user@example.com:8080/a/b?q=1#x. or (http://plain.org) here\n";
    let mut r = rig("o2-url", &[&format!("a.txt={text}")], &["a.txt"]);
    r.keys("wgx");
    assert_eq!(handed(&r), ["https://user@example.com:8080/a/b?q=1#x"]);
    assert_eq!(r.status(), "opened example.com:8080");
    r.keys("fhgx");
    assert_eq!(handed(&r).last().unwrap(), "http://plain.org");
    r.keys("$gx");
    assert_eq!(r.status(), "gx: no web address under the cursor");
    assert_eq!(handed(&r).len(), 2);
}

#[test]
fn open_2_only_http_and_https_and_only_plain_ascii_are_handed_over() {
    let lines = [
        "ftp://example.com/x",
        "javascript:alert(1)",
        "file:///etc/passwd",
        "https://",
        "https://例え.jp/x",
        "https://a.com/x\\y",
        "HTTPS://Example.COM/ok",
    ];
    let text = lines.join("\n") + "\n";
    let mut r = rig("o2-schemes", &[&format!("a.txt={text}")], &["a.txt"]);
    for _ in &lines {
        r.keys("gxj");
    }
    assert_eq!(handed(&r), ["HTTPS://Example.COM/ok"]);
}

#[test]
fn open_2_a_markdown_link_s_target_and_never_one_that_md_10_marks() {
    let md = "[the docs](https://docs.example.com/guide \"t\")\n[example.com](https://evil.example/login)\n";
    let mut r = rig("o2-md", &[&format!("a.md={md}")], &["a.md"]);
    r.keys("lgx");
    assert_eq!(handed(&r), ["https://docs.example.com/guide"]);
    assert_eq!(r.status(), "opened docs.example.com");
    // its text says a place its target is not: the target is named, and not opened
    r.keys("jlgx");
    assert_eq!(handed(&r).len(), 1);
    assert_eq!(r.status(), "not opened: the link is marked, it goes to https://evil.example/login");
    // on the target itself too
    r.keys("f(lgx");
    assert_eq!(handed(&r).len(), 1);
    // in a file that is no Markdown, a link in a comment is judged all the same
    let code = "// see [example.com](https://evil.example/x)\n";
    let mut c = rig("o2-code", &[&format!("a.rs={code}")], &["a.rs"]);
    c.keys("f(lgx");
    assert!(c.status().starts_with("not opened: the link is marked"), "{}", c.status());
    assert!(handed(&c).is_empty());
}

#[test]
fn open_2_plain_mode_opens_nothing_and_says_so() {
    let mut r = rig("o2-plain", &["a.txt=https://example.com\n"], &["--plain", "a.txt"]);
    r.keys("gx");
    assert!(handed(&r).is_empty());
    assert_eq!(r.status(), "plain mode runs no program: not opened");
}

#[test]
fn open_2_the_opener_is_found_on_an_absolute_path_entry_and_given_the_address_as_one_argument() {
    let dir = temp_dir("o2-opener");
    let (bin, witness) = (dir.0.join("bin"), dir.0.join("args"));
    std::fs::create_dir(&bin).unwrap();
    let opener = bin.join(io::OPENER);
    let script = format!(
        "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\n",
        witness.display()
    );
    std::fs::write(&opener, script).unwrap();
    std::fs::set_permissions(&opener, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = |dirs: &[&str]| Some(OsString::from(dirs.join(":")));
    let abs = bin.to_str().unwrap();
    assert_eq!(io::which(path(&["", ".", "bin", abs]), io::OPENER), Some(opener.clone()));
    assert_eq!(
        io::which(path(&["bin", "./bin"]), io::OPENER),
        None,
        "a relative entry is not looked in"
    );
    // one argument, as it is: a `;` or a `$(...)` in it would be a shell's, and there is none
    let url = "https://example.com/a;b$(touch_x)&c=1";
    io::browse(Some(&opener), url).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while !witness.exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(std::fs::read_to_string(&witness).unwrap(), format!("{url}\n"));
    assert!(io::browse(None, url).unwrap_err().contains("not opened"));
}

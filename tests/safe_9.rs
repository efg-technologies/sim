//! SAFE-9: no modelines, no network, only the allowed dependencies.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use std::fs;
use std::path::Path;

fn sources(dir: &Path, out: &mut Vec<String>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(fs::read_to_string(&p).unwrap());
        }
    }
}

#[test]
fn safe_9_src_has_no_network_code() {
    let mut all = vec![];
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut all);
    assert!(all.len() > 10);
    assert!(all.iter().all(|s| !s.contains("std::net")));
}

#[test]
fn safe_9_dependencies_are_exactly_the_allowlist() {
    let toml =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let section = toml.split("[dependencies]").nth(1).unwrap().split("\n[").next().unwrap();
    let mut deps: Vec<&str> = section
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| Some(l.split_once('=')?.0.trim()))
        .collect();
    deps.sort_unstable();
    assert_eq!(
        deps,
        ["crossterm", "notify", "regex-lite", "ropey", "signal-hook", "similar", "unicode-width"]
    );
}

#[test]
fn safe_9_modelines_in_the_file_change_nothing() {
    let body = "\tx\n";
    let plain = screen(&mut app_with(Some("f.rs"), body.as_bytes(), (40, 4)));
    for line in [
        "# vim: set tabstop=8 :",
        "/* vi: ts=8 */",
        "-*- tab-width: 8 -*-",
        "// si: wrap=10",
        ":q!",
    ] {
        let with =
            screen(&mut app_with(Some("f.rs"), format!("{line}\n{body}").as_bytes(), (40, 4)));
        assert_eq!(with.rows[2].text, plain.rows[1].text.replacen("   1", "   3", 1), "{line}");
        assert_eq!((with.shape, with.rows[0].text.get(5..)), (plain.shape, Some(line)));
    }
}

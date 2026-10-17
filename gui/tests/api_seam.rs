//! GUI-8
//! The window reaches sim through `src/api.rs` and nowhere else, and keeps none of sim's rules of its
//! own: what a cell finally is, which layers there are and what each surface is come from sim's
//! `Resolver`, `Screen::layers` and `Paint::role`, so a change of sim's paint rules cannot leave the
//! window drawing the old ones.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers; the tests themselves may unwrap
use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The code of a file: no line comments, no doc comments.
fn code(p: &Path) -> Vec<(usize, String)> {
    fs::read_to_string(p)
        .unwrap()
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim_start().starts_with("//"))
        .map(|(i, l)| (i + 1, l.split(" //").next().unwrap_or(l).to_string()))
        .collect()
}

fn files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut v = vec![];
    rust_files(&root.join("src"), &mut v);
    rust_files(&root.join("tests"), &mut v);
    v.retain(|p| !p.ends_with("tests/api_seam.rs")); // (this file names them to look for them)
    v.sort();
    v
}

/// Whether `line` names a path of `krate` (`sim::...`) and not of a crate that ends in its name.
fn names(line: &str, krate: &str) -> bool {
    let pat = format!("{krate}::");
    line.match_indices(&pat).any(|(i, _)| {
        line[..i].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    })
}

#[test]
fn gui_8_only_api_rs_names_sims_crates() {
    let mut seen = 0;
    for p in files() {
        if p.ends_with("src/api.rs") {
            continue;
        }
        for (n, l) in code(&p) {
            seen += 1;
            assert!(
                !names(&l, "sim") && !names(&l, "sim_core"),
                "{}:{n} names sim's crate: {l}",
                p.display()
            );
        }
    }
    assert!(seen > 1000, "the scan saw the crate ({seen} lines)");
    // and api.rs does name them (the scan is not looking at nothing)
    let api = code(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/api.rs"));
    assert!(
        api.iter().any(|(_, l)| names(l, "sim")) && api.iter().any(|(_, l)| names(l, "sim_core"))
    );
}

#[test]
fn gui_8_the_scan_tells_a_path_of_sim_from_a_crate_that_ends_in_its_name() {
    assert!(names("use sim::paint::Screen;", "sim"));
    assert!(names("let x = (sim::color::mix)(a);", "sim"));
    assert!(!names("use sim_gui::api::*;", "sim"));
    assert!(!names("use my_sim::x;", "sim"));
    assert!(names("use sim_core::core::app::Key;", "sim_core"));
}

#[test]
fn gui_8_the_window_has_no_copy_of_sims_paint_rules() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // the modules that copied them are gone
    assert!(!root.join("cellstyle.rs").exists(), "a cell's style is sim's `Resolver`");
    for p in files() {
        if p.starts_with(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")) {
            continue;
        }
        for (n, l) in code(&p) {
            let at = format!("{}:{n}: {l}", p.display());
            // the look table, the SGR order, the floors of the fog: sim's
            for word in
                ["made_of", "fn sgr", "TEXT_FLOOR", "NUMBER_FLOOR", "fade_to(", "Geo", "geo()"]
            {
                assert!(!l.contains(word), "{at}");
            }
            // a surface is told by its layer or its role, not by the identity of its color
            let tones = ["t.", "tones.", "self.t.", "self.tones."];
            for t in tones {
                assert!(!l.contains(&format!("== {t}")) && !l.contains(&format!("!= {t}")), "{at}");
            }
        }
    }
}

/// Whether `line` has `word` not as the end of a longer name (`Config {` is not `FontConfig {`).
fn word_in(line: &str, word: &str) -> bool {
    line.match_indices(word).any(|(i, _)| {
        line[..i].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    })
}

/// The window is a front end like the terminal (PLAIN-1, GUI-4): its start is `begin`, which applies the
/// rules of plain mode, of the state directory and of git's hooks once for every front end, and it decides
/// none of them: it does not build the sim it runs or the shell's config, does not run `forbid_programs`
/// or look for an `rg`, does not name the state directory or catch the signals itself.
#[test]
fn gui_8_the_window_starts_sim_only_through_the_start_every_front_end_shares() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut all = vec![];
    rust_files(&src, &mut all);
    all.sort();
    let mut seen = 0;
    for p in &all {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        for (n, l) in code(p) {
            seen += 1;
            let at = format!("{}:{n}: {l}", p.display());
            for word in [
                "Env {",
                "Sim::new(",
                "Config {",
                "forbid_programs",
                "plain_by_default",
                "privileged(",
                "use_no_hooks",
                "which_rg",
                "state_dir(",
                "migrate(",
                "signal_hook",
            ] {
                assert!(!word_in(&l, word), "the window decides {word} itself: {at}");
            }
            // a world of its own making is for the offscreen path (a render or a test is not a privileged
            // context); a window's is the system's, `World::here()`
            if name != "headless.rs" && name != "api.rs" {
                assert!(!word_in(&l, "World {"), "a world made by hand outside headless.rs: {at}");
            }
        }
    }
    assert!(seen > 1000);
    let has = |file: &str, word: &str| code(&src.join(file)).iter().any(|(_, l)| l.contains(word));
    assert!(
        has("session.rs", "begin(") && has("session.rs", "arguments("),
        "Session::new starts through begin"
    );
    assert!(has("session.rs", "signals("), "and catches the signals as the terminal does");
    assert!(has("app.rs", "World::here()"), "the window's world is the system's");
    assert!(!has("session.rs", "World::here()"), "(a session takes the world it is given)");
}

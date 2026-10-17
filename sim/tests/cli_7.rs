//! CLI-7: `sim --version` and `sim --help` print and exit 0, an unknown flag prints the usage to
//! stderr and exits 2, and none of them touches the terminal: no raw mode, no query, no escape.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
use std::process::{Command, Output, Stdio};

fn sim(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sim"))
        .args(args)
        .stdin(Stdio::null())
        .env_remove("NO_COLOR")
        .output()
        .unwrap()
}

fn text(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

/// Nothing a terminal would act on: no escape sequence, no control char but the line break.
fn plain(o: &Output) {
    for b in o.stdout.iter().chain(&o.stderr) {
        assert!(*b == b'\n' || !b.is_ascii_control(), "a control byte {b:#x}");
    }
}

#[test]
fn cli_7_version_prints_the_name_and_the_version_and_exits_0() {
    for args in [&["--version"][..], &["--plain", "--version"], &["--version", "a.txt"]] {
        let o = sim(args);
        assert_eq!(text(&o.stdout), format!("sim {}\n", env!("CARGO_PKG_VERSION")), "{args:?}");
        assert_eq!((o.status.code(), o.stderr.len()), (Some(0), 0), "{args:?}");
        plain(&o);
    }
}

#[test]
fn cli_7_help_is_a_short_usage_with_the_flags_the_variables_and_the_way_to_more() {
    let o = sim(&["--help"]);
    let (out, err) = (text(&o.stdout), text(&o.stderr));
    assert_eq!((o.status.code(), err.as_str()), (Some(0), ""));
    plain(&o);
    for want in [
        "usage: sim",
        "--plain",
        "--version",
        "--help",
        "NO_COLOR",
        "SIM_ON_NORMAL",
        ":help",
        "F1",
        "the next-generation Vim",
    ] {
        assert!(out.contains(want), "{want:?} in\n{out}");
    }
    // the owner's wording: sim is never described as a likeness of Vim
    let low = out.to_lowercase();
    assert!(!low.contains("-like") && !low.contains("-style") && !low.contains("clone"), "{out}");
    assert!(out.lines().count() < 30, "short: {} lines", out.lines().count());
    // after `--` a file may be called anything: that is a file to open (there is no terminal here)
    let o = sim(&["--", "--help"]);
    assert!(o.status.code() != Some(0) && !text(&o.stdout).contains("usage"));
}

#[test]
fn cli_7_an_unknown_flag_prints_the_usage_to_stderr_and_exits_2() {
    for flag in ["--bogus", "-x", "--plains", "-h"] {
        let o = sim(&[flag]);
        assert_eq!(o.status.code(), Some(2), "{flag}");
        assert!(o.stdout.is_empty(), "{flag}: nothing on stdout");
        assert!(text(&o.stderr).contains("usage: sim"), "{flag}: {}", text(&o.stderr));
        plain(&o);
    }
}

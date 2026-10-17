//! GUI-16
//! `sim-gui --help` says how the window is told what font to use, and everything else it can be told, and
//! it opens no window to say it (it is run with no display at all).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers; the tests themselves may unwrap
use std::process::Command;

/// The binary with an environment of nothing: no display, no home, no font setting.
fn sim_gui(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sim-gui")).args(args).env_clear().output().unwrap()
}

#[test]
fn gui_16_help_names_the_font_override_and_the_platforms_default() {
    for flag in ["--help", "-h"] {
        let out = sim_gui(&[flag]);
        assert_eq!(out.status.code(), Some(0), "{flag}");
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(
            text.contains("sim-gui 0.1.0") || text.contains(env!("CARGO_PKG_VERSION")),
            "{text}"
        );
        for word in [
            "SIM_GUI_FONT ",
            "SIM_GUI_FONT_SIZE",
            "SIM_GUI_LOCALE",
            "SIM_GUI_THEME",
            "SIM_GUI_REDUCED_MOTION",
            "SIM_GUI_ADAPTER",
            "SIM_GUI_ICONS",
            "SIM_GUI_NO_STATE",
            "monospace",
        ] {
            assert!(text.contains(word), "--help does not say {word}: {text}");
        }
        // every family the default tries is named, whole (a name that wrapped would not be found)
        for name in sim_gui::font::DEFAULT_FAMILIES.iter().chain(&sim_gui::font::MACOS_FAMILIES) {
            assert!(text.contains(name), "--help does not name {name}: {text}");
        }
        assert!(!text.contains("{version}"));
    }
}

#[test]
fn gui_16_every_switch_the_window_reads_is_in_the_help() {
    // the words the sources read from the environment are the words --help lists
    let help = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/help.txt")).unwrap();
    let mut read = std::collections::BTreeSet::new();
    let mut files = vec![];
    for e in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "rs") {
            files.push(p);
        }
    }
    for p in files {
        let s = std::fs::read_to_string(&p).unwrap();
        for (i, _) in s.match_indices("var(\"SIM_GUI_") {
            let rest = &s[i + 5..];
            read.insert(rest[..rest.find('"').unwrap()].to_string());
        }
    }
    assert!(read.len() >= 6, "{read:?}");
    for word in &read {
        assert!(help.contains(word.as_str()), "{word} is read and not in the help");
    }
}

#[test]
fn gui_16_version_prints_without_a_window() {
    let out = sim_gui(&["--version"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("sim-gui {}\n", env!("CARGO_PKG_VERSION"))
    );
}

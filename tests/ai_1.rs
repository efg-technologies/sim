//! AI-1: `:ref` sends `path:start-end` to the clipboard (through the Clipboard effect, which the
//! shell writes as OSC 52): relative to the current directory when the file is inside it,
//! absolute otherwise. The shell tells App the directory; the core never reads it.
mod common;
use common::*;
use si::core::app::{App, Effect};

fn at(cwd: Option<&str>, path: Option<&str>, text: &str) -> App {
    let mut a = app_with(path, text.as_bytes(), (70, 8));
    a.cwd = cwd.map(String::from);
    a
}

/// What `:ref` (or `keys`) put on the clipboard, and what it said.
fn sent(a: &mut App, keys: &str) -> (Option<String>, String) {
    let fx = feed(a, keys);
    let clip = fx.iter().find_map(|e| match e {
        Effect::Clipboard(s) => Some(s.clone()),
        _ => None,
    });
    (clip, status(&screen(a)).to_string())
}

fn copied(s: &str) -> (Option<String>, String) {
    (Some(s.to_string()), format!("copied {s}"))
}

#[test]
fn ai_1_normal_mode_sends_the_current_line() {
    let mut a = at(Some("/work"), Some("src/a.rs"), "a\nb\nc\nd\n");
    assert_eq!(sent(&mut a, ":ref\n"), copied("src/a.rs:1-1"));
    assert_eq!(sent(&mut a, "jj:ref\n"), copied("src/a.rs:3-3"));
    assert_eq!(sent(&mut a, ":.ref\n"), copied("src/a.rs:3-3"));
    assert!(!a.buf.dirty());
}

#[test]
fn ai_1_a_range_gives_start_and_end() {
    let mut a = at(Some("/work"), Some("a.rs"), "a\nb\nc\nd\ne\n");
    assert_eq!(sent(&mut a, ":2,4ref\n"), copied("a.rs:2-4"));
    assert_eq!(sent(&mut a, ":4,2ref\n"), copied("a.rs:2-4"));
    assert_eq!(sent(&mut a, ":%ref\n"), copied("a.rs:1-5"));
    assert_eq!(sent(&mut a, ":$ref\n"), copied("a.rs:5-5"));
    assert_eq!(sent(&mut a, ":9ref\n"), copied("a.rs:5-5"), "numbers clamp to the file");
}

#[test]
fn ai_1_visual_mode_uses_the_selection() {
    let mut a = at(Some("/work"), Some("a.rs"), "a\nb\nc\nd\ne\n");
    assert_eq!(sent(&mut a, "jVj:ref\n"), copied("a.rs:2-3"));
    assert_eq!(a.vi.line, "");
    assert_eq!(sent(&mut a, "ggvjj:ref\n"), copied("a.rs:1-3"));
    assert_eq!(sent(&mut a, "G$v:ref\n"), copied("a.rs:5-5"));
    // The command line is pre-filled with the range, as the user sees it.
    feed(&mut a, "ggVj:");
    assert_eq!(status(&screen(&mut a)), ":'<,'>");
}

#[test]
fn ai_1_the_path_is_relative_inside_the_current_directory_and_absolute_outside() {
    for (cwd, path, want) in [
        ("/work", "a.rs", "a.rs"),
        ("/work", "./a.rs", "a.rs"),
        ("/work", "src/../lib/a.rs", "lib/a.rs"),
        ("/work", "/work/src/a.rs", "src/a.rs"),
        ("/work/src", "/work/src/deep/a.rs", "deep/a.rs"),
        ("/work", "/etc/hosts", "/etc/hosts"),
        ("/work/src", "../a.rs", "/work/a.rs"),
        ("/work/src", "../../etc/a.rs", "/etc/a.rs"),
        ("/work", "/workshop/a.rs", "/workshop/a.rs"),
        ("/work", "/work/../tmp/a.rs", "/tmp/a.rs"),
        ("/", "/etc/hosts", "etc/hosts"),
    ] {
        let mut a = at(Some(cwd), Some(path), "x\n");
        assert_eq!(sent(&mut a, ":ref\n"), copied(&format!("{want}:1-1")), "{path} from {cwd}");
    }
}

#[test]
fn ai_1_without_a_known_directory_the_path_is_as_it_was_given() {
    let mut a = at(None, Some("src/a.rs"), "x\n");
    assert_eq!(sent(&mut a, ":ref\n"), copied("src/a.rs:1-1"));
}

#[test]
fn ai_1_a_buffer_without_a_file_has_nothing_to_point_at() {
    let mut a = at(Some("/work"), None, "x\n");
    assert_eq!(sent(&mut a, ":ref\n"), (None, "no file name".to_string()));
}

#[test]
fn ai_1_it_works_on_a_read_only_buffer_and_changes_no_register() {
    let mut a = app_with(Some("bin"), &[b'a', 0xff, b'\n'], (70, 6));
    a.cwd = Some("/work".into());
    assert_eq!(sent(&mut a, ":ref\n"), copied("bin:1-1"));
    let mut a = at(Some("/work"), Some("a"), "one\ntwo\n");
    feed(&mut a, "yy");
    sent(&mut a, "j:ref\n");
    feed(&mut a, "p");
    assert_eq!(text(&a), "one\ntwo\none", "the register still holds the yank");
}

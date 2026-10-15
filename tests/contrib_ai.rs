//! contrib/ai: the AI filter script, run against a stub `claude` on PATH (no network, no model).
// Helpers outside `#[test]` fns are not covered by clippy.toml, so allow it for the file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const AI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/contrib/ai");

/// What the stub does with the prompt: it replies with the text between the markers, which makes
/// `ai` an identity filter (the line break that `ai` adds before the end marker is stripped again).
const ECHO: &str =
    r"sed -n '/^-----BEGIN TEXT-----$/,/^-----END TEXT-----$/p' @STDIN@ | sed '1d;$d'";

/// A unique directory under target/tmp, removed on drop.
struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Tmp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("contrib-ai-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::create_dir_all(dir.join("tmp")).unwrap();
        Tmp(dir)
    }

    /// Installs a `claude` that saves its arguments and stdin next to itself, then runs `body`.
    fn stub(&self, body: &str) {
        let path = self.0.join("bin").join("claude");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\ncat > \"$0.stdin\"\n{}\n",
            body.replace("@STDIN@", "\"$0.stdin\"")
        );
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn recorded(&self, what: &str) -> String {
        fs::read_to_string(self.0.join("bin").join(format!("claude.{what}"))).unwrap()
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Run {
    code: i32,
    out: String,
    err: String,
}

impl Run {
    fn first_err_line(&self) -> &str {
        self.err.lines().next().unwrap_or("")
    }
}

/// Runs `sh contrib/ai args` with `input` on stdin. `claude` is the stub when there is one; the
/// real `claude` (if this machine has it) is never on the PATH, whatever happens.
fn run(t: &Tmp, args: &[&str], input: &[u8], env: &[(&str, &str)]) -> Run {
    let stub = t.0.join("bin");
    let old = std::env::var_os("PATH").unwrap_or_default();
    let others = std::env::split_paths(&old).filter(|d| !d.join("claude").exists());
    let path = std::env::join_paths(std::iter::once(stub).chain(others)).unwrap();
    let mut child = Command::new("/bin/sh")
        .arg(AI)
        .args(args)
        .env("PATH", path)
        .env("TMPDIR", t.0.join("tmp"))
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // ai may exit before it reads anything (a usage error): the broken pipe is not the test's concern
    let _ = child.stdin.take().unwrap().write_all(input);
    let o = child.wait_with_output().unwrap();
    let left: Vec<OsString> =
        fs::read_dir(t.0.join("tmp")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(left.is_empty(), "ai left temporary files behind: {left:?}");
    Run {
        code: o.status.code().unwrap(),
        out: String::from_utf8(o.stdout).unwrap(),
        err: String::from_utf8(o.stderr).unwrap(),
    }
}

#[test]
fn the_instruction_and_the_text_reach_claude_and_only_its_reply_comes_out() {
    let t = Tmp::new();
    t.stub("printf 'Hello there.\\n'");
    let r = run(&t, &["fix typos"], b"helo there\nsecond line\n", &[]);
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, "Hello there.\n", ""));
    let prompt = t.recorded("stdin");
    assert!(prompt.starts_with("INSTRUCTION: fix typos\n"), "{prompt}");
    assert!(prompt.contains("-----BEGIN TEXT-----\nhelo there\nsecond line\n-----END TEXT-----\n"));
    // print mode, with no tools and no saved session; the rules ride in the system prompt
    let args = t.recorded("args");
    let args: Vec<&str> = args.lines().collect();
    assert_eq!(args[0], "-p");
    assert_eq!(args[1], "--system-prompt");
    assert!(args[2].contains("reply with ONLY the resulting text"), "{args:?}");
    assert!(args.contains(&"--no-session-persistence"), "{args:?}");
    assert!(args.windows(2).any(|w| w == ["--tools", ""]), "{args:?}");
    assert!(!args.contains(&"--model"), "no model unless AI_MODEL says so: {args:?}");
}

#[test]
fn the_words_of_the_instruction_are_one_instruction_kept_as_typed() {
    let t = Tmp::new();
    t.stub("printf 'x\\n'");
    run(&t, &["translate", "to", "English"], b"a\n", &[]);
    assert!(t.recorded("stdin").starts_with("INSTRUCTION: translate to English\n"));
    let odd = "use `ls` & 'quotes' \"too\", $HOME, %s and \\n";
    run(&t, &[odd], b"a\n", &[]);
    assert!(t.recorded("stdin").starts_with(&format!("INSTRUCTION: {odd}\n")));
}

#[test]
fn ai_model_picks_the_model() {
    let t = Tmp::new();
    t.stub("printf 'x\\n'");
    run(&t, &["go"], b"a\n", &[("AI_MODEL", "sonnet")]);
    let args = t.recorded("args");
    let args: Vec<&str> = args.lines().collect();
    assert!(args.windows(2).any(|w| w == ["--model", "sonnet"]), "{args:?}");
}

#[test]
fn the_final_line_break_of_the_text_is_kept_whatever_claude_prints() {
    let t = Tmp::new();
    for (reply, input, want) in [
        ("x\\ny\\n\\n\\n", "a\nb\n", "x\ny\n"), // extra line breaks are dropped
        ("x\\ny", "a\nb\n", "x\ny\n"),          // a missing one is put back
        ("x\\ny\\n\\n", "a\nb", "x\ny"),        // none in the text, none in the result
        ("x\\ny", "a\nb", "x\ny"),
        ("x\\n", "a", "x"),
    ] {
        t.stub(&format!("printf '{reply}'"));
        let r = run(&t, &["go"], input.as_bytes(), &[]);
        assert_eq!((r.code, r.out.as_str()), (0, want), "reply {reply:?} for {input:?}");
    }
}

#[test]
fn the_text_comes_back_byte_for_byte_when_claude_changes_nothing() {
    let t = Tmp::new();
    t.stub(ECHO);
    for input in [
        "plain\n",
        "no final line break",
        "  indented\n\ttabbed\n\n\nblank lines above\n",
        "100% %s %d \\n \\\\ $HOME `date` $(date) 'q' \"q\"\n",
        "日本語のテキスト\n二行目\n",
        "dos\r\nline ends\r\n",
        "-----BEGIN TEXT-----\nlooks like a marker\n",
    ] {
        let r = run(&t, &["go"], input.as_bytes(), &[]);
        assert_eq!((r.code, r.out.as_str()), (0, input), "{input:?}");
    }
}

#[test]
fn a_missing_claude_is_said_on_stderr_and_nothing_is_printed() {
    let t = Tmp::new(); // no stub
    let r = run(&t, &["go"], b"a\n", &[]);
    assert_eq!(r.code, 127);
    assert_eq!(r.out, "");
    assert!(r.first_err_line().starts_with("ai: claude not found"), "{}", r.err);
}

#[test]
fn a_failing_claude_exits_non_zero_with_its_reason_first_and_prints_nothing() {
    let t = Tmp::new();
    t.stub("printf 'half an answer'; echo 'boom: credit balance is too low' >&2; echo more >&2; exit 3");
    let r = run(&t, &["go"], b"a\n", &[]);
    assert_eq!((r.code, r.out.as_str()), (3, "")); // a half answer is not passed on
    assert_eq!(r.first_err_line(), "ai: claude failed (exit 3): boom: credit balance is too low");
    assert!(r.err.contains("\nmore\n"), "claude's own stderr follows: {}", r.err);
}

#[test]
fn a_failing_claude_that_only_speaks_on_stdout_is_still_explained() {
    let t = Tmp::new();
    t.stub("echo; echo 'Invalid API key - Please run /login'; exit 1");
    let r = run(&t, &["go"], b"a\n", &[]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.first_err_line(),
        "ai: claude failed (exit 1): Invalid API key - Please run /login"
    );
    let t = Tmp::new();
    t.stub("exit 4");
    let r = run(&t, &["go"], b"a\n", &[]);
    assert_eq!((r.code, r.first_err_line()), (4, "ai: claude failed (exit 4)"));
}

#[test]
fn an_empty_reply_is_an_error_not_a_request_to_delete_the_text() {
    let t = Tmp::new();
    for reply in ["", "\\n\\n", "  \\n"] {
        t.stub(&format!("printf '{reply}'"));
        let r = run(&t, &["go"], b"keep me\n", &[]);
        assert_eq!((r.code, r.out.as_str()), (1, ""), "reply {reply:?}");
        assert_eq!(r.first_err_line(), "ai: claude returned no text");
    }
}

#[test]
fn no_instruction_is_a_usage_error_and_claude_is_not_run() {
    let t = Tmp::new();
    t.stub("echo ran > \"$0.ran\"");
    for args in [&[][..], &[""][..]] {
        let r = run(&t, args, b"a\n", &[]);
        assert_eq!((r.code, r.out.as_str()), (2, ""));
        assert!(r.first_err_line().starts_with("ai: usage:"), "{}", r.err);
    }
    assert!(!t.0.join("bin").join("claude.ran").exists());
}

#[test]
fn claude_runs_in_an_empty_directory_not_where_the_file_is() {
    let t = Tmp::new();
    t.stub("pwd > \"$0.pwd\"; ls -A | wc -l | tr -d ' ' > \"$0.count\"; printf 'x\\n'");
    run(&t, &["go"], b"a\n", &[]);
    assert!(t.recorded("pwd").contains("/ai."), "{}", t.recorded("pwd"));
    // only what ai itself put there: text, prompt, out, err
    assert_eq!(t.recorded("count").trim(), "4");
}

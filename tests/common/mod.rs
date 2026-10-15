//! Helpers shared by the integration tests.
#![allow(dead_code, clippy::unwrap_used)]
use si::core::app::{App, Effect, Event, Key};
use si::core::buffer::{Hunk, Mark, Words};
use si::core::view::{Frame, Style};
use unicode_width::UnicodeWidthChar;

/// Key notation: a char is itself (`\n` Enter, `\x1b` Esc, `\x7f` Backspace, `\t` Tab, `\x12`
/// Ctrl-r); `<Esc> <CR> <BS> <Tab> <Del> <Up> <Down> <Left> <Right> <Home> <End> <PageUp>
/// <PageDown> <C-x> <lt>` are the others. A `<` that starts none of them is a plain `<`.
pub fn keys(s: &str) -> Vec<Key> {
    let (mut out, mut rest) = (vec![], s);
    while let Some(c) = rest.chars().next() {
        let named =
            rest.strip_prefix('<').and_then(|r| r.split_once('>')).and_then(|(name, tail)| {
                let k = match name {
                    "Esc" => Key::Esc,
                    "CR" => Key::Enter,
                    "BS" => Key::Backspace,
                    "Tab" => Key::Tab,
                    "Del" => Key::Delete,
                    "Up" => Key::Up,
                    "Down" => Key::Down,
                    "Left" => Key::Left,
                    "Right" => Key::Right,
                    "Home" => Key::Home,
                    "End" => Key::End,
                    "PageUp" => Key::PageUp,
                    "PageDown" => Key::PageDown,
                    "lt" => Key::Char('<'),
                    n if n.len() == 3 && n.starts_with("C-") => Key::Ctrl(n.chars().nth(2)?),
                    _ => return None,
                };
                Some((k, tail))
            });
        if let Some((k, tail)) = named {
            out.push(k);
            rest = tail;
            continue;
        }
        out.push(match c {
            '\n' => Key::Enter,
            '\x1b' => Key::Esc,
            '\x7f' => Key::Backspace,
            '\t' => Key::Tab,
            '\x12' => Key::Ctrl('r'),
            c => Key::Char(c),
        });
        rest = &rest[c.len_utf8()..];
    }
    out
}

pub fn app_with(path: Option<&str>, bytes: &[u8], size: (u16, u16)) -> App {
    App::new(path.map(String::from), Ok(Some(bytes.to_vec())), size)
}

/// An unnamed 80x24 App holding `text`.
pub fn app(text: &str) -> App {
    app_with(None, text.as_bytes(), (80, 24))
}

/// Types the keys and returns every Effect they produced.
pub fn feed(app: &mut App, keys: &str) -> Vec<Effect> {
    self::keys(keys).into_iter().flat_map(|k| app.update(Event::Key(k))).collect()
}

pub fn last_frame(fx: &[Effect]) -> &Frame {
    fx.iter()
        .rev()
        .find_map(|e| match e {
            Effect::Render(f) => Some(f),
            _ => None,
        })
        .unwrap()
}

/// The current screen.
pub fn screen(app: &mut App) -> Frame {
    let (w, h) = app.size;
    last_frame(&app.update(Event::Resize(w, h))).clone()
}

pub fn text(app: &App) -> String {
    app.buf.text.to_string()
}

/// A unique empty directory, removed when dropped.
pub struct TempDir(pub std::path::PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn temp_dir(name: &str) -> TempDir {
    let d = std::env::temp_dir().join(format!("si-it-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    TempDir(d)
}

/// Runs `f` on a thread and gives up after `secs`: a command that spins must fail, not hang.
pub fn within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || drop(tx.send(f())));
    rx.recv_timeout(std::time::Duration::from_secs(secs)).ok()
}

/// A deadline far away: a diff in a test never gives up (PROP-7 has its own tests).
pub fn far() -> std::time::Instant {
    std::time::Instant::now() + std::time::Duration::from_secs(3600)
}

/// The status line (last row).
pub fn status(f: &Frame) -> &str {
    &f.rows.last().unwrap().text
}

/// A Frame as text: its rows between bars, each followed by one style symbol per column
/// (`.` Primary, `-` Secondary, `#` Change, `x` Deleted, `_` underlined, blank past the text).
pub fn dump(f: &Frame) -> String {
    let mut out =
        format!("{}x{} cursor {},{} {:?}\n", f.width, f.height, f.cursor.0, f.cursor.1, f.shape);
    for r in &f.rows {
        let mut styles = String::new();
        for c in r.text.chars() {
            for _ in 0..c.width().unwrap_or(0) {
                let col = styles.len() as u16;
                let style =
                    r.spans.iter().find(|(s, _)| s.contains(&col)).map_or(Style::Primary, |s| s.1);
                styles.push(match (r.underline.iter().any(|u| u.contains(&col)), style) {
                    (true, _) => '_',
                    (_, Style::Primary) => '.',
                    (_, Style::Secondary) => '-',
                    (_, Style::Change) => '#',
                    (_, Style::Deleted) => 'x',
                });
            }
        }
        let pad = " ".repeat(f.width as usize - styles.len());
        out += &format!("|{}{pad}|\n|{styles}{pad}|\n", r.text);
    }
    out
}

/// Compares `actual` with tests/golden/{name}.txt; `SI_BLESS=1` rewrites the file.
pub fn golden(name: &str, actual: &str) {
    let path = format!("{}/tests/golden/{name}.txt", env!("CARGO_MANIFEST_DIR"));
    if std::env::var_os("SI_BLESS").is_some() {
        std::fs::write(&path, actual).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(actual, want, "{path} differs (SI_BLESS=1 rewrites it)");
}

/// Splits `|` (the cursor) out of `s`.
fn cursor_of(s: &str) -> (String, usize) {
    let at = s.find('|').unwrap();
    (format!("{}{}", &s[..at], &s[at + 1..]), s[..at].chars().count())
}

/// The text with `|` at the cursor.
pub fn marked(app: &App) -> String {
    let (t, c) = (text(app), app.buf.cursor);
    format!("{}|{}", t.chars().take(c).collect::<String>(), t.chars().skip(c).collect::<String>())
}

/// An 80x24 App holding `before` (`|` marks the cursor), after typing `keys`.
pub fn typed(before: &str, keys: &str) -> App {
    let (t, c) = cursor_of(before);
    let mut a = app_with(None, t.as_bytes(), (80, 24));
    assert!(c < t.chars().count() || t.is_empty(), "{before:?}: Normal mode rests on a char");
    a.buf.cursor = c;
    feed(&mut a, keys);
    a
}

/// Types `keys` into `before` and checks the text and the cursor against `after`.
pub fn check(before: &str, keys: &str, after: &str) {
    assert_eq!(marked(&typed(before, keys)), after, "{before:?} then {keys:?}");
}

/// `check` for each row of a table.
pub fn table(rows: &[(&str, &str, &str)]) {
    for (before, keys, after) in rows {
        check(before, keys, after);
    }
}

/// A small deterministic generator (xorshift), so the benchmarks see the same text every run.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() >> 11) as usize % n
    }
}

/// Code-like text of `lines` lines averaging about `avg` bytes: indented statements, comments,
/// blank lines, a few Japanese comments, some long lines and a few very long ones. `foo` and
/// `foobar` appear now and then (in about 1% of the lines), but `foo` followed by digits and
/// `bar`, and `zzqx`, never do.
pub fn code_text(lines: usize, avg: usize) -> String {
    const WORDS: [&str; 22] = [
        "let", "mut", "self", "buf", "text", "line", "cursor", "if", "else", "match", "fn", "pub",
        "Some(x)", "None", "return", "&mut", "u64", "usize", "count", "start", "end", "i + 1",
    ];
    const JA: [&str; 4] = ["行を", "カーソル", "保存して終了", "未保存の変更"];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut out = String::with_capacity(lines * (avg + 1));
    for i in 0..lines {
        let kind = rng.below(100);
        let target = match (i % 2500 == 7, kind) {
            (true, _) => 3000,
            (_, 90..=92) => avg * 5,
            _ => avg / 3 + rng.below(avg * 4 / 3),
        };
        let start = out.len();
        if kind < 8 {
            out.push('\n');
            continue;
        }
        out.push_str(&"    ".repeat(rng.below(4)));
        if kind < 14 {
            out.push_str("// ");
        }
        while out.len() - start < target {
            if (14..20).contains(&kind) {
                out.push_str(JA[rng.below(JA.len())]);
            } else {
                out.push_str(WORDS[rng.below(WORDS.len())]);
            }
            out.push([' ', ' ', '.', '(', ')', ';', ','][rng.below(7)]);
            if rng.below(9) == 0 {
                out.push_str(&rng.below(5000).to_string());
            }
            if rng.below(900) == 0 {
                out.push_str(["foo(", "foobar "][rng.below(2)]);
            }
        }
        out.push('\n');
    }
    out
}

/// The p-th percentile (0..=100) of sorted samples.
pub fn percentile<T: Copy>(sorted: &[T], p: usize) -> T {
    sorted[(sorted.len() * p).div_ceil(100).max(1) - 1]
}

/// Benchmarks mean something only on a release build.
#[allow(clippy::assertions_on_constants)]
pub fn release_only() {
    assert!(
        !cfg!(debug_assertions),
        "benchmarks need a release build: cargo test --release -- --ignored bench"
    );
}

/// Runs scripts/pty_bench.py (which spawns the real binary in a pty) and returns the key=value
/// lines it prints. `None`, after saying why, when python3 is missing.
pub fn pty_bench(args: &[&str]) -> Option<std::collections::HashMap<String, f64>> {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/pty_bench.py");
    let out = match std::process::Command::new("python3").arg(script).args(args).output() {
        Ok(out) => out,
        Err(e) => {
            eprintln!("skipped: python3 is needed for scripts/pty_bench.py, but: {e}");
            return None;
        }
    };
    eprint!("{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "pty_bench.py {args:?} failed");
    let out = String::from_utf8(out.stdout).unwrap();
    Some(
        out.lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_string(), v.parse().unwrap()))
            .collect(),
    )
}

/// A mark of a change from disk; `words` are the columns to paint in the line.
pub fn mark(line: usize, kind: char, words: Vec<std::ops::Range<usize>>) -> Mark {
    Mark { line, kind, words: Some(words.into()) }
}

/// The columns a mark paints; `None` is the whole line.
pub fn words(m: &Mark) -> Option<Vec<std::ops::Range<usize>>> {
    m.words.as_ref().map(|w| w.to_vec())
}

/// A hunk that is a conflict with the disk.
pub fn hunk(mine: std::ops::Range<usize>, theirs: &str, words: Option<Words>) -> Hunk {
    Hunk { mine, theirs: theirs.into(), disk: true, words: words.map(std::sync::Arc::new) }
}

/// The number of cases of a property: `SI_FUZZ_CASES` if it is set, `default` otherwise.
pub fn cases(default: u32) -> u32 {
    std::env::var("SI_FUZZ_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// `text` (its lines, no final line break) with the lines `h.mine` replaced by the lines of
/// `h.theirs`: what accepting the hunk must give (PROP-8).
pub fn splice(text: &str, h: &Hunk) -> String {
    let mut lines: Vec<&str> = text.split('\n').collect();
    let n = lines.len();
    let theirs = h.theirs.split_inclusive('\n').map(|l| l.strip_suffix('\n').unwrap_or(l));
    lines.splice(h.mine.start.min(n)..h.mine.end.min(n), theirs);
    if lines.is_empty() {
        lines.push("");
    }
    lines.join("\n")
}

/// What a decision key (`y n Y N`, pressed in REVIEW) must have done to the text and the hunks
/// (PROP-8): a reject only drops hunks, an accept splices their lines in (for hunks that do not
/// overlap, which is what a merge makes), and `y` and `n` drop exactly one.
pub fn check_decision(
    k: &str,
    pre: &str,
    pre_hunks: &[Hunk],
    post: &str,
    post_hunks: &[Hunk],
) -> Result<(), String> {
    let theirs = |v: &[Hunk]| v.iter().map(|h| h.theirs.to_string()).collect::<Vec<_>>();
    let flat = pre_hunks.windows(2).all(|p| p[0].mine.end <= p[1].mine.start);
    let fail = |why: &str| {
        Err(format!("{k}: {why}: {pre:?} -> {post:?}; hunks {pre_hunks:?} -> {post_hunks:?}"))
    };
    match k {
        "N" if post != pre || !post_hunks.is_empty() => fail("N changes the text or leaves hunks"),
        "Y" if flat
            && (!post_hunks.is_empty()
                || post != pre_hunks.iter().rev().fold(pre.to_string(), |t, h| splice(&t, h))) =>
        {
            fail("Y is not every hunk spliced in")
        }
        "y" | "n" if pre_hunks.len() != post_hunks.len() + 1 => fail("not exactly one hunk went"),
        "y" | "n" => {
            // The hunk that went is one whose removal leaves the others as they were.
            let rest = |i: usize| {
                let mut t = theirs(pre_hunks);
                t.remove(i);
                t == theirs(post_hunks)
            };
            let want =
                |i: usize| if k == "y" { splice(pre, &pre_hunks[i]) } else { pre.to_string() };
            if (0..pre_hunks.len()).any(|i| rest(i) && post == want(i)) {
                Ok(())
            } else {
                fail("the text is not the hunk's")
            }
        }
        _ => Ok(()),
    }
}

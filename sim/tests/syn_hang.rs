//! SYN-3: no text makes the highlighter spin. syntect 5.3.0 has no guard for matches that consume nothing and
//! go round in a circle (two contexts that `set` each other), and the JavaScript syntax that the set holds for
//! `source.js`, which Markdown fences, HTML's `<script>` and a dozen more syntaxes embed, has such a circle: a
//! line that ends in a block comment after an expression (`y /* c */`) makes it spin on a core for ever.
//!
//! This fuzz gives every syntax of the set random documents of tokens (operators, comment marks, quotes,
//! keywords, fences, tags), and the Markdown and HTML syntaxes the same inside a fence of each language and a
//! `<script>` and a `<style>`, and the files of this repository. A parse that does not end cannot be stopped in
//! a thread, so each syntax runs in a process of its own (this test binary, called again) with a watchdog on
//! every line: more than two seconds in debug is a hang, and the process says on which document and dies.
//!
//! It is `#[ignore]`d while syntect has no guard, since it fails (SPEC, SYN-3's known limit):
//!
//!   cargo test -p sim-editor --test syn_hang -- --ignored --nocapture
//!   SIM_HANG_DOCS=3000 SIM_HANG_SEED=7 cargo test ... (more documents of another seed)
#![allow(clippy::unwrap_used, clippy::panic)] // a test
use sim::syn::{Syn, syntaxes};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference};

/// What one line may take in debug before it is a hang (a line takes under a millisecond), and what sixteen
/// lines of a real file may (a line of 9,000 chars is slow in debug, and is not a loop).
const LINE_LIMIT: Duration = Duration::from_secs(2);
const FILE_LIMIT: Duration = Duration::from_secs(20);

const TOKENS: &[&str] = &[
    "y",
    "x",
    "foo",
    "bar_1",
    "a.b",
    "return",
    "if",
    "else",
    "for",
    "while",
    "function",
    "class",
    "def",
    "fn",
    "let",
    "var",
    "const",
    "import",
    "export",
    "from",
    "as",
    "in",
    "of",
    "new",
    "this",
    "self",
    "struct",
    "enum",
    "impl",
    "use",
    "end",
    "do",
    "then",
    "begin",
    "case",
    "when",
    "switch",
    "try",
    "catch",
    "async",
    "await",
    "yield",
    "static",
    "public",
    "type",
    "interface",
    "module",
    "package",
    "func",
    "val",
    "object",
    "trait",
    "0",
    "42",
    "3.14",
    "0xff",
    "1e9",
    "true",
    "false",
    "null",
    "nil",
    "None",
    "+",
    "-",
    "*",
    "/",
    "%",
    "<",
    ">",
    "=",
    "==",
    "=>",
    "->",
    "&",
    "&&",
    "|",
    "||",
    "^",
    "!",
    "~",
    "?",
    "??",
    ":",
    "::",
    ";",
    ",",
    ".",
    "..",
    "...",
    "@",
    "#",
    "$",
    "\\",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    "<<",
    ">>",
    "++",
    "--",
    "+=",
    "!=",
    "'",
    "\"",
    "`",
    "'a'",
    "\"s\"",
    "`t`",
    "'''",
    "\"\"\"",
    "```",
    "~~~",
    "/*",
    "*/",
    "/**",
    "//",
    "///",
    "<!--",
    "-->",
    "#!",
    "(*",
    "*)",
    "{-",
    "-}",
    "=begin",
    "=end",
    "<script>",
    "</script>",
    "<style>",
    "</style>",
    "<?php",
    "?>",
    "<%",
    "%>",
    "{{",
    "}}",
    "{%",
    "%}",
    "<div>",
    "</div>",
    "<![CDATA[",
    "]]>",
    "<<EOF",
    "EOF",
    "$(",
    "${",
    "#{",
    "\\n",
    "\\u00e9",
    "é",
    "日本",
    "\t",
    "  ",
    "    ",
    "/re/g",
    "r\"raw\"",
    "|x|",
    "#include",
    "@Override",
];

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as usize % n
    }
}

/// A document of 2 to 9 lines of up to 6 tokens.
fn document(rng: &mut Rng) -> Vec<String> {
    (0..2 + rng.below(8))
        .map(|_| {
            let mut line = if rng.below(3) == 0 { "  ".to_string() } else { String::new() };
            for _ in 0..rng.below(7) {
                line.push_str(TOKENS[rng.below(TOKENS.len())]);
                if rng.below(4) != 0 {
                    line.push(' ');
                }
            }
            line + "\n"
        })
        .collect()
}

/// What the watchdog reports if the parse in progress does not end.
type Doing = Arc<Mutex<String>>;

/// Starts the watchdog: the process dies, saying what it was parsing, when `beat` stands still for a line's limit.
fn watch(beat: &Arc<AtomicU64>, doing: &Doing, limit: Duration) {
    let (beat, doing) = (beat.clone(), doing.clone());
    std::thread::spawn(move || {
        let (mut last, mut since) = (0, Instant::now());
        loop {
            std::thread::sleep(Duration::from_millis(50));
            let now = beat.load(Ordering::Relaxed);
            if now != last {
                (last, since) = (now, Instant::now());
            } else if since.elapsed() > limit {
                println!("HANG {}", doing.lock().unwrap());
                std::process::exit(3);
            }
        }
    });
}

/// A parse with a watchdog on every line.
struct Run {
    beat: Arc<AtomicU64>,
    doing: Doing,
}

impl Run {
    fn new(limit: Duration) -> Run {
        let run = Run { beat: Arc::default(), doing: Arc::default() };
        watch(&run.beat, &run.doing, limit);
        run
    }

    /// `lines` parsed from the start with `syntax`; `what` says what they are, if one does not end.
    fn parse(&self, syntax: &SyntaxReference, lines: &[String], what: &str) {
        let (mut state, mut stack) = (ParseState::new(syntax), ScopeStack::new());
        for (n, line) in lines.iter().enumerate() {
            *self.doing.lock().unwrap() = format!("{} ({what}) line {n} of {lines:?}", syntax.name);
            self.beat.fetch_add(1, Ordering::Relaxed);
            if let Ok(ops) = state.parse_line(line, syntaxes()) {
                ops.into_iter().for_each(|(_, op)| drop(stack.apply(&op)));
            }
        }
    }
}

/// `inside` between a line that opens and a line that closes.
fn wrapped(open: String, inside: Vec<String>, close: &str) -> Vec<String> {
    std::iter::once(open).chain(inside).chain([close.to_string()]).collect()
}

/// The child for syntax number `index` of the set: random documents of its own, and as a fence of Markdown
/// (by its extension and by its name).
fn child(index: usize, docs: usize, seed: u64) {
    let (set, run) = (syntaxes(), Run::new(LINE_LIMIT));
    let syntax = &set.syntaxes()[index];
    let mut rng = Rng(seed ^ (index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    for _ in 0..docs {
        run.parse(syntax, &document(&mut rng), "random");
    }
    let markdown = set.find_syntax_by_name("Markdown").unwrap();
    let ext = syntax.file_extensions.first().cloned().unwrap_or_default();
    for _ in 0..docs / 4 {
        for (open, close) in
            [(format!("```{ext}\n"), "```\n"), (format!("~~~ {}\n", syntax.name), "~~~\n")]
        {
            let lines = wrapped(open, document(&mut rng), close);
            run.parse(markdown, &lines, &format!("Markdown with {} in a fence", syntax.name));
        }
    }
    println!("ok {index} {}", syntax.name);
}

/// The child for HTML: `<script>`, `<style>` and an attribute of events, whatever they embed.
fn html(docs: usize, seed: u64) {
    let (set, run) = (syntaxes(), Run::new(LINE_LIMIT));
    let mut rng = Rng(seed);
    for _ in 0..docs {
        for (open, close) in [
            ("<script>\n", "</script>\n"),
            ("<style>\n", "</style>\n"),
            ("<script type=\"module\">\n", "</script>\n"),
            ("<a onclick=\"\n", "\">x</a>\n"),
        ] {
            let lines = wrapped(open.to_string(), document(&mut rng), close);
            for name in ["HTML", "PHP", "Vue Component", "Svelte"] {
                run.parse(set.find_syntax_by_name(name).unwrap(), &lines, "a tag with code in it");
            }
        }
    }
    println!("ok html");
}

/// The files of this repository, each as the highlighter would see it (`Syn`, with its line limit and its
/// cached states), the Markdown ones with their fences.
fn files() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let run = Run::new(FILE_LIMIT);
    let mut stack = vec![root];
    let mut n = 0;
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let (path, name) = (e.path(), e.file_name().to_string_lossy().into_owned());
            if name == "target" || name == ".git" || name == "golden" {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let lines: Vec<String> = text.lines().take(3000).map(str::to_string).collect();
            let Some(mut syn) = Syn::new(&name, lines.first().map_or("", String::as_str)) else {
                continue;
            };
            *run.doing.lock().unwrap() = format!("the file {}", path.display());
            for from in (0..lines.len()).step_by(16) {
                run.beat.fetch_add(1, Ordering::Relaxed);
                syn.spans(&|i| lines.get(i).cloned(), from, 16);
            }
            n += 1;
        }
    }
    println!("ok files {n}");
}

/// The process that runs the children: one for each syntax of the set, a few at a time.
#[test]
#[ignore = "syntect has no guard for matches that consume nothing: the JavaScript syntax spins (SPEC SYN-3)"]
fn syn_3_no_syntax_of_the_set_makes_the_highlighter_spin() {
    if let Ok(index) = std::env::var("SIM_HANG_CHILD") {
        let docs = std::env::var("SIM_HANG_DOCS").ok().and_then(|d| d.parse().ok()).unwrap_or(300);
        let seed = std::env::var("SIM_HANG_SEED").ok().and_then(|d| d.parse().ok()).unwrap_or(1);
        return match index.as_str() {
            "files" => files(),
            "html" => html(docs / 4, seed),
            n => child(n.parse().unwrap(), docs, seed),
        };
    }
    let exe = std::env::current_exe().unwrap();
    let count = syntaxes().syntaxes().len();
    let mut todo: Vec<String> = ["files", "html"]
        .map(String::from)
        .into_iter()
        .chain((0..count).map(|i| i.to_string()))
        .collect();
    let (mut running, mut hangs, mut done) = (Vec::new(), Vec::new(), 0);
    while !todo.is_empty() || !running.is_empty() {
        while running.len() < 4 && !todo.is_empty() {
            let which = todo.pop().unwrap();
            let name = "syn_3_no_syntax_of_the_set_makes_the_highlighter_spin";
            let child = Command::new(&exe)
                .args(["--exact", "--ignored", "--nocapture", name])
                .env("SIM_HANG_CHILD", which)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            running.push(child);
        }
        // (a child ends by itself: its watchdog stops it at the first line that does not)
        let (mut ended, mut still) = (vec![], vec![]);
        for mut c in running {
            if c.try_wait().unwrap().is_some() { ended.push(c) } else { still.push(c) }
        }
        running = still;
        for c in ended {
            let out = String::from_utf8_lossy(&c.wait_with_output().unwrap().stdout).into_owned();
            match out.lines().find(|l| l.starts_with("HANG")) {
                Some(l) => hangs.push(l.chars().take(400).collect::<String>()),
                None => done += 1,
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("{done} of {} runs ended, {} hung", count + 2, hangs.len());
    assert!(hangs.is_empty(), "{} runs spin:\n{}", hangs.len(), hangs.join("\n"));
}

//! SYN-1, SYN-3, SYN-4
//! syntect scopes to typographic spans per line, never a color, with the parse state cached
//! every [`EVERY`] lines so that an edit re-parses from the nearest state above it (pure, apart
//! from loading the syntax set once).
use std::ops::Range;
use std::path::Path;
use std::sync::OnceLock;
use syntect::parsing::{ParseState, Scope, ScopeStack, ScopeStackOp, SyntaxSet};

/// What a char looks like. The order is the precedence: where the scopes of a char disagree the
/// first wins, so a keyword-like word inside a comment or a string stays dim or italic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Attr {
    Dim,
    Italic,
    Bold,
}

/// Char ranges of a line, in order and apart. Chars outside every span are plain.
pub type Spans = Vec<(Range<usize>, Attr)>;

/// Line `i` without its line break; none past the end.
pub type Text<'a> = &'a dyn Fn(usize) -> Option<String>;

/// The parse state is kept at the start of every 16th line. A line takes about 40 us to parse
/// (release, fancy-regex), so after an edit the edited line is shown again after at most 15
/// lines of parsing: 0.6 ms measured, inside the 1 ms a key may take (SPEED-1). 32 measured
/// 1.2 ms, over it; 8 would cost five times the memory for 0.2 ms (1.1 MB resident for the
/// states of 70,000 lines at 16, measured).
pub const EVERY: usize = 16;

/// The bundled syntaxes. The first call decodes their index (0.3 ms in release), and the first
/// highlight of a language its contexts and regexes (17 ms for Rust): do both in a thread,
/// never before the first frame.
pub fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// What each scope looks like; the longest scope that is a prefix wins, so a more specific
/// one can take it back (`None`: plain). Markup is typography already: headings and strong
/// are bold, emphasis italic.
const RULES: [(&str, Option<Attr>); 12] = [
    ("comment", Some(Attr::Dim)),
    ("string", Some(Attr::Italic)),
    ("string.unquoted", None), // a bare word, YAML's plain value, bash's `$(...)`: not a literal
    ("constant.character", Some(Attr::Italic)),
    ("markup.italic", Some(Attr::Italic)),
    ("keyword", Some(Attr::Bold)),
    ("keyword.operator", None),
    ("storage", Some(Attr::Bold)),
    ("storage.type.string", None),
    ("storage.modifier.lifetime", None),
    ("markup.heading", Some(Attr::Bold)),
    ("markup.bold", Some(Attr::Bold)),
];

fn attr_of(scope: Scope) -> Option<Attr> {
    static RULES_SCOPES: OnceLock<Vec<(Scope, Option<Attr>)>> = OnceLock::new();
    let rules = RULES_SCOPES.get_or_init(|| {
        RULES.iter().filter_map(|&(s, a)| Some((Scope::new(s).ok()?, a))).collect()
    });
    let matching = rules.iter().filter(|(r, _)| r.is_prefix_of(scope));
    matching.max_by_key(|(r, _)| r.len()).and_then(|&(_, a)| a)
}

pub struct Syn {
    /// The language, e.g. `Rust`.
    pub name: &'static str,
    /// The state at the start of line `i * EVERY`, for as many as are known to be valid.
    states: Vec<(ParseState, ScopeStack)>,
    /// Lines parsed so far (for tests and benches).
    pub parsed: usize,
}

impl Syn {
    /// SYN-4: the syntax for a file, from its name (the whole name, then the extension: `Makefile`,
    /// `.bashrc`, `a.rs`), then from its first line (`#!/usr/bin/env python`). None if there is
    /// none, or only plain text.
    pub fn new(name: &str, first_line: &str) -> Option<Syn> {
        let set = syntaxes();
        let path = Path::new(name);
        let by_name = path.file_name().and_then(|f| set.find_syntax_by_extension(f.to_str()?));
        let by_ext = || path.extension().and_then(|e| set.find_syntax_by_extension(e.to_str()?));
        let syntax =
            by_name.or_else(by_ext).or_else(|| set.find_syntax_by_first_line(first_line))?;
        let start = (ParseState::new(syntax), ScopeStack::new());
        (syntax.name != "Plain Text").then_some(Syn {
            name: &syntax.name,
            states: vec![start],
            parsed: 0,
        })
    }

    /// A line `line` was edited, or lines were inserted or removed there: the states below it are stale.
    pub fn edit(&mut self, line: usize) {
        self.states.truncate(line / EVERY + 1);
    }

    /// How many lines must be parsed to get from the nearest valid state to `line`: if that is
    /// a lot, do it in the background (SYN-3) and show the lines plain meanwhile.
    pub fn distance(&self, line: usize) -> usize {
        line - (line / EVERY).min(self.states.len() - 1) * EVERY
    }

    /// The spans of lines `from..from + n` (fewer if `text` ends). Parses from the nearest
    /// valid state, and keeps new ones on the way.
    pub fn spans(&mut self, text: Text, from: usize, n: usize) -> Vec<Spans> {
        let first = (from / EVERY).min(self.states.len() - 1) * EVERY;
        let (mut parse, mut stack) = self.states[first / EVERY].clone();
        let mut out = vec![];
        for i in first..from.saturating_add(n) {
            let Some(line) = text(i) else { break };
            let spans = self.line(&mut parse, &mut stack, &line);
            if i >= from {
                out.push(spans);
            }
            if (i + 1) % EVERY == 0 && (i + 1) / EVERY == self.states.len() {
                self.states.push((parse.clone(), stack.clone()));
            }
        }
        out
    }

    fn line(&mut self, parse: &mut ParseState, stack: &mut ScopeStack, line: &str) -> Spans {
        self.parsed += 1;
        let text = format!("{line}\n");
        let Ok(ops) = parse.parse_line(&text, syntaxes()) else { return vec![] };
        let (mut spans, mut at, mut col, mut attr) = (Spans::new(), 0, 0, look(stack));
        let width = line.chars().count();
        for (off, op) in ops.into_iter().chain([(text.len(), ScopeStackOp::Noop)]) {
            let end = (col + text[at..off].chars().count()).min(width);
            match (attr, spans.last_mut()) {
                (Some(a), Some((r, b))) if a == *b && r.end == col => r.end = end,
                (Some(a), _) if end > col => spans.push((col..end, a)),
                _ => {}
            }
            (at, col) = (off, end);
            let _ = stack.apply(&op);
            attr = look(stack);
        }
        spans
    }
}

/// How a char looks while this is the stack of scopes it is in.
fn look(stack: &ScopeStack) -> Option<Attr> {
    stack.as_slice().iter().filter_map(|&s| attr_of(s)).min()
}

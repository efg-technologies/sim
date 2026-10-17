//! SYN-1, SYN-3, SYN-4, UI-8
//! syntect scopes to looks per line (keyword, string, number, type, function, comment), with the
//! parse state cached every [`EVERY`] lines so that an edit re-parses from the nearest state
//! above it (pure, apart from loading the syntax set once).
use crate::icons;
use crate::paint::Look;
use std::ops::Range;
use std::path::Path;
use std::sync::OnceLock;
use syntect::parsing::{ParseState, Scope, ScopeStack, ScopeStackOp, SyntaxReference, SyntaxSet};

/// Char ranges of a line, in order and apart. Chars outside every span are plain.
pub type Spans = Vec<(Range<usize>, Look)>;

/// Line `i` without its line break; none past the end.
pub type Text<'a> = &'a dyn Fn(usize) -> Option<String>;

/// The parse state is kept at the start of every 16th line. A line takes about 40 us to parse
/// (release, fancy-regex), so after an edit the edited line is shown again after at most 15
/// lines of parsing: 0.6 ms measured, inside the 1 ms a key may take (SPEED-1). 32 measured
/// 1.2 ms, over it; 8 would cost five times the memory for 0.2 ms (1.1 MB resident for the
/// states of 70,000 lines at 16, measured).
pub const EVERY: usize = 16;

/// A line of more than this many bytes is plain and is not parsed: some syntaxes take time that
/// grows with the square of a line (a shell heredoc whose word is 100,000 chars: 5.8 s), and the
/// highlighter is one thread for every file. Code with lines this long is minified or generated.
pub const MAX_LINE: usize = 10_000;

/// The syntaxes of bat (the `two-face` crate), which has the languages syntect's own set lacks
/// (SYN-4). The first call decodes their index, and the first highlight of a language its
/// contexts and regexes: do both in a thread, never before the first frame.
pub fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

/// What each scope looks like; the longest scope that is a prefix wins, so a more specific
/// one can take it back (`None`: plain). Operators, names and punctuation are plain. Markup is
/// typography (UI-8): headings by level, emphasis, strong, links, bullets and quotes, and a dim
/// for the markers that make them, none of which is hidden. Blue is never used (SYN-1).
const RULES: &[(&str, Option<Look>)] = &[
    ("comment", Some(Look::Com)),
    ("string", Some(Look::Str)),
    ("string.unquoted", None), // a bare word, YAML's plain value, bash's `$(...)`: not a literal
    ("constant", Some(Look::Num)),
    ("constant.character", Some(Look::Str)),
    ("keyword", Some(Look::Kw)),
    ("keyword.operator", None),
    ("keyword.operator.dockerfile", Some(Look::Kw)), // `CMD` is an operator to that syntax
    ("storage", Some(Look::Kw)),
    ("storage.type.string", None), // the `f` of an f-string
    ("storage.modifier.lifetime", None),
    ("variable.language", Some(Look::Kw)), // this, self, super
    ("support.type", Some(Look::Ty)),
    ("entity.name.type", Some(Look::Ty)),
    ("entity.name.class", Some(Look::Ty)),
    ("entity.name.struct", Some(Look::Ty)),
    ("entity.name.enum", Some(Look::Ty)),
    ("entity.name.trait", Some(Look::Ty)),
    ("entity.other.inherited-class", Some(Look::Ty)),
    ("entity.name.function", Some(Look::Fun)),
    ("support.function", Some(Look::Fun)),
    ("support.macro", Some(Look::Fun)),
    ("variable.function", Some(Look::Fun)),
    ("markup.heading.1", Some(Look::H1)),
    ("markup.heading.2", Some(Look::H2)),
    ("markup.heading.3", Some(Look::H3)),
    ("markup.heading.4", Some(Look::H4)),
    ("markup.heading.5", Some(Look::H5)),
    ("markup.heading.6", Some(Look::H6)),
    ("markup.heading", Some(Look::H3)), // levels 3 to 6 have no number in bat's syntax: see `Syn::line`
    ("punctuation.definition.heading", Some(Look::Mark)),
    ("markup.bold", Some(Look::Bold)),
    ("markup.italic", Some(Look::Em)),
    ("punctuation.definition.bold", Some(Look::Mark)),
    ("punctuation.definition.italic", Some(Look::Mark)),
    ("markup.raw.inline", Some(Look::Code)),
    ("punctuation.definition.raw", Some(Look::Tick)),
    ("punctuation.definition.raw.code-fence", Some(Look::Mark)),
    ("meta.code-fence.definition", Some(Look::Mark)),
    ("constant.other.language-name", Some(Look::Lang)),
    ("meta.link.inline.description", Some(Look::Link)),
    ("meta.link.reference.description", Some(Look::Link)),
    ("meta.image.inline.description", Some(Look::Link)),
    ("markup.underline.link", Some(Look::Url)),
    ("punctuation.definition.image", Some(Look::Mark)),
    ("punctuation.definition.constant", Some(Look::Mark)),
    ("punctuation.definition.link", Some(Look::Mark)),
    ("punctuation.definition.metadata", Some(Look::Mark)),
    ("markup.list.unnumbered.bullet", Some(Look::Bullet)),
    ("markup.list.numbered.bullet", Some(Look::Bullet)),
    ("punctuation.definition.blockquote", Some(Look::Quote)),
    ("meta.separator.thematic-break", Some(Look::Rule)),
    ("meta.table.header-separator", Some(Look::Mark)), // `|---|:--:|`, end to end: layout draws it as a grid
    ("punctuation.separator.table-cell", Some(Look::Mark)),
    ("punctuation.section.table-header", Some(Look::Mark)),
];

/// UI-8: whether a scope is a line of a fenced block (its fences and what is between them).
fn fenced(scope: Scope) -> bool {
    static SCOPES: OnceLock<Vec<Scope>> = OnceLock::new();
    let all = SCOPES.get_or_init(|| {
        ["markup.raw.code-fence", "meta.code-fence"]
            .iter()
            .filter_map(|s| Scope::new(s).ok())
            .collect()
    });
    all.iter().any(|s| s.is_prefix_of(scope))
}

fn attr_of(scope: Scope) -> Option<Look> {
    static RULES_SCOPES: OnceLock<Vec<(Scope, Option<Look>)>> = OnceLock::new();
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

/// SYN-4: the extensions of JavaScript that are read with TypeScript's syntax (see `Syn::new`).
fn alias(ext: &str) -> String {
    match ext.to_ascii_lowercase().as_str() {
        "js" | "mjs" | "cjs" | "es6" => "ts".into(),
        "jsx" => "tsx".into(),
        e => e.into(),
    }
}

/// SYN-4: the syntax for a file's name: the whole name, then the extension (`Makefile`, `.bashrc`, `a.rs`);
/// for a name of chezmoi's (`dot_zshrc`, `dot_gitconfig.tmpl`), what it stands for, which is what the
/// file is on the machine.
fn by_name(name: &str) -> Option<&'static SyntaxReference> {
    let set = syntaxes();
    let by = |path: &Path| {
        let by_name = path.file_name().and_then(|f| set.find_syntax_by_extension(f.to_str()?));
        by_name.or_else(|| {
            path.extension().and_then(|e| set.find_syntax_by_extension(&alias(e.to_str()?)))
        })
    };
    let file = Path::new(name).file_name().and_then(|f| f.to_str()).unwrap_or(name);
    let stands = icons::target(file);
    let (own, chezmoi) = (by(Path::new(name)), by(Path::new(&stands)));
    if stands == file.to_ascii_lowercase() { own } else { chezmoi.or(own) }
}

/// MD-10: whether the highlighter calls a file Markdown by its name (`.md`, `.mdown`, `.mkd`, `.ronn`, …:
/// the names of its table, not a list of sim's).
pub fn is_markdown(name: &str) -> bool {
    by_name(name).is_some_and(|s| s.name == "Markdown")
}

impl Syn {
    /// SYN-4: the syntax for a file, from its name (the whole name, then the extension: `Makefile`,
    /// `.bashrc`, `a.rs`), then from its first line (`#!/usr/bin/env python`). None if there is
    /// none, or only plain text. bat's "JavaScript" is never used: with the pure-Rust regex engine
    /// syntect 5.3 loops for ever on `x = y /* c */` in it, and the set has no `.mjs`, `.cjs` or
    /// `.jsx`; TypeScript's syntax reads JavaScript, so that is what these files get.
    pub fn new(name: &str, first_line: &str) -> Option<Syn> {
        let set = syntaxes();
        let named = by_name(name);
        let found = named.or_else(|| set.find_syntax_by_first_line(first_line))?;
        let ts = set.find_syntax_by_extension("ts");
        let syntax = if found.name == "JavaScript" { ts.unwrap_or(found) } else { found };
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

    /// The spans of lines `from..from + n` (fewer if `text` ends). Parses from the nearest
    /// valid state, and keeps new ones on the way.
    pub fn spans(&mut self, text: Text, from: usize, n: usize) -> Vec<Spans> {
        let first = (from / EVERY).min(self.states.len() - 1) * EVERY;
        let (mut parse, mut stack) = self.states[first / EVERY].clone();
        let mut out = vec![];
        for i in first..from.saturating_add(n) {
            let Some(line) = text(i) else { break };
            let mut spans = self.line(&mut parse, &mut stack, &line);
            if i == 0 {
                // the first line of a file opens YAML front matter, which bat's markdown does not know
                spans.retain(|s| s.1 != Look::Rule);
            }
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
        if line.len() > MAX_LINE {
            return vec![]; // the state stays as it was: the lines after it do not start from a guess
        }
        self.parsed += 1;
        let text = format!("{line}\n");
        let Ok(ops) = parse.parse_line(&text, syntaxes()) else { return vec![] };
        let (mut spans, mut at, mut col, mut attr) = (Spans::new(), 0, 0, look(stack));
        let (width, mut band) = (line.chars().count(), false);
        for (off, op) in ops.into_iter().chain([(text.len(), ScopeStackOp::Noop)]) {
            band |= off > at && stack.as_slice().iter().any(|&s| fenced(s));
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
        // A line of a fenced block starts with a span of no width, for the surface across it: a
        // blank line in the block has no char to carry one (UI-8).
        if band {
            spans.insert(0, (0..0, Look::Band));
        } else if self.name.starts_with("Markdown") {
            done(line, &mut spans);
        }
        // bat's syntax numbers headings 1 and 2 only: the others are `H3` here, and the count of
        // their `#`s says the level.
        let level = [Look::H3, Look::H4, Look::H5, Look::H6]
            [line.chars().take_while(|&c| c == '#').count().clamp(3, 6) - 3];
        spans.iter_mut().filter(|s| s.1 == Look::H3).for_each(|s| s.1 = level);
        spans
    }
}

/// UI-8: the text of a task that is done, `- [x] text` (a bullet or a number, a blank, `[x]` or
/// `[X]` and a blank), gets `Look::Done` where no other look of the line has it (code, a link, bold
/// keep theirs). The line is not in a fenced block.
fn done(line: &str, spans: &mut Spans) {
    let cs: Vec<char> = line.chars().collect();
    let blanks = |i: usize| cs[i.min(cs.len())..].iter().take_while(|c| **c == ' ').count();
    let mut i = cs.iter().take_while(|c| matches!(c, ' ' | '\t')).count();
    match cs.get(i) {
        Some('-' | '*' | '+') => i += 1,
        Some(c) if c.is_ascii_digit() => {
            i += cs[i..].iter().take_while(|c| c.is_ascii_digit()).count();
            if !matches!(cs.get(i), Some('.' | ')')) {
                return;
            }
            i += 1;
        }
        _ => return,
    }
    let b = blanks(i);
    i += b;
    if b == 0 || !matches!(cs.get(i..i + 3), Some(['[', 'x' | 'X', ']'])) || blanks(i + 3) == 0 {
        return;
    }
    let start = i + 3 + blanks(i + 3);
    let (mut at, mut gaps) = (start, vec![]);
    for (r, _) in spans.iter().filter(|(r, _)| r.end > start) {
        if r.start > at {
            gaps.push((at..r.start, Look::Done));
        }
        at = at.max(r.end);
    }
    if at < cs.len() {
        gaps.push((at..cs.len(), Look::Done));
    }
    spans.extend(gaps);
    spans.sort_by_key(|s| (s.0.start, s.0.end));
}

/// How a char looks while this is the stack of scopes it is in.
fn look(stack: &ScopeStack) -> Option<Look> {
    stack.as_slice().iter().filter_map(|&s| attr_of(s)).min()
}

//! UI-18
//! The breadcrumb `dir › file › section` (pure): the section the cursor is in, found by a few
//! string tests over the lines above it (a Markdown heading, or the definition it is inside), the
//! parts of the path, and how they are cut from the left. No parse: a definition the layout of the
//! file hides from it is not found, and none is made up.
use crate::layout::{fit, tail};
use ropey::{Rope, RopeSlice};
use std::path::Path;

/// How many lines up the section is looked for, and how much of each line is read.
const SCAN: usize = 400;
const LINE: usize = 200;
/// What stands between the parts.
pub const SEP: &str = " › ";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Md,
    Rust,
    Py,
    Js,
    Go,
    Rb,
    Sh,
    Lua,
}

fn kind(path: &str) -> Option<Kind> {
    let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "md" | "markdown" => Kind::Md,
        "rs" => Kind::Rust,
        "py" => Kind::Py,
        "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "mts" => Kind::Js,
        "go" => Kind::Go,
        "rb" => Kind::Rb,
        "sh" | "bash" | "zsh" => Kind::Sh,
        "lua" => Kind::Lua,
        _ => return None,
    })
}

/// The leading blanks of a line, none for a blank one: no copy of the line is made for it.
fn indent(line: RopeSlice) -> Option<usize> {
    let mut n = 0;
    for c in line.chars() {
        match c {
            ' ' | '\t' => n += 1,
            '\n' | '\r' => return None,
            _ => return Some(n),
        }
    }
    None
}

/// The first `LINE` chars of a line, without its break.
fn head(line: RopeSlice) -> String {
    line.chars().take(LINE).filter(|c| !matches!(c, '\n' | '\r')).collect()
}

/// The section line `cur` of the file `path` is in, if it has one. `fenced` is whether the cursor's
/// line is in a fenced block of a Markdown file, and `above` whether the line above it is (what
/// tells the closing fence from the opening one when the cursor is on a fence). A line is copied
/// only when its indentation or its first char says it may start a definition or a heading: the
/// lines of a body are not. The name is as the file has it: `parts` makes it fit to be drawn.
pub fn section(path: &str, cur: usize, text: &Rope, fenced: bool, above: bool) -> Option<String> {
    let first = cur.saturating_sub(SCAN);
    let get = |l: usize| (l < text.len_lines()).then(|| text.line(l));
    match kind(path)? {
        Kind::Md => {
            let mut inside = fenced;
            for l in (first..=cur).rev() {
                let line = get(l)?;
                if !matches!(line.chars().find(|&c| c != ' '), Some('#' | '`' | '~')) {
                    continue;
                }
                let t = head(line);
                let t = t.trim_start();
                if t.starts_with("```") || t.starts_with("~~~") {
                    // (the cursor's own fence line is a closing one if the line above is in the block)
                    inside = if l == cur { above } else { !inside };
                } else if let Some(h) = heading(t).filter(|_| !inside) {
                    return Some(h);
                }
            }
            None
        }
        k => {
            let own = head(get(cur)?);
            if let Some(n) = define(k, &own) {
                return Some(n);
            }
            // a closing line belongs to the block it closes; a blank one to the lesser of its
            // neighbors' indentation (a blank after a method is the class's)
            let mut limit = if let Some(i) = indent(get(cur)?) {
                i + usize::from(closes(&own))
            } else {
                let below = (cur + 1..cur + 20).filter_map(get).find_map(indent);
                (first..cur)
                    .rev()
                    .filter_map(get)
                    .find_map(indent)
                    .map_or(usize::MAX, |p| below.map_or(p, |b| p.min(b)))
            };
            for l in (first..cur).rev() {
                let line = get(l)?;
                let Some(i) = indent(line).filter(|&i| i < limit) else { continue };
                if let Some(n) = define(k, &head(line)) {
                    return Some(n);
                }
                limit = i;
            }
            None
        }
    }
}

/// A line that closes a block: `}`, `)`, `]`, `end`, `fi`, `done`, `esac`.
fn closes(t: &str) -> bool {
    let t = t.trim_start();
    t.starts_with(['}', ')', ']']) || matches!(ident(t).as_str(), "end" | "fi" | "done" | "esac")
}

/// The text of a Markdown heading (ATX: one to six `#`s and a blank), without its `#`s.
fn heading(t: &str) -> Option<String> {
    let n = t.chars().take_while(|&c| c == '#').count();
    let rest = t[n..].strip_prefix([' ', '\t'])?;
    let rest = rest.trim();
    let text = match rest.trim_end_matches('#') {
        cut if cut.len() < rest.len() && cut.ends_with(' ') => cut.trim_end(),
        _ => rest,
    };
    ((1..=6).contains(&n) && !text.is_empty()).then(|| text.to_string())
}

/// The identifier at the start of `s`: letters, digits, `_`, and the `.`, `?`, `!`, `$` and `::`
/// of the names of methods and paths.
fn ident(s: &str) -> String {
    let (mut out, mut it) = (String::new(), s.chars().peekable());
    while let Some(&c) = it.peek() {
        let more = c.is_alphanumeric() || matches!(c, '_' | '.' | '?' | '!' | '$');
        if more || (c == ':' && s[out.len()..].starts_with("::")) {
            out.push(c);
            it.next();
            if c == ':' {
                out.extend(it.next());
            }
        } else {
            break;
        }
    }
    out
}

/// The word `w` at the start of `s`, with a blank after it: what follows.
fn after<'a>(s: &'a str, w: &str) -> Option<&'a str> {
    s.strip_prefix(w).and_then(|r| r.strip_prefix([' ', '\t'])).map(str::trim_start)
}

/// The name of the definition a line starts, if it starts one, by the words the language uses.
fn define(k: Kind, line: &str) -> Option<String> {
    let mut t = line.trim_start();
    // what may come before the word: visibility, `async`, `export`, `static`...
    loop {
        let w = ident(t);
        let skip = matches!(
            (k, w.as_str()),
            (Kind::Rust, "pub" | "async" | "unsafe" | "const" | "default" | "extern")
                | (
                    Kind::Js,
                    "export"
                        | "default"
                        | "async"
                        | "static"
                        | "public"
                        | "private"
                        | "protected"
                        | "readonly"
                        | "abstract"
                        | "get"
                        | "set"
                        | "declare"
                )
                | (Kind::Py, "async")
                | (Kind::Lua, "local")
        );
        if !skip {
            break;
        }
        t = t[w.len()..].trim_start();
        // `pub(crate)` and `extern "C"`
        if t.starts_with('(') && w == "pub" {
            t = t.split_once(')').map_or("", |x| x.1).trim_start();
        }
        if t.starts_with('"') {
            t = t[1..].split_once('"').map_or("", |x| x.1).trim_start();
        }
        if k == Kind::Js && t.starts_with('*') {
            break;
        }
    }
    let name = |rest: &str| Some(ident(rest)).filter(|n| !n.is_empty());
    let words: &[&str] = match k {
        Kind::Rust => &["fn", "struct", "enum", "trait", "mod", "union"],
        Kind::Py => &["def", "class"],
        Kind::Js => &["function", "class", "interface"],
        Kind::Go => &["type"],
        Kind::Rb => &["def", "class", "module"],
        Kind::Sh => &["function"],
        Kind::Lua => &["function"],
        Kind::Md => &[],
    };
    if k == Kind::Rust
        && let Some(rest) = t.strip_prefix("impl").filter(|r| r.starts_with(['<', ' ']))
    {
        // `impl<T> Trait for Foo<T> {`: what the line says up to the brace, without the generics of `impl`
        let rest = rest.trim_start();
        let rest = if rest.starts_with('<') { skip_angle(rest) } else { rest };
        let rest = rest.split(['{', ';']).next().unwrap_or("");
        let rest = rest.split(" where ").next().unwrap_or("").trim();
        return (!rest.is_empty()).then(|| format!("impl {rest}"));
    }
    if let Some(n) = words.iter().find_map(|w| after(t, w)).and_then(name) {
        return Some(n);
    }
    match k {
        // `func (s *S) Run() {`: the receiver is not the name
        Kind::Go => after(t, "func").and_then(|r| {
            let r = if r.starts_with('(') {
                r.split_once(')').map_or("", |x| x.1).trim_start()
            } else {
                r
            };
            name(r)
        }),
        // `foo() {` and `function foo()`
        Kind::Sh => {
            let n = ident(t);
            let rest = t[n.len()..].trim_start();
            (!n.is_empty() && rest.starts_with("()")).then_some(n)
        }
        // a method: `name(...) {` at the end of the line, which is no statement
        Kind::Js => {
            let n = ident(t.strip_prefix('*').unwrap_or(t).trim_start());
            let stmt = ["if", "for", "while", "switch", "catch", "return", "else", "with", "new"];
            let rest = t[t.find(&n)? + n.len()..].trim_start();
            (!n.is_empty()
                && !stmt.contains(&n.as_str())
                && rest.starts_with('(')
                && t.trim_end().ends_with('{'))
            .then_some(n)
        }
        _ => None,
    }
}

/// `s` without the `<...>` it starts with (nested ones too).
fn skip_angle(s: &str) -> &str {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        depth += i32::from(c == '<') - i32::from(c == '>');
        if depth == 0 {
            return s[i + 1..].trim_start();
        }
    }
    ""
}

/// The parts of the breadcrumb: the components of the path, and the section.
/// A part is made fit to be drawn, as `fit` does it (SAFE-8): a directory's name, a heading and a
/// definition's name can hold anything, a control char that a terminal would obey included.
pub fn parts(path: &str, section: Option<&str>) -> Vec<String> {
    let dirs = path.split('/').filter(|p| !p.is_empty() && *p != ".");
    dirs.chain(section).map(|p| fit(p, usize::MAX).0).collect()
}

/// The parts that fit `max` columns when joined by `SEP`: whole components are dropped from the
/// left for an `…` first, and the last part alone is cut from its left.
pub fn cut(parts: &[String], max: usize) -> Vec<String> {
    let cols = |v: &[String]| fit(&v.join(SEP), usize::MAX).1;
    let Some(last) = parts.last().filter(|_| max > 0) else { return vec![] };
    if cols(parts) <= max {
        return parts.to_vec();
    }
    for k in 1..parts.len() {
        let v: Vec<String> =
            std::iter::once("…".to_string()).chain(parts[k..].iter().cloned()).collect();
        if cols(&v) <= max {
            return v;
        }
    }
    vec![tail(last, max)]
}

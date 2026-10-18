//! VI-4, VI-7, VI-14, FILT-1, DISK-6, PERF-4, SAFE-7, PLAIN-1
//! The command line: ex commands with their ranges, `:s`, filters, and search.
use super::app::{App, Effect, Resume, Run, plural};
use super::vi::Mode;
use regex_lite::{Captures, Regex, escape};
use std::{borrow::Cow, ops::Range, thread};

/// The replacement of `:s`: `&` and `\1`-`\9` are parts of the match, `\x` is `x`.
fn expand(rep: &str, caps: &Captures) -> String {
    let (mut out, mut it) = (String::new(), rep.chars());
    while let Some(c) = it.next() {
        match c {
            '&' => out.push_str(&caps[0]),
            '\\' => match it.next() {
                Some(d @ '1'..='9') => {
                    out += caps.get(d as usize - '0' as usize).map_or("", |m| m.as_str())
                }
                Some(d) => out.push(d),
                None => out.push('\\'),
            },
            _ => out.push(c),
        }
    }
    out
}

/// A buffer of this many bytes or more is searched on 8 threads. Starting and joining them takes
/// about 60 µs (p99 120 µs, measured). At 512 KiB a search takes 1.1 ms (literal) to 11.5 ms
/// (regex) on one thread and 0.3 to 2.5 ms on eight, so start-up is small there, while below it
/// even the slowest one-thread search stays under a quarter of PERF-4's 50 ms.
const PAR_MIN: usize = 512 << 10;
/// The first steps of a search run before any thread starts, so a match near the cursor costs none.
const HEAD: usize = 256;

/// A literal that every match of `pat` contains ("" if none is found): the longest run of plain
/// chars that no quantifier binds. Alternation, groups, classes and hex escapes give up, so a
/// line without it can never match, and needs no regex (PERF-4).
fn literal(pat: &str) -> String {
    let (mut run, mut it) = (String::new(), pat.chars());
    while let Some(c) = it.next() {
        match c {
            '|' | '(' | '[' => return String::new(),
            '\\' => match it.next() {
                Some(e) if "\\.+*?()|[]{}^$#&-~".contains(e) => run.push(e),
                Some('x' | 'u' | 'U') | None => return String::new(),
                _ => run.push('\0'),
            },
            '*' | '+' | '?' | '{' => {
                run.pop();
                run.push('\0');
                if c == '{' {
                    it.by_ref().find(|&d| d == '}');
                }
            }
            '.' | '^' | '$' => run.push('\0'),
            _ => run.push(c),
        }
    }
    run.split('\0').max_by_key(|s| s.chars().count()).unwrap_or_default().into()
}

/// Whether a search ignores case: smartcase, which looks at letters only, so an escape such as
/// `\W` or `\x4A` does not count (VI-4). `cs`: it was asked to be case-sensitive.
pub fn folds(pat: &str, cs: bool) -> bool {
    let mut it = pat.chars();
    while let Some(c) = it.next() {
        if c == '\\'
            && let Some(e) = it.next()
            && let Some(n) = "xuU".find(e).map(|i| [2, 4, 8][i])
        {
            if it.as_str().starts_with('{') {
                it.find(|&d| d == '}'); // `\x{4A}`
            } else {
                it.by_ref().take(n).for_each(drop); // `\x4A`
            }
        } else if c.is_uppercase() {
            return false;
        }
    }
    !cs
}

impl App {
    fn regex(&mut self, pat: &str, cs: bool) -> Option<Regex> {
        let ci = folds(pat, cs);
        let re = Regex::new(&if ci { format!("(?i){pat}") } else { pat.to_string() });
        re.map_or_else(
            |e| self.fail(format!("bad pattern: {}", e.to_string().lines().next().unwrap_or(""))),
            Some,
        )
    }

    /// A submitted command line, with its `:`, `/` or `?`.
    pub(super) fn submit(&mut self, line: &str) {
        let mut cs = line.chars();
        match cs.next() {
            Some(':') => self.ex(cs.as_str()),
            Some(c) => {
                let (pat, fwd) = (cs.as_str(), c == '/');
                match (pat, &mut self.search) {
                    ("", Some(s)) => s.2 = fwd,
                    ("", None) => return self.say("no previous pattern"),
                    _ => self.search = Some((pat.into(), false, fwd)),
                }
                if let Some(t) = self.find_match(fwd) {
                    (self.buf.cursor, self.buf.want) = (t, None);
                    self.clamp();
                }
            }
            None => {}
        }
    }

    /// One line address (0-based) at the start of `s`, and the rest.
    fn addr<'a>(&self, s: &'a str) -> Option<(usize, &'a str)> {
        let (b, last) = (&self.buf, self.buf.lines() - 1);
        Some(match s.chars().next()? {
            '.' => (b.line_of(b.cursor), &s[1..]),
            '$' => (last, &s[1..]),
            '\'' if s[1..].starts_with('<') => (self.marks.0.min(last), &s[2..]),
            '\'' if s[1..].starts_with('>') => (self.marks.1.min(last), &s[2..]),
            '0'..='9' => {
                let d = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
                ((s[..d].parse::<usize>().ok()?.max(1) - 1).min(last), &s[d..])
            }
            _ => return None,
        })
    }

    /// The range at the start of `s` (0-based lines, first <= last) and the rest.
    fn range<'a>(&self, s: &'a str) -> (Option<(usize, usize)>, &'a str) {
        if let Some(rest) = s.strip_prefix('%') {
            return (Some((0, self.buf.lines() - 1)), rest);
        }
        let Some((a, rest)) = self.addr(s) else { return (None, s) };
        match rest.strip_prefix(',').and_then(|r| self.addr(r)) {
            Some((z, rest)) => (Some((a.min(z), a.max(z))), rest),
            None => (Some((a, a)), rest),
        }
    }

    /// Runs an ex command, without its `:`.
    pub(super) fn ex(&mut self, line: &str) {
        let (range, rest) = self.range(line.trim());
        let rest = rest.trim_start();
        if let Some(cmd) = rest.strip_prefix('!') {
            let l = self.buf.line_of(self.buf.cursor);
            return self.filter(range.unwrap_or((l, l)), cmd.trim());
        }
        let (name, arg) =
            rest.split_at(rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len()));
        let (bang, arg) = arg.strip_prefix('!').map_or((false, arg), |a| (true, a));
        let (arg, dirty) = (arg.trim(), self.buf.dirty());
        let line = self.buf.line_of(self.buf.cursor);
        match name {
            "" if !rest.is_empty() => self.say(format!("bad range: {rest}")),
            "" => {
                if let Some((_, last)) = range {
                    let b = &mut self.buf;
                    (b.cursor, b.want, self.center) = (b.nonblank(last), None, true);
                }
            }
            "w" | "wq" | "x" if arg.starts_with('!') => self.say("filters are :{range}!cmd"),
            "w" => self.write(arg, bang, false),
            "wq" => self.write(arg, bang, true),
            "x" if dirty => self.write(arg, bang, true),
            "q" if dirty && !bang => self.say("no write since last change (:q! to override)"),
            "x" | "q" => self.fx.push(Effect::Quit(0)),
            "cq" => self.fx.push(Effect::Quit(1)),
            "e" if dirty && !bang => self.say("no write since last change (:e! to discard)"),
            "e" if self.saving.is_some() || self.resume.is_some() => {
                self.say("a save is already running")
            }
            "e" => match if arg.is_empty() { self.path.clone() } else { Some(arg.into()) } {
                // SAFE-7: mine is conflicting with the disk's, and the reload discards it: it is
                // copied first, and the reload follows the copy.
                Some(path) if dirty && self.theirs.is_some() => {
                    let (bytes, v) = (self.buf.encode(), self.buf.version());
                    self.resume = Some(Resume::Open(path, v));
                    let at = self.path.clone().unwrap_or_default();
                    self.fx.push(Effect::Keep { path: at, bytes, ext: "yours" });
                }
                Some(path) => {
                    self.fx.extend(self.place()); // leaving this file (CLI-6)
                    self.opening.push_back(self.buf.version());
                    self.fx.push(Effect::Open(path));
                }
                None => self.say("no file name"),
            },
            "s" => self.subst(range.unwrap_or((line, line)), arg),
            "seen" => self.buf.marks = Default::default(),
            "reg" | "registers" => self.say(self.regs.list(usize::from(self.size.0))),
            _ => self.say(format!("not an editor command: {name}")),
        }
        self.clamp();
    }

    /// `:{range}!cmd` (FILT-1): the lines go through `sh -c` without anyone waiting; `FilterDone`
    /// brings the answer. One filter runs at a time.
    fn filter(&mut self, (l1, l2): (usize, usize), cmd: &str) {
        if self.plain {
            return self.say("plain mode runs no program: a filter is refused");
        }
        let busy =
            self.filter.is_some().then_some("a filter is already running (Ctrl-c cancels it)");
        let usage = cmd.is_empty().then_some("usage: :{range}!cmd");
        if let Some(why) = self.buf.readonly.clone().or(busy.or(usage).map(Into::into)) {
            return self.say(why);
        }
        let b = &self.buf;
        let input = format!("{}\n", b.text.slice(b.start(l1)..b.end(l2))).into_bytes();
        self.filter =
            Some(Run { text: b.text.clone(), lines: l1..l2 + 1, at: self.tick, killed: false });
        self.fx.push(Effect::Spawn { cmd: cmd.into(), input });
    }

    /// VI-14: where the first match is of the pattern typed so far on a `/` or `?` line, from the
    /// cursor. A pattern that is empty, bad or matches nothing has none.
    pub(super) fn preview(&mut self) -> Option<usize> {
        let vi = &self.vi;
        let line = (vi.mode == Mode::Cmd).then(|| vi.line.clone());
        let line = line.filter(|l| l.len() > 1 && l.starts_with(['/', '?']))?;
        let fwd = line.starts_with('/');
        let last = self.search.replace((line[1..].into(), false, fwd));
        let found = self.find_match(fwd);
        self.search = last;
        found
    }

    /// `:s/re/rep/[g]` on the lines `l1..=l2`.
    fn subst(&mut self, (l1, l2): (usize, usize), arg: &str) {
        if let Some(why) = self.buf.readonly.clone() {
            return self.say(why);
        }
        let Some(body) = arg.strip_prefix('/') else { return self.say("usage: :s/re/rep/[g]") };
        let (mut parts, mut i, mut it) = (<[String; 3]>::default(), 0, body.chars());
        while let Some(c) = it.next() {
            match c {
                '/' if i < 2 => i += 1,
                '\\' => match it.next() {
                    Some('/') => parts[i].push('/'),
                    Some(d) => parts[i].extend(['\\', d]),
                    None => parts[i].push('\\'),
                },
                _ => parts[i].push(c),
            }
        }
        let [mut pat, rep, flags] = parts;
        let global = match flags.as_str() {
            "" => false,
            "g" => true,
            f => return self.say(format!("unknown flag: {f}")),
        };
        if pat.is_empty() {
            pat = self.search.as_ref().map(|s| s.0.clone()).unwrap_or_default();
        }
        let Some(re) = self.regex(&pat, false) else { return };
        let (mut subs, mut lines, mut at) = (0, 0, None);
        self.buf.checkpoint();
        // Last line first: the marks of the lines still to do stay where they are (PERF-2).
        for l in (l1..=l2).rev() {
            let b = &mut self.buf;
            let text = b.text.line(l).to_string();
            let (mut n, text) = (0, text.trim_end_matches('\n'));
            let out = re.replacen(text, usize::from(!global), |caps: &Captures| {
                n += 1;
                expand(&rep, caps)
            });
            if n > 0 {
                b.replace(b.start(l)..b.end(l), &out);
                (subs, lines, at) = (subs + n, lines + 1, at.max(Some(l)));
            }
        }
        match at {
            Some(l) => {
                (self.buf.cursor, self.buf.want) = (self.buf.nonblank(l), None);
                let (subs, lines) =
                    (plural(subs, "substitution", "substitutions"), plural(lines, "line", "lines"));
                self.say(format!("{subs} on {lines}"));
            }
            None => self.say(format!("not found: {pat}")),
        }
    }

    /// `n N * #`: where the search goes from the cursor. `*` and `#` search from the word's start.
    pub(super) fn search_key(&mut self, k: char) -> Option<usize> {
        let from = self.buf.cursor;
        if matches!(k, '*' | '#') {
            let (word, start) = self.word_here()?;
            self.search = Some((word, true, k == '*'));
            self.buf.cursor = start;
        }
        let found = self.find_match(self.search.as_ref().is_none_or(|s| s.2) == (k != 'N'));
        self.buf.cursor = from;
        found
    }

    /// The word at or after the cursor as a pattern, and where it starts: `\b` where the word starts or ends with a
    /// char that `\b` understands (regex-lite's word chars are ASCII).
    fn word_here(&mut self) -> Option<(String, usize)> {
        let b = &self.buf;
        let (l, w) = (b.line_of(b.cursor), |c: char| c.is_alphanumeric() || c == '_');
        let Some(i) = (b.cursor..b.end(l)).find(|&i| w(b.ch(i))) else {
            return self.fail("no word under the cursor");
        };
        let a = (b.start(l)..=i).rev().take_while(|&j| w(b.ch(j))).last().unwrap_or(i);
        let z = (i..b.end(l)).take_while(|&j| w(b.ch(j))).last().unwrap_or(i) + 1;
        let edge = |c: char| if c.is_ascii_alphanumeric() || c == '_' { "\\b" } else { "" };
        let word: String = b.text.slice(a..z).chars().collect();
        Some((format!("{}{}{}", edge(b.ch(a)), escape(&word), edge(b.ch(z - 1))), a))
    }

    /// The next match of the last search from the cursor, wrapping around the file. Step `i` of
    /// the search is one line: step 0 the rest of the cursor line, step n (after a full turn) its
    /// other part. A big buffer is searched in 8 chunks of steps on as many threads, which only
    /// read the text; the first chunk with a match wins, so the result is the sequential one.
    fn find_match(&mut self, fwd: bool) -> Option<usize> {
        let Some((pat, cs, _)) = self.search.clone() else {
            return self.fail("no previous pattern");
        };
        let re = self.regex(&pat, cs)?;
        let ci = folds(&pat, cs);
        let lit = if ci { literal(&pat).to_ascii_lowercase() } else { literal(&pat) };
        let (b, text) = (&self.buf, &self.buf.text);
        let (l0, c0, n) = (b.line_of(b.cursor), b.col(), b.lines());
        let step = |re: &Regex, i: usize| {
            let l = if fwd { (l0 + i) % n } else { (l0 + n - i % n) % n };
            let line = Cow::from(text.line(l));
            let line = line.trim_end_matches('\n');
            let has =
                |t: &str| if ci { t.to_ascii_lowercase().contains(&lit) } else { t.contains(&lit) };
            if !lit.is_empty() && !has(line) {
                return None;
            }
            let ok = |c: usize| match i {
                0 if fwd => c > c0,
                0 => c < c0,
                i if i == n && fwd => c <= c0,
                i if i == n => c >= c0,
                _ => true,
            };
            let mut starts =
                re.find_iter(line).map(|m| line[..m.start()].chars().count()).filter(|&c| ok(c));
            (if fwd { starts.next() } else { starts.last() }).map(|c| (i, text.line_to_char(l) + c))
        };
        let scan = |re: &Regex, mut r: Range<usize>| r.find_map(|i| step(re, i));
        let rest = HEAD.min(n + 1)..n + 1;
        let found = scan(&re, 0..rest.start).or_else(|| {
            if text.len_bytes() < self.par_min.unwrap_or(PAR_MIN) {
                return scan(&re, rest);
            }
            let size = rest.len().div_ceil(8).max(1);
            let at = |k: usize| (rest.start + k * size).min(rest.end);
            let (scan, re) = (&scan, &re);
            thread::scope(|s| {
                // Each thread gets its own copy of the regex: they would queue on one cache pool's lock.
                // A chunk whose thread could not start (or died) is searched here instead.
                let all: Vec<_> = (0..8)
                    .map(|k| {
                        let (x, y) = (at(k), at(k + 1));
                        let h =
                            thread::Builder::new().spawn_scoped(s, move || scan(&re.clone(), x..y));
                        (x..y, h.ok())
                    })
                    .collect();
                all.into_iter()
                    .find_map(|(r, h)| h.and_then(|h| h.join().ok()).unwrap_or_else(|| scan(re, r)))
            })
        });
        let Some((i, t)) = found else {
            return self.fail(format!("not found: {pat}"));
        };
        if if fwd { l0 + i >= n } else { i > l0 } {
            self.msg = Some("search wrapped".into());
        }
        self.center = true;
        Some(t)
    }
}

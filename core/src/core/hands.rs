//! VI-18, VI-20, VI-21, VI-22
//! What the hands bring from the owner's Neovim: the comment toggle (`gc`), the surroundings
//! (mini.surround's `gsa`, `gsd`, `gsr`), the pairs that typing makes (mini.pairs') and the lines that
//! Alt-h/j/k/l move (mini.move's).
use super::app::{App, Key};
use super::edit::PAIRS;
use std::ops::Range;
use std::sync::Arc;

/// The comment strings of Neovim's ftplugins (`'commentstring'`, Neovim 0.12), by the extension of the
/// file's name or by the whole name.
#[rustfmt::skip]
const COMMENTS: [(&str, &str); 8] = [
    ("// %s", "rs c h cpp hpp cc cxx go js jsx mjs cjs ts tsx mts cts java kt kts swift zig scala dart cs jsonc proto groovy scss less"),
    ("/* %s */", "css php"),
    ("<!-- %s -->", "html htm md markdown xml svg vue svelte"),
    ("# %s", "py sh bash zsh fish rb pl toml yaml yml conf nix r R jl ex exs nim ps1 bashrc zshrc profile Makefile makefile GNUmakefile mk Dockerfile Containerfile"),
    ("-- %s", "lua sql hs"),
    ("; %s", "lisp el clj ini"),
    ("% %s", "tex"),
    ("\"%s", "vim vimrc"),
];

/// What Lua's `%s` is: the blanks around a comment's parts.
fn ws(c: char) -> bool {
    c.is_ascii_whitespace() || c == '\x0b'
}

/// A commented line without its comment's parts `l` and `r` (Neovim's `make_uncomment_function`), or
/// `None` if it does not have them.
fn bare(s: &str, l: &str, r: &str) -> Option<String> {
    let body = s.trim_start_matches(ws);
    let core = body.strip_prefix(l)?;
    let mid = if r.is_empty() { core } else { core.trim_end_matches(ws).strip_suffix(r)? };
    if mid.trim_matches(ws).is_empty() {
        return Some(mid.into()); // (no indent and no blanks after it are left behind)
    }
    Some([&s[..s.len() - body.len()], mid, &core[mid.len() + r.len()..]].concat())
}

impl App {
    /// VI-18: `gc` comments the lines `l1..=l2` at the smallest indent of those that are not blank, or
    /// uncomments them when every one of those is commented (Neovim's `vim._comment`), and leaves the
    /// cursor at `at`'s place. The comment string is the file name's.
    pub(super) fn comment(&mut self, l1: usize, l2: usize, at: usize) {
        let name = self.path.as_deref().map(|p| p.rsplit('/').next().unwrap_or(p));
        let ext = name.map(|n| n.rsplit_once('.').map_or(n, |x| x.1));
        let known =
            |e: &str| COMMENTS.iter().find(|c| c.1.split(' ').any(|x| x == e || name == Some(x)));
        let Some((l, r)) = ext.and_then(known).and_then(|c| c.0.split_once("%s")) else {
            return self.say(format!("no comment string for {}", name.unwrap_or("[no name]")));
        };
        let b = &self.buf;
        let (lt, rt, line) = (l.trim(), r.trim(), b.line_of(at));
        let place = (line, (at - b.start(line)).min(b.line_len(line).saturating_sub(1)));
        let lines: Vec<String> =
            (l1..=l2).map(|i| b.text.slice(b.start(i)..b.end(i)).to_string()).collect();
        let full = lines.iter().filter(|s| !s.trim_matches(ws).is_empty());
        let indent = |s: &&String| s.len() - s.trim_start_matches(ws).len();
        let ind = full.clone().min_by_key(indent).map_or("", |s| &s[..indent(&s)]);
        // (a line is commented when, without its blanks, it starts and ends with the trimmed parts)
        let undo = full
            .clone()
            .all(|s| s.trim_matches(ws).strip_prefix(lt).is_some_and(|u| u.ends_with(rt)));
        for (i, s) in lines.iter().enumerate() {
            let new = match s.trim_matches(ws).is_empty() {
                _ if undo => bare(s, l, r).or_else(|| bare(s, lt, rt)).unwrap_or_else(|| s.clone()),
                true => format!("{ind}{lt}{rt}"),
                false => format!("{ind}{l}{}{r}", &s[ind.len()..]),
            };
            if new != *s {
                let (b, line) = (&mut self.buf, l1 + i);
                b.replace(b.start(line)..b.end(line), &new);
            }
        }
        let b = &mut self.buf;
        b.cursor = b.start(place.0) + place.1.min(b.line_len(place.0));
    }
}

/// VI-20: what mini.surround puts for a char: an opening bracket with a blank inside (`( ` and ` )`), a
/// closing one and `b` without, `q` double quotes, any other char itself; `t`, `f` and `?` ask mini for
/// a name and are not built.
fn parts(c: char) -> Option<(String, String)> {
    let c = match c {
        't' | 'f' | '?' => return None,
        'b' => ')',
        'q' => '"',
        c => c,
    };
    Some(match PAIRS.iter().find(|p| p.0 == c || p.1 == c) {
        Some(&(o, e)) if o == c => (format!("{o} "), format!(" {e}")),
        Some(&(o, e)) => (o.into(), e.into()),
        None => (c.into(), c.into()),
    })
}

impl App {
    /// VI-20: `gsa` puts the parts of `c` around `a..e`; around whole lines from the first char that is not
    /// blank to the last (mini's, as charwise).
    pub(super) fn surround(&mut self, a: usize, e: usize, lw: bool, c: Option<char>) {
        let Some((l, r)) = c.and_then(parts) else { return self.say(ASKS) };
        let b = &mut self.buf;
        let (l2, a) = (b.line_of(e), if lw { b.nonblank(b.line_of(a)) } else { a });
        let e = if lw {
            b.start(l2) + b.text.line(l2).to_string().trim_end_matches(ws).chars().count()
        } else {
            e
        };
        b.replace(e..e, &r);
        b.replace(a..a, &l);
        b.cursor = a + l.chars().count();
    }

    /// VI-20: `gsd` takes away the surrounding `old` the cursor is in, and `gsr` puts the parts of `new` in
    /// its place; the cursor goes after the left one.
    pub(super) fn resurround(&mut self, old: Option<char>, new: Option<char>) {
        let old = old.filter(|o| parts(*o).is_some());
        let (Some(o), Some((l, r))) = (old, new.map_or(Some(Default::default()), parts)) else {
            return self.say(ASKS);
        };
        let Some((left, right)) = self.around(o) else {
            return self.say(format!("no surrounding {o} found within 20 lines"));
        };
        let b = &mut self.buf;
        b.replace(right, &r);
        b.replace(left.clone(), &l);
        b.cursor = left.start + l.chars().count();
    }

    /// The surrounding `o` the cursor is in, as mini.surround finds it ('cover'): the narrowest pair that
    /// holds the cursor, on its line, or else within 20 lines of it. Brackets pair as `%b()` does, and the
    /// same char pairs with the next one (`"`, `'`, `` ` ``, or any other); `b` is any bracket and `q` any
    /// quote. An opening bracket takes the blanks inside it too.
    fn around(&self, o: char) -> Option<(Range<usize>, Range<usize>)> {
        const QUOTES: [(char, char); 3] = [('\'', '\''), ('"', '"'), ('`', '`')];
        let kinds: Vec<(char, char)> = match o {
            'b' => PAIRS[..3].to_vec(),
            'q' => QUOTES.to_vec(),
            _ => vec![PAIRS.iter().find(|p| p.0 == o || p.1 == o).copied().unwrap_or((o, o))],
        };
        let (b, l) = (&self.buf, self.buf.line_of(self.buf.cursor));
        for n in [0, 20] {
            let (from, to) = (b.start(l.saturating_sub(n)), b.end((l + n).min(b.lines() - 1)));
            let hay: Vec<char> = b.text.slice(from..to).chars().collect();
            let (at, mut best) = (b.cursor - from, None::<(usize, usize)>);
            for &(op, cl) in &kinds {
                // (a quote closes the one before it and opens the next, as `x.-x` matches)
                let mut open = vec![];
                for (i, &c) in hay.iter().enumerate() {
                    if c == cl
                        && let Some(j) = open.pop()
                        && j <= at
                        && at <= i
                        && best.is_none_or(|(x, y)| i - j < y - x)
                    {
                        best = Some((j, i));
                    }
                    if c == op {
                        open.push(i);
                    }
                }
            }
            let Some((j, i)) = best else { continue };
            let inner = |k: usize| "([{<".contains(o) && k > j && k < i && ws(hay[k]);
            let w1 = (j + 1..i).take_while(|&k| inner(k)).count();
            let w2 = (j + 1 + w1..i).rev().take_while(|&k| inner(k)).count();
            return Some((from + j..from + j + 1 + w1, from + i - w2..from + i + 1));
        }
        None
    }
}

impl App {
    /// VI-21: a char typed in Insert mode, as mini.pairs' defaults type it: `(` `[` `{` put their closing
    /// char after the cursor too, a closing one steps over the same char after the cursor, and `"` `'`
    /// `` ` `` step over or put two; nothing is put in pairs after a backslash, nor `'` after a letter.
    /// What was put in is what `.` types (as vim's redo has mini's keys), so `.` does not pair again.
    pub(super) fn typed(&mut self, c: char, prev: char, next: char) {
        let quote = "\"'`".contains(c);
        let close = match c {
            '(' => ')',
            '[' => ']',
            '{' => '}',
            c if quote => c,
            _ => '\0',
        };
        let over = next == c && (quote || (")]}".contains(c) && prev != '\\'));
        let two = close != '\0' && prev != '\\' && !(c == '\'' && prev.is_ascii_alphabetic());
        let keys = match (self.replaying, over, two) {
            (true, ..) | (_, false, false) => vec![Key::Char(c)],
            (_, true, _) => vec![Key::Right],
            _ => vec![Key::Char(c), Key::Char(close), Key::Left],
        };
        if self.rec.last() == Some(&Key::Char(c)) {
            self.rec.pop();
            self.rec.extend(&keys);
        }
        for k in keys {
            match k {
                Key::Char(x) => self.type_str(x.encode_utf8(&mut [0; 4])),
                Key::Right => self.buf.cursor += 1,
                _ => self.buf.cursor -= 1,
            }
        }
    }
}

impl App {
    /// VI-22: Alt-j and Alt-k move the line, or the lines of the selection, `n` lines down or up (as far as
    /// there are), and Alt-l and Alt-h shift them `n` units in or out. The marks go with their lines,
    /// and the cursor (and the selection's other end) with its char.
    pub(super) fn move_lines(&mut self, dir: Option<char>, n: usize) {
        let b = &self.buf;
        // (the last char a selection takes; V-LINE's ends at the line break of its last line, an empty one too)
        let line = self.vi.mode == super::vi::Mode::VLine;
        let sel =
            self.selection().map(|(s, e)| (s, if line { e } else { e.saturating_sub(1).max(s) }));
        let (s, e) = sel.unwrap_or((b.cursor, b.cursor));
        let (l1, l2, down) = (b.line_of(s), b.line_of(e), dir == Some('j'));
        let anchor = if sel.is_some() { self.anchor } else { b.cursor }; // (out of Visual mode, stale)
        let ends = [anchor, b.cursor].map(|i| (b.line_of(i), i - b.start(b.line_of(i))));
        let (lens, size) = (ends.map(|(l, _)| b.line_len(l)), l2 - l1 + 1);
        let d = if down {
            n.min(b.lines() - 1 - l2)
        } else {
            n.min(l1) * usize::from(dir == Some('k'))
        };
        // (where a line goes: the lines from `lo` to `hi` turn round, the moved ones by `d`)
        let ((lo, hi), turn) = if down { ((l1, l2 + d), d) } else { ((l1 - d, l2), size) };
        let go = |l: usize| if l < lo || l > hi { l } else { lo + (l - lo + turn) % (hi - lo + 1) };
        match dir {
            Some('h' | 'l') => self.shift(l1, l2, dir == Some('l'), n),
            _ if d == 0 => return,
            _ => {
                let text = |a: usize, z: usize| b.text.slice(b.start(a)..b.end(z)).to_string();
                let (block, by) =
                    (text(l1, l2), if down { text(l2 + 1, hi) } else { text(lo, l1 - 1) });
                let new = if down { format!("{by}\n{block}") } else { format!("{block}\n{by}") };
                let (vm, mut dm) = (b.vmark, b.marks.to_vec());
                // (the marks stay in the order of their lines: the block's and the passed ones swap)
                let (i, j) =
                    (dm.partition_point(|m| m.line < lo), dm.partition_point(|m| m.line <= hi));
                let k = dm[i..j].partition_point(|m| m.line < if down { l2 + 1 } else { l1 });
                dm[i..j].rotate_left(k);
                dm.iter_mut().for_each(|m| m.line = go(m.line));
                let b = &mut self.buf;
                b.replace(b.start(lo)..b.end(hi), &new);
                (b.vmark, b.marks) = (vm.map(|m| m.map(|(l, c)| (go(l), c))), Arc::new(dm));
            }
        }
        // (a shift moves the columns by what each line gained or lost)
        let shifted = matches!(dir, Some('h' | 'l'));
        let [a, c] = [0, 1].map(|i| {
            let ((l, c), b) = (ends[i], &self.buf);
            b.pos((go(l), if shifted { (c + b.line_len(l)).saturating_sub(lens[i]) } else { c }))
        });
        (self.anchor, self.buf.cursor) = (a, c);
    }
}

/// What `gsa`, `gsd` and `gsr` say for a surrounding that asks for a name.
const ASKS: &str = "a surrounding that asks for a name (t f ?) is not built";

//! KEY-1, KEY-3, KEY-4, DIFF-6
//! The key table (`keys.tsv`, pure): every key sim has, by mode, with a one-line description and a
//! longer text, read once; the next keys after a prefix, which which-key lists; the rows of the help;
//! and the small window which-key is.
use crate::color::Tones;
use crate::layout::{Line, border, fit};
use crate::paint::{Look, Paint, Role};
use crate::pick::{Rect, Win};
use std::sync::OnceLock;

/// An entry of the table: the tokens of its keys (each with its alternatives), the modes it is for
/// (`n` Normal, `v` Visual, `o` after an operator, `i` Insert, `c` the command line, `t` the tree, `p`
/// the picker), a description and a longer text.
pub struct Entry {
    pub seq: Vec<Vec<String>>,
    pub modes: String,
    pub desc: String,
    pub long: String,
}

/// How a token of the table is written for people: its escapes are the key itself (`<lt>` is `<`, `<Bar>`
/// is `|`, `<gt>` is `>`); the names of keys without a char (`<Space>`, `<Tab>`, `<C-n>`) stay as the
/// author's Neovim writes them.
pub fn shown(token: &str) -> &str {
    match token {
        "<lt>" => "<",
        "<Bar>" => "|",
        "<gt>" => ">",
        t => t,
    }
}

/// The operators: what they are followed by is a motion or a text object.
const OPERATORS: [&str; 6] = ["d", "c", "y", "<lt>", ">", "!"];

pub fn table() -> &'static [Entry] {
    static TABLE: OnceLock<Vec<Entry>> = OnceLock::new();
    TABLE.get_or_init(|| include_str!("../keys.tsv").lines().filter_map(parse).collect())
}

fn parse(line: &str) -> Option<Entry> {
    let mut f = line.split('\t');
    let (keys, modes, desc, long) = (f.next()?, f.next()?, f.next()?, f.next()?);
    let seq = keys.split(' ').map(|t| t.split('|').map(String::from).collect()).collect();
    Some(Entry { seq, modes: modes.into(), desc: desc.into(), long: long.into() })
}

impl Entry {
    /// Every sequence of keys the entry stands for: a token's alternatives, in turn.
    pub fn sequences(&self) -> Vec<Vec<&str>> {
        let mut all: Vec<Vec<&str>> = vec![vec![]];
        for token in &self.seq {
            all = all
                .iter()
                .flat_map(|p| token.iter().map(|t| [&p[..], &[t.as_str()]].concat()))
                .collect();
        }
        all
    }

    /// How the keys are written: each sequence run together and the escapes of the table gone (`<Space>bp`,
    /// `i<`), ` / ` between the alternatives (`h / <Left>`, `i< / i>`).
    pub fn keys(&self) -> String {
        let seqs: Vec<String> =
            self.sequences().iter().map(|q| q.iter().map(|t| shown(t)).collect()).collect();
        match seqs.len() {
            0..=4 => seqs.join(" / "),
            n => format!("{} … {}", seqs[0], seqs[n - 1]), // (`1 … 9`)
        }
    }

    /// The modes in words.
    pub fn modes(&self) -> String {
        let name = |c| match c {
            'n' => "normal",
            'v' => "visual",
            'o' => "operator",
            'i' => "insert",
            'c' => "command",
            't' => "tree",
            _ => "picker",
        };
        self.modes.chars().map(name).collect::<Vec<_>>().join("/")
    }
}

/// KEY-3: the keys that can come next after `prefix` (tokens: `d`, `<Space>`), as (key, description), in
/// the order of the table, each key once, the alternatives of a token as one (`h / <Left>`), written for people
/// (`shown`). After an operator
/// they are the entries for `o` that follow what comes after it, and the operator's own key. `mode` is the
/// one the prefix was typed in: `n`, `v` or `t`.
pub fn next(prefix: &[String], mode: char) -> Vec<(String, String)> {
    let op = prefix.first().is_some_and(|p| OPERATORS.contains(&p.as_str()));
    let rest = if op { &prefix[1..] } else { prefix };
    let mut rows: Vec<(String, String, bool)> = vec![];
    // the keys of the prefix itself first (`dd`), then what an operator takes (motions and objects)
    for pass in [0, 1] {
        let (applies, head) = if pass == 0 { ('\0', prefix) } else { ('o', rest) };
        for e in table().iter().filter(|e| {
            (pass == 0 && e.modes.contains(mode)) || (pass == 1 && op && e.modes.contains(applies))
        }) {
            let follows =
                e.seq.len() > head.len() && head.iter().zip(&e.seq).all(|(h, t)| t.contains(h));
            if !follows {
                continue;
            }
            let alts = e.seq[head.len()].iter().map(|t| shown(t));
            let (key, exact) =
                (alts.collect::<Vec<_>>().join(" / "), e.seq.len() == head.len() + 1);
            match rows.iter_mut().find(|r| r.0 == key) {
                Some(r) if exact && !r.2 => *r = (key, e.desc.clone(), true),
                Some(_) => {}
                None => rows.push((key, e.desc.clone(), exact)),
            }
        }
    }
    rows.into_iter().map(|r| (r.0, r.1)).collect()
}

/// KEY-4: the help's rows: the entry's index in the table, and the row as it is written and searched.
pub fn rows() -> &'static [(usize, String)] {
    static ROWS: OnceLock<Vec<(usize, String)>> = OnceLock::new();
    ROWS.get_or_init(|| {
        let key = table().iter().map(|e| e.keys().chars().count()).max().unwrap_or(0).min(KEYS);
        let line = |e: &Entry| format!("{:key$}  {}  {}", cell(&e.keys(), key), e.desc, e.modes());
        table().iter().enumerate().map(|(i, e)| (i, line(e))).collect()
    })
}

/// The keys of a row in `max` columns: where they are longer, the alternatives that fit and ` …`
/// (the preview has them all), never half of a key.
fn cell(keys: &str, max: usize) -> String {
    if keys.chars().count() <= max {
        return keys.into();
    }
    let ends =
        keys.match_indices(" / ").map(|(i, _)| i).filter(|&i| keys[..i].chars().count() + 2 <= max);
    ends.last().map_or_else(|| fit(keys, max).0, |i| format!("{} …", &keys[..i]))
}

/// The widest keys column of the help (a row shows `keys  description  mode`).
const KEYS: usize = 16;

/// The most rows of a which-key window (it is a small window: a list that is longer ends in `… n more`).
const MOST: usize = 16;

/// KEY-3: the window of which-key for the screen's size `(w, h)`: where it is (near the bottom, in the middle,
/// above the status line) and its rows, `title` in the top border. None where the screen has no room
/// (fewer than 4 rows for it). `t` is none in calm: no color.
pub fn which(
    title: &str,
    rows: &[(String, String)],
    (sw, sh): (u16, u16),
    t: Option<&Tones>,
) -> Option<(Win, Vec<Line>)> {
    let room = usize::from(sh).saturating_sub(7).min(MOST);
    if rows.is_empty() || room < 2 || sw < 12 {
        return None;
    }
    let more = rows.len() > room;
    let shown = if more { room - 1 } else { rows.len() };
    let key_w = rows.iter().take(shown).map(|r| fit(&r.0, usize::MAX).1).max().unwrap_or(1);
    let desc_w = rows.iter().take(shown).map(|r| fit(&r.1, usize::MAX).1).max().unwrap_or(1);
    let more_text = format!("… {} more (:help)", rows.len() - shown);
    let body =
        (key_w + 2 + desc_w).max(fit(&more_text, usize::MAX).1).max(fit(title, usize::MAX).1 + 2);
    let w = (body + 4).min(usize::from(sw) - 2);
    let inner = w - 4;
    let h = shown + usize::from(more) + 2;
    let (x, y) = ((usize::from(sw) - w) / 2, usize::from(sh) - 1 - h);
    let ink = |c| Paint { fg: c, ..Paint::default() };
    let (frame, accent) = (ink(t.map(|t| t.focus)), ink(t.map(Tones::accent_fg)));
    let mut out = vec![border(
        w as u16,
        ['╭', '╮'],
        frame,
        &[(format!(" {title} "), Paint { bold: t.is_some(), ..Paint::default() })],
        &[],
    )];
    let boxed = |inside: Line| {
        let mut l = Line::default();
        l.put("│ ", frame);
        l.add(inside);
        l.put(" │", frame);
        l
    };
    for (k, d) in rows.iter().take(shown) {
        let mut l = Line::default();
        let (key, _) = fit(k, key_w);
        l.put(&key, accent);
        l.looks.push((0..l.cols, Look::Bold));
        l.pad(key_w as u16 + 2);
        l.put(&fit(d, inner.saturating_sub(key_w + 2)).0, Paint::default());
        l.pad(inner as u16);
        out.push(boxed(l));
    }
    if more {
        let mut l = Line::default();
        l.put(&fit(&more_text, inner).0, Paint::default());
        l.row.spans.push((0..l.cols, sim_core::core::view::Style::Secondary));
        l.pad(inner as u16);
        out.push(boxed(l));
    }
    out.push(border(w as u16, ['╰', '╯'], frame, &[], &[]));
    if let Some(t) = t {
        for l in &mut out {
            let fill = Paint { bg: Some(t.cursor), role: Role::Fill, ..Paint::default() };
            l.paints.push((0..w as u16, fill));
        }
    }
    let win = Win {
        x: x as u16,
        y: y as u16,
        w: w as u16,
        h: h as u16,
        stacked: false,
        prompt: Rect::default(),
        list: Rect::default(),
        pane: Rect::default(),
    };
    Some((win, out))
}

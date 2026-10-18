//! VI-15
//! Registers: where yanks and deletes go and where a put reads from; `:registers` says what they hold.
use super::app::{App, Effect};
use ropey::Rope;

/// What a register holds: its text, and whether it is whole lines (each one ends with `\n`). A rope
/// shares its text with the buffer's and with the undo steps, so nine big deletes cost no more than the
/// nodes they changed (measured, CONCEPT).
pub type Reg = (Rope, bool);

/// The registers (VI-15). Memory only: nothing here is ever written.
#[derive(Default)]
pub struct Regs {
    map: Vec<(char, Reg)>,
    /// The register the unnamed one points to.
    last: Option<char>,
}

impl Regs {
    /// Whether `c` names a register: `a`-`z`, `A`-`Z` (appends), `0`-`9`, `"` the unnamed one, `-` the small
    /// deletes, `_` the black hole, and `+` `*` (one register: what was sent to the clipboard; written,
    /// never read).
    pub fn valid(c: char) -> bool {
        c.is_ascii_alphanumeric() || "\"-_+*".contains(c)
    }

    /// The register `name` names (`None`: the unnamed one) if it holds something.
    pub fn get(&self, name: Option<char>) -> Option<&Reg> {
        let k = match name {
            None | Some('"') => self.last?,
            Some(c) => c.to_ascii_lowercase(),
        };
        self.map.iter().find(|e| e.0 == k).map(|e| &e.1).filter(|r| r.0.len_chars() > 0)
    }

    /// The register `k`, made if there is none; the list stays in the order of the names.
    fn slot(&mut self, k: char) -> &mut Reg {
        let i = self.map.partition_point(|e| e.0 < k);
        if self.map.get(i).is_none_or(|e| e.0 != k) {
            self.map.insert(i, (k, Reg::default()));
        }
        &mut self.map[i].1
    }

    /// Writes register `c` (an uppercase letter appends) and returns the register it is.
    fn put(&mut self, c: char, text: Rope, lw: bool) -> char {
        let k = c.to_ascii_lowercase();
        let r = self.slot(k);
        if c.is_ascii_uppercase() && r.0.len_chars() > 0 {
            // Lines added to chars, or chars to lines, make the register lines.
            let ends = text.slice(text.len_chars().saturating_sub(1)..) == "\n";
            if lw || r.1 {
                let at = r.0.len_chars();
                r.0.insert(at, if r.1 { "" } else { "\n" });
                r.0.append(text);
                r.0.append(Rope::from_str(if ends { "" } else { "\n" }));
            } else {
                r.0.append(text);
            }
            r.1 |= lw;
        } else {
            *r = (text, lw);
        }
        k
    }

    /// A yank: into `"0` unless it has a name (a yank with a name leaves `"0` alone, as in vim). `+` and
    /// `*` are one register, the text sim last sent to the clipboard: shown, and put by `p`, never read
    /// from the clipboard.
    pub fn yank(&mut self, name: Option<char>, text: Rope, lw: bool) {
        let k = match name {
            Some('_') => return,
            None | Some('"') => '0',
            Some('*') => '+',
            Some(c) => c,
        };
        self.last = Some(self.put(k, text, lw));
    }

    /// A delete or change. `big`: it took a line or more, or moved by a motion that vi always counts
    /// as one (`%` and the like): it goes to `"1`, the others to `"-`.
    pub fn delete(&mut self, name: Option<char>, text: Rope, lw: bool, big: bool) {
        let name = name.filter(|c| *c != '"').map(|c| if c == '*' { '+' } else { c });
        if name == Some('_') {
            return;
        }
        if let Some(c) = name {
            self.last = Some(self.put(c, text.clone(), lw));
        }
        if big {
            self.map.retain(|e| e.0 != '9');
            self.map
                .iter_mut()
                .filter(|e| ('1'..'9').contains(&e.0))
                .for_each(|e| e.0 = char::from(e.0 as u8 + 1));
            *self.slot('1') = (text.clone(), lw);
            if !name.is_some_and(|c| c.is_ascii_uppercase()) {
                self.last = Some('1');
            }
        }
        if name.is_none() && !lw && text.len_lines() < 2 {
            self.last = Some(self.put('-', text, false));
        }
    }

    /// `:registers`: the unnamed register and every one that holds something, on one line of `width`
    /// columns (`one_line`).
    pub fn list(&self, width: usize) -> String {
        let held = self.map.iter().filter(|e| e.1.0.len_chars() > 0);
        let all =
            self.get(None).map(|r| ('"', &r.0)).into_iter().chain(held.map(|e| (e.0, &e.1.0)));
        let rows: Vec<(String, String)> = all
            .map(|(k, t)| (format!("\"{k} "), t.slice(..t.len_chars().min(width)).to_string()))
            .collect();
        one_line(&rows, width, "no registers")
    }
}

/// Rows of a head and a text, on one line of `width` columns, ` · ` between them: a line break in a text
/// shows as `⏎`, and each text is whole or, if it does not fit, cut with `…` to its share of what the
/// heads and the dots leave (`shares`). `empty` is what there is to say of no rows.
pub(super) fn one_line(rows: &[(String, String)], width: usize, empty: &str) -> String {
    let texts: Vec<Vec<char>> =
        rows.iter().map(|r| r.1.replace('\n', "⏎").chars().collect()).collect();
    let (heads, dots) =
        (rows.iter().map(|r| r.0.chars().count()).sum::<usize>(), rows.len().saturating_sub(1));
    let needs: Vec<usize> = texts.iter().map(Vec::len).collect();
    let cuts = shares(&needs, width.saturating_sub(heads + 3 * dots));
    let shown: Vec<String> = (rows.iter().zip(&texts).zip(cuts))
        .map(|((r, t), share)| {
            let text: String = if t.len() <= share {
                t.iter().collect()
            } else {
                t[..share - 1].iter().chain(&['…']).collect()
            };
            r.0.clone() + &text
        })
        .collect();
    if shown.is_empty() { empty.into() } else { shown.join(" · ") }
}

/// How many columns each text (`needs` are their lengths) may have so that all of them fit `room`
/// columns: one that needs less than an equal share takes what it needs, and the others divide what is
/// left (at least 4 each, so a line with too many is cut by the screen, not made unreadable).
fn shares(needs: &[usize], room: usize) -> Vec<usize> {
    let (mut out, mut open, mut left) =
        (vec![0; needs.len()], (0..needs.len()).collect::<Vec<_>>(), room);
    while let Some(i) = open.iter().position(|&j| needs[j] <= left / open.len()) {
        let j = open.swap_remove(i);
        (out[j], left) = (needs[j], left - needs[j]);
    }
    open.iter().for_each(|&j| out[j] = (left / open.len()).max(4));
    out
}

impl App {
    /// sim shares the registers of all its open files: they are taken from the file that typed last
    /// and given to the next one (VI-15).
    pub fn take_regs(&mut self) -> Regs {
        std::mem::take(&mut self.regs)
    }

    pub fn put_regs(&mut self, regs: Regs) {
        self.regs = regs;
    }

    /// A yank into register `name` (VI-15). A plain one and one named `+` or `*` are copied out as well
    /// (VI-8); one with another name stays inside.
    pub(super) fn yank(&mut self, name: Option<char>, text: Rope, lw: bool) {
        if matches!(name, None | Some('"' | '+' | '*')) {
            self.fx.push(Effect::Clipboard(text.to_string()));
        }
        self.took = text.len_chars();
        self.regs.yank(name, text, lw);
    }
}

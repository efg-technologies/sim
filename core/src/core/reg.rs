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
    /// deletes, `_` the black hole, and `+` `*` the clipboard (written, never read).
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

    /// A yank: into `"0` unless it has a name. The clipboard's names are no register of their own.
    pub fn yank(&mut self, name: Option<char>, text: Rope, lw: bool) {
        let k = match name {
            Some('_') => return,
            None | Some('"' | '+' | '*') => '0',
            Some(c) => c,
        };
        self.last = Some(self.put(k, text, lw));
    }

    /// A delete or change. `big`: it took a line or more, or moved by a motion that vi always counts
    /// as one (`%` and the like): it goes to `"1`, the others to `"-`.
    pub fn delete(&mut self, name: Option<char>, text: Rope, lw: bool, big: bool) {
        let name = name.filter(|c| !"\"+*".contains(*c));
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
    /// columns: each is cut to its share of what the names and the dots leave (at most 16 chars, at
    /// least 4), so that the last one is on the screen when there are not too many.
    pub fn list(&self, width: usize) -> String {
        let held = self.map.iter().filter(|e| e.1.0.len_chars() > 0);
        let n = held.clone().count() + 1;
        let cut = (width.saturating_sub(7 * n) / n).clamp(4, 16);
        let show = |k: char, t: &Rope| {
            let shown = t.slice(..t.len_chars().min(cut)).to_string().replace('\n', "⏎");
            format!("\"{k} {shown}{}", if t.len_chars() > cut { "…" } else { "" })
        };
        let all: Vec<String> = self
            .get(None)
            .map(|r| show('"', &r.0))
            .into_iter()
            .chain(held.map(|e| show(e.0, &e.1.0)))
            .collect();
        if all.is_empty() { "no registers".into() } else { all.join(" · ") }
    }
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

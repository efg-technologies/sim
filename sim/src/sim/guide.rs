//! KEY-2, KEY-3, KEY-5, KEY-6, MD-5, MD-6
//! What guides the user's hands in sim's state (pure): the leader (Space and what follows it), the
//! prefix that is typed and not complete, the pause after which which-key shows what can come next,
//! the letters on the tabs, and the hint for Tab.
use super::panel::Out;
use super::{Sim, SimEffect};
use crate::keys;
use crate::layout;
use crate::tabs;
use sim_core::core::app::{Event, Key};
use sim_core::core::vi::Mode;

/// The hint of KEY-6, and how many uses of Tab end it.
const HINT: &str = "Tab / S-Tab: next / previous file";
const LEARNED: u8 = 3;

/// A key as the key table writes it.
fn token(c: char) -> String {
    match c {
        '<' => "<lt>".into(),
        ' ' => "<Space>".into(),
        '|' => "<Bar>".into(),
        c => c.to_string(),
    }
}

impl Sim {
    /// KEY-3: the keys of a sequence that is typed and not complete, as the key table writes them:
    /// the leader's, an operator and what follows it, `g`, `z`, `Z`, `[`, `]`. Not a count, and not the
    /// char that `f`, `t` and `r` wait for.
    pub fn prefix(&self) -> Vec<String> {
        if let Some(l) = &self.leader {
            return ["<Space>".to_string()].into_iter().chain(l.chars().map(token)).collect();
        }
        if self.panel.open && self.panel.focus {
            return if self.panel.g { vec!["g".into()] } else { vec![] };
        }
        let Some(b) = self.bufs.get(self.cur) else { return vec![] };
        let vi = &b.app.vi;
        let typed = matches!(vi.mode, Mode::Normal | Mode::Visual | Mode::VLine) && !vi.text();
        if typed { vi.pending().chars().map(token).collect() } else { vec![] }
    }

    /// The mode a prefix is typed in, for the key table: the tree's, Visual, or Normal.
    fn key_mode(&self) -> char {
        match self.bufs.get(self.cur).map(|b| b.app.vi.mode) {
            _ if self.panel.open && self.panel.focus => 't',
            Some(Mode::Visual | Mode::VLine) => 'v',
            _ => 'n',
        }
    }

    /// KEY-3: the window which-key shows: the prefix as its title and the keys that can come next.
    pub(super) fn which_rows(&self) -> Option<(String, Vec<(String, String)>)> {
        let p = self.which.as_ref().filter(|_| self.panel.live.is_none())?;
        Some((p.iter().map(|t| keys::shown(t)).collect(), keys::next(p, self.key_mode())))
    }

    /// KEY-2: whether a Space is the leader here: in Normal mode with nothing typed, and in the tree
    /// unless it is taking a query or a `g`.
    pub(super) fn leads(&self) -> bool {
        if self.panel.live.is_some() {
            return false;
        }
        if self.panel.open && self.panel.focus {
            return !self.panel.g && !self.panel.search.as_ref().is_some_and(|s| s.typing);
        }
        self.idle()
    }

    /// KEY-2: a key while the leader's sequence is half typed, or the Space that starts one. A key
    /// that no sequence has, and Esc, end it and do nothing.
    pub(super) fn leader_key(&mut self, k: Key, out: Out) {
        let c = if let Key::Char(c) = k { c } else { '\0' };
        match (self.leader.take().as_deref(), c) {
            (None, ' ') => self.leader = Some(String::new()),
            (Some(""), 'b') => self.leader = Some("b".into()),
            (Some(""), 'g') => self.leader = Some("g".into()),
            (Some(""), 'u') => self.leader = Some("u".into()),
            (Some("u"), 'm') => self.read_key(out),
            (Some("g"), 'e') => self.changes_key(out),
            (Some("g"), 'i') => self.guard_key(),
            (Some("b"), 'p' | 'c') => self.tab_pick(c == 'c', out),
            (Some("b"), 'k') => self.kept_key(),
            _ => {}
        }
    }

    /// GIT-5: `<Space>gi` says again what the guard did: why git was not run (the message of its first
    /// refusal, with the key and the file), or which settings of the repository's it turned off, by name.
    /// The names come from the repository's config and are cut and made safe where they are drawn.
    fn guard_key(&mut self) {
        let tail = |k: &String| layout::tail(k, 30);
        self.msg = Some(match (&self.why, self.off.is_empty()) {
            _ if self.plain => "plain mode: no git".into(),
            (Some(why), _) => why.clone(),
            _ if self.no_repo => "git: no repository here".into(),
            (None, false) => {
                let names: Vec<String> = self.off.iter().map(tail).collect();
                format!("git: turned off the repository's {}", names.join(", "))
            }
            (None, true) => "git: nothing of this repository's config is turned off".into(),
        });
    }

    /// SAFE-7: `<Space>bk` says again where the version that `:w!` or `:e!` gave up went, alone on the line;
    /// a path wider than the line is cut from the left, so that the file's name stays.
    fn kept_key(&mut self) {
        let room = usize::from(self.size.0).saturating_sub(8); // (BUF-2's position, `  12/34`, follows a message)
        let said = self.kept.as_deref().map(|p| layout::tail(p, room));
        self.msg = Some(said.unwrap_or_else(|| "no other version was kept".into()));
        self.saw_kept(); // TUTOR-2
    }

    /// MD-5, MD-6: `<Space>um` turns the read view of Markdown on and off for every file, and the
    /// choice is remembered; plain mode has none.
    fn read_key(&mut self, out: Out) {
        if self.plain {
            return self.msg = Some("plain mode: no read view".into());
        }
        self.md = !self.md;
        self.msg = Some(format!("markdown: {}", if self.md { "read view" } else { "raw" }));
        out.push(SimEffect::Md(self.md));
    }

    /// KEY-5: the letters of the tabs, bufferline's: from the first letter of each file's name.
    pub(super) fn tab_letters(&self) -> Vec<char> {
        let names: Vec<String> =
            self.bufs.iter().map(|b| tab_name(b.app.path.as_deref())).collect();
        tabs::letters(&names)
    }

    /// KEY-5: `<Space>bp` and `<Space>bc`: the letters are put on the tabs, or on the status line
    /// where there are no tabs, until the next key.
    fn tab_pick(&mut self, close: bool, _out: Out) {
        if self.bufs.is_empty() {
            return;
        }
        self.tabpick = Some(close);
        if !self.ui() {
            let names = self.bufs.iter().map(|b| tab_name(b.app.path.as_deref()));
            let all: Vec<String> = self
                .tab_letters()
                .into_iter()
                .zip(names)
                .map(|(l, n)| format!("{l} {n}"))
                .collect();
            self.msg =
                Some(format!("pick{}: {}", if close { " to close" } else { "" }, all.join(" · ")));
        }
    }

    /// KEY-5: the key after the letters were shown: a tab's letter shows its file (or closes it, as `:q` does:
    /// a file with changes stays, with its message, and is the one shown); anything else is nothing.
    pub(super) fn pick_tab(&mut self, k: Key, close: bool, out: Out) {
        let Key::Char(c) = k else { return };
        let Some(i) = self.tab_letters().iter().position(|&l| l == c) else { return };
        if !close {
            return (self.cur, self.panel.focus) = (i, false);
        }
        let id = self.bufs[i].id;
        for key in [Key::Char(':'), Key::Char('q'), Key::Enter] {
            self.to(id, Event::Key(key), out);
        }
        if let Some(j) = self.bufs.iter().position(|b| b.id == id) {
            (self.cur, self.panel.focus) = (j, false); // refused: it is shown, and says why
        }
    }

    /// KEY-3: after a key the window of a prefix is shut (the prefix is another now), and while one is
    /// typed a pause is asked for.
    pub(super) fn arm(&mut self, out: Out) {
        (self.which, self.pause) = (None, self.pause + 1);
        if !self.prefix().is_empty() {
            out.push(SimEffect::Pause(self.pause));
        }
    }

    /// KEY-3: the pause is over: the window opens for the prefix that is still typed, if no key came since.
    pub(super) fn paused(&mut self, n: u64) {
        let p = self.prefix();
        if n == self.pause && !p.is_empty() && self.panel.live.is_none() {
            self.which = Some(p);
        }
    }

    /// KEY-6: Tab or Shift-Tab showed another file: one use more, kept until there are three.
    pub(super) fn used_tab(&mut self, out: Out) {
        if self.tab_uses < LEARNED {
            self.tab_uses += 1;
            out.push(SimEffect::TabUses(self.tab_uses));
        }
    }

    /// KEY-6: the first time a second file is open in a run, the hint says what moves between them,
    /// until Tab has been used three times.
    pub(super) fn tab_hint(&mut self) {
        if self.bufs.len() == 2 && !self.hinted && self.tab_uses < LEARNED {
            (self.hinted, self.msg) = (true, Some(HINT.into()));
        }
    }
}

/// A file's name as a tab letter is taken from it: the last part of its path; HEAD's version of a file
/// (DIFF-3) keeps its `HEAD:`, as its tab says it.
fn tab_name(path: Option<&str>) -> String {
    let p = path.unwrap_or("[no name]");
    let (head, rest) = p.strip_prefix("HEAD:").map_or(("", p), |r| ("HEAD:", r));
    format!("{head}{}", rest.rsplit('/').next().unwrap_or(rest))
}

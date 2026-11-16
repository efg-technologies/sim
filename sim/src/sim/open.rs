//! OPEN-1, OPEN-2
//! What the text names, opened (pure): `gf` the file named at the cursor, in Sim; `gx` the web address at
//! the cursor, by the system's opener, which the shell starts. In a Markdown file the inline link the cursor
//! is in names its target. The shell looks for a file on the disk; nothing here reads one.
use super::panel::Out;
use super::{Sim, SimEffect, is_markdown, text_of};
use crate::lie::{Budget, line_marks};
use sim_core::core::cli;
use std::ops::Range;
use std::path::{Component, PathBuf};

/// The chars that end a name or an address in a line, as a blank does.
const ENDS: &str = "\"'`<>()[]{}|,;";

/// What the cursor (`at`) names in the chars `cs` of a line, and the chars that name it: in a Markdown file
/// (`md`) the target of the inline link `[text](target)` that holds it (up to a blank, without `<` `>` and a
/// `#`); else the run of chars around it that no blank and nothing of `ENDS` ends, without the stops after it.
fn named(cs: &[char], at: usize, md: bool) -> Option<(String, Range<usize>)> {
    for i in (0..cs.len().saturating_sub(1)).filter(|&i| md && cs[i] == ']' && cs[i + 1] == '(') {
        let (Some(open), Some(n)) =
            (cs[..i].iter().rposition(|&c| c == '['), cs[i..].iter().position(|&c| c == ')'))
        else {
            continue;
        };
        if (open..i + n + 1).contains(&at) {
            let t: String = cs[i + 2..i + n].iter().collect();
            let t = t.split_whitespace().next().unwrap_or("").trim_matches(['<', '>']);
            return Some((t.split('#').next().unwrap_or("").into(), open..i + n + 1));
        }
    }
    let end = |c: &char| c.is_whitespace() || ENDS.contains(*c);
    if cs.get(at).is_none_or(end) {
        return None;
    }
    let from = cs[..at].iter().rposition(end).map_or(0, |i| i + 1);
    let to = at + cs[at..].iter().position(end).unwrap_or(cs.len() - at);
    let word: String = cs[from..to].iter().collect();
    Some((word.trim_end_matches(['.', ',', ':', ';', '!', '?']).into(), from..to))
}

impl Sim {
    /// What the cursor of the file shown names, with the line's chars and the line: OPEN-1, OPEN-2.
    fn named_here(&self) -> Option<(String, Range<usize>, Vec<char>, usize)> {
        let b = self.bufs.get(self.cur)?;
        let (buf, md) = (&b.app.buf, is_markdown(b.app.path.as_deref()));
        let l = buf.line_of(buf.cursor);
        let cs: Vec<char> = text_of(buf, l)?.chars().collect();
        let (name, span) = named(&cs, buf.col(), md).filter(|n| !n.0.is_empty())?;
        Some((name, span, cs, l))
    }

    /// OPEN-1: `gf` asks the shell for the file the cursor names, beside the file first and then in the root,
    /// with the keys of its `:line:col` to type once it is open.
    pub(super) fn gf(&mut self, out: Out) {
        let Some((word, ..)) = self.named_here() else {
            return self.msg = Some("gf: no file name under the cursor".into());
        };
        let Some(c) = cli::parse(&["--".into(), word], |_| false) else { return };
        let (Some(name), Some(b)) = (c.file, self.bufs.get(self.cur)) else { return };
        let p = match (name.strip_prefix("~/"), &self.home) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(&name),
        };
        let here = b.app.path.as_ref().and_then(|f| Some(self.cwd.join(f).parent()?.join(&p)));
        let at = [here, Some(self.root.join(&p))];
        let at = at.into_iter().flatten().map(|p| words(&p)).collect();
        out.push(SimEffect::Seek(name, at, c.keys));
    }

    /// OPEN-2: `gx` hands the web address under the cursor to the opener, unless MD-10 marks its link.
    pub(super) fn gx(&mut self, out: Out) {
        if self.plain {
            return self.msg = Some("plain mode runs no program: not opened".into());
        }
        let host = |u: &str| {
            let rest = u.split_once("://").map_or("", |x| x.1);
            let auth = rest.split(['/', '?', '#']).next().unwrap_or("");
            auth.rsplit_once('@').map_or(auth, |x| x.1).to_string()
        };
        let web = |u: &str| {
            let low = u.get(..8).unwrap_or(u).to_ascii_lowercase();
            (low.starts_with("http://") || low.starts_with("https://"))
                && u.chars().all(|c| c.is_ascii_graphic() && !"\"'`\\<>".contains(c))
                && !host(u).is_empty()
        };
        let Some((url, span, cs, l)) = self.named_here().filter(|u| web(&u.0)) else {
            return self.msg = Some("gx: no web address under the cursor".into());
        };
        let Some(b) = self.bufs.get(self.cur) else { return };
        let (buf, refs) = (&b.app.buf, || self.refs_of(b));
        let marks =
            line_marks(l, &cs, &[], &|k| text_of(buf, k), Some(&refs), &mut Budget::frame());
        if marks.iter().any(|m| m.start < span.end && span.start < m.end) {
            return self.msg = Some(format!("not opened: the link is marked, it goes to {url}"));
        }
        self.msg = Some(format!("opened {}", host(&url)));
        out.push(SimEffect::Browse(url));
    }
}

/// OPEN-1: a path with `..` taken out by its words (`src/../a.md` is `a.md`); a link is not followed.
fn words(p: &std::path::Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir if out.file_name().is_some() => drop(out.pop()),
            c => out.push(c),
        }
    }
    out
}

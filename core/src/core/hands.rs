//! VI-18
//! What the hands bring from the owner's Neovim: the comment toggle (`gc`).
use super::app::App;

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

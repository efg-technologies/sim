//! DIFF-7, DIFF-8
//! The preview of the changes view (pure): git's diff of one file against HEAD, made into the text the text
//! pane shows from its first hunk on (what comes before it, `diff --git`, `index`, `---` and `+++`, says
//! nothing a row does not), what each line of it is, the line of the file it is now, and where each hunk
//! starts with the first line of the file it changes.

/// What a line of the preview is: a hunk's header, a line that went (`-`), one that came (`+`), or one that
/// stayed (the context, and git's `\ No newline at end of file`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Is {
    Head,
    Went,
    Came,
    Same,
}

/// A file's diff as the preview shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Patch {
    /// The lines from the first hunk's header on, as git wrote them; or, with no hunk, why: one line.
    pub text: String,
    /// What each line is, and its line in the file now (1-based; none for a line that went and a header).
    pub lines: Vec<(Is, Option<usize>)>,
    /// Each hunk: the line of the text its header is, and the first line of the file it changes (1-based).
    pub hunks: Vec<(usize, usize)>,
}

impl Patch {
    /// A preview with no hunk: why there is none, as a header's line.
    pub fn note(why: &str) -> Patch {
        Patch { text: format!("{why}\n"), lines: vec![(Is::Head, None)], hunks: vec![] }
    }

    /// `git diff`'s output for a file (or for a rename's two names). A binary file and a diff with no hunk (a
    /// mode, a submodule) say so.
    pub fn parse(diff: &[u8]) -> Patch {
        let all = String::from_utf8_lossy(diff);
        let (mut p, mut head, mut now, mut found, mut binary) =
            (Patch::default(), true, 0, false, false);
        for l in all.split_terminator('\n') {
            // (a body line starts with ` `, `+`, `-` or `\`: `diff ` begins another file's header)
            head |= l.starts_with("diff ");
            if head && !l.starts_with("@@") {
                binary |= l.starts_with("Binary files ");
                continue;
            }
            head = false;
            let (is, n) = match l.as_bytes().first() {
                Some(b'@') => {
                    (now, found) = (new_start(l), false);
                    p.hunks.push((p.lines.len(), now.max(1)));
                    (Is::Head, None)
                }
                Some(b'+') => (Is::Came, Some(now)),
                Some(b'-') => (Is::Went, None),
                Some(b' ') => (Is::Same, Some(now)),
                _ => (Is::Same, None),
            };
            if !found
                && matches!(is, Is::Came | Is::Went)
                && let Some(h) = p.hunks.last_mut()
            {
                (found, h.1) = (true, now.max(1));
            }
            now += usize::from(n.is_some());
            p.lines.push((is, n));
            p.text.push_str(l);
            p.text.push('\n');
        }
        match (p.hunks.is_empty(), binary, all.is_empty()) {
            (false, ..) => p,
            (_, true, _) => Patch::note("binary file: no diff to show"),
            (_, _, true) => Patch::note("no changes from HEAD"),
            _ => Patch::note("no change in the text"),
        }
    }
}

/// The first line of the new side of a hunk's header (`@@ -1,3 +4,5 @@` is 4).
fn new_start(head: &str) -> usize {
    let plus = head.split(' ').find_map(|w| w.strip_prefix('+'));
    plus.and_then(|w| w.split(',').next()?.parse().ok()).unwrap_or(1)
}

/// DIFF-7: an untracked file as git would write its diff against nothing: one hunk of `+` lines, and a binary
/// file as git says one is.
pub fn added(bytes: &[u8]) -> Vec<u8> {
    let Some(n) = crate::changes::lines(bytes) else { return b"Binary files differ\n".to_vec() };
    let mut out = format!("@@ -0,0 +1,{n} @@\n").into_bytes();
    for l in bytes.split_inclusive(|&b| b == b'\n') {
        out.push(b'+');
        out.extend_from_slice(l);
    }
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    out
}

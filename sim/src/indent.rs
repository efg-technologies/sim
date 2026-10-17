//! UI-6
//! Indentation (pure): the width a file indents by, found from its first lines, the columns the
//! leading blanks of a line cover, and the columns of its indent guides.

/// How a file is indented: with tabs (shown as four columns) or with spaces by 2 or 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Indent {
    pub tabs: bool,
    pub width: u16,
}

impl Default for Indent {
    fn default() -> Indent {
        Indent { tabs: false, width: 4 }
    }
}

/// How many lines are looked at to find the indentation.
pub const LOOK: usize = 1000;

/// The leading blanks of a line: the columns they cover (a tab is to the next multiple of 4, as it
/// is shown), whether one is a tab, and whether the line is only blanks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lead {
    pub cols: u16,
    pub tab: bool,
    pub blank: bool,
}

pub fn lead(line: &str) -> Lead {
    let (mut cols, mut tab, mut chars) = (0u16, false, 0usize);
    for c in line.chars() {
        match c {
            ' ' => cols += 1,
            '\t' => (cols, tab) = (cols + 4 - cols % 4, true),
            _ => break,
        }
        chars += 1;
    }
    Lead { cols, tab, blank: chars == line.chars().count() }
}

/// The indentation of a file from its first lines: tabs when more lines start with a tab than with
/// a blank; else 2 when the lines indented by 2 more than a multiple of 4 are at least a fifth of
/// the indented lines, else 4.
pub fn detect<'a>(lines: impl Iterator<Item = &'a str>) -> Indent {
    let (mut tabbed, mut spaced, mut two) = (0usize, 0usize, 0usize);
    for l in lines.take(LOOK) {
        match l.chars().next() {
            Some('\t') => tabbed += 1,
            Some(' ') => {
                let d = lead(l);
                if d.blank {
                    continue;
                }
                spaced += 1;
                two += usize::from(d.cols % 4 == 2);
            }
            _ => {}
        }
    }
    if tabbed > spaced {
        Indent { tabs: true, width: 4 }
    } else {
        Indent { tabs: false, width: if spaced > 0 && two * 5 >= spaced { 2 } else { 4 } }
    }
}

/// The columns of the guides of a line whose blanks cover `cols`: 0, `width`, 2 `width` and so on
/// while they are before where the text starts; from level `from` (a blank line has none at 0).
pub fn guides(cols: u16, width: u16, from: u16) -> Vec<u16> {
    let w = width.max(1);
    (from..).map(|k| k * w).take_while(|&c| c < cols).collect()
}

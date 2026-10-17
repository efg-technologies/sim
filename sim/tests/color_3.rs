//! COLOR-3: what a screen says without its colors. Drawn with none (`NO_COLOR`, COLOR-2), a changed
//! line, an added one and a deleted one are three shapes in the sign column, a word that came from
//! the disk is inverse and a hidden character inverse and bold, a folder says the strongest git
//! status below it in a letter, a tab says "changed on disk" with `↯` and "unsaved" with a bold `●`,
//! and the mode is a word. The screens are golden files: each row, and under it what the terminal
//! is told about its cells: `.` plain, `b` bold, `f` faint, `i` italic, `u` underline, `r` inverse,
//! `R` inverse and bold, `c` faint italic, `?` any other mix.
#![allow(clippy::unwrap_used, clippy::panic)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::layout::Skin;
use sim::paint::Painter;
use sim::sim::SimEvent;
use sim_core::core::app::Event;

const BOLD: u8 = 1;
const FAINT: u8 = 2;
const ITALIC: u8 = 4;
const UNDER: u8 = 8;
const INVERSE: u8 = 16;

/// What a terminal would show of the bytes the Painter wrote for one whole screen: per row, per
/// column, the char and its attributes. Only the sequences the Painter writes are understood: the
/// cursor to a row, SGR, and the rest are skipped; a color is an error.
fn cells(bytes: &[u8], width: usize, height: usize) -> Vec<Vec<(char, u8)>> {
    let mut grid = vec![vec![(' ', 0u8); width]; height];
    let (text, mut row, mut col, mut now) = (String::from_utf8_lossy(bytes), 0, 0, 0u8);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            if row < height && col < width && !c.is_control() {
                grid[row][col] = (c, now);
                col += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0).max(1);
            }
            continue;
        }
        if chars.next() != Some('[') {
            continue;
        }
        let mut params = String::new();
        while let Some(&p) = chars.peek().filter(|p| !p.is_ascii_alphabetic()) {
            params.push(p);
            chars.next();
        }
        let params = params;
        match chars.next() {
            Some('H') => {
                let mut at = params.split(';').filter_map(|p| p.parse::<usize>().ok());
                (row, col) = (at.next().unwrap_or(1) - 1, at.next().unwrap_or(1) - 1);
            }
            Some('m') => {
                for p in params.split(';').map(|p| p.parse::<u16>().unwrap_or(0)) {
                    now = match p {
                        0 => 0,
                        1 => now | BOLD,
                        2 => now | FAINT,
                        3 => now | ITALIC,
                        4 => now | UNDER,
                        7 => now | INVERSE,
                        other => panic!(
                            "{other} in ESC[{params}m is not weight, slant, underline or inverse"
                        ),
                    };
                }
            }
            _ => {}
        }
    }
    grid
}

fn mark(attrs: u8) -> char {
    match attrs {
        0 => '.',
        BOLD => 'b',
        FAINT => 'f',
        ITALIC => 'i',
        UNDER => 'u',
        INVERSE => 'r',
        x if x == INVERSE | BOLD => 'R',
        x if x == FAINT | ITALIC => 'c',
        _ => '?',
    }
}

/// The screen as the golden file has it.
fn stripped(r: &Rig) -> (String, Vec<Vec<(char, u8)>>) {
    let f = &r.shot.frame;
    let mut p = Painter::new(Vec::<u8>::new());
    p.draw(r.shot.clone()).unwrap();
    let grid = cells(&p.out, usize::from(f.width), usize::from(f.height));
    let mut out = format!("{}x{}\n", f.width, f.height);
    for row in &grid {
        let text: String = row.iter().map(|c| c.0).collect();
        out += &format!("|{}|\n", text.trim_end());
        if row.iter().any(|c| c.1 != 0) {
            out += &format!("|{}|\n", row.iter().map(|c| mark(c.1)).collect::<String>().trim_end());
        }
    }
    (out, grid)
}

/// Where `what` is on the screen: its row and column.
fn find(grid: &[Vec<(char, u8)>], what: &str) -> (usize, usize) {
    let want: Vec<char> = what.chars().collect();
    for (y, row) in grid.iter().enumerate() {
        let text: Vec<char> = row.iter().map(|c| c.0).collect();
        if let Some(x) = text.windows(want.len()).position(|w| w == &want[..]) {
            return (y, x);
        }
    }
    panic!("{what:?} is not on the screen")
}

/// The cell a few columns left of `what`: where the sign column of its line is.
fn sign_of(grid: &[Vec<(char, u8)>], r: &Rig, what: &str) -> (char, u8) {
    let (y, _) = find(grid, what);
    grid[y][usize::from(r.sim.geo().ex)]
}

/// A repository whose `src/a.rs` has a changed line, an added one and a deleted one, with an
/// untracked file in `docs/` and a deleted one in `old/`; the panel is open with `src/` expanded and
/// `src/a.rs` open in depth, with no color and no icons.
fn scene(name: &str) -> Rig {
    let text: String = (1..=10).map(|i| format!("l{i}\n")).collect();
    let (a, b) = (format!("src/a.rs={text}"), "src/b.rs=b\n");
    let mut r =
        rig_env(name, &[&a, b, "docs/c.md=# c\n", "old/gone.txt=x\n"], &[], (90, 20), |e| {
            git(&e.cwd, &["init", "-q"]);
            git(&e.cwd, &["add", "-A"]);
            git(&e.cwd, &["commit", "-q", "-m", "init"]);
            std::fs::write(e.cwd.join("src/a.rs"), "l1\nL2\nl3\nl4\nl5\nnew\nl6\nl7\nl9\nl10\n")
                .unwrap();
            std::fs::remove_file(e.cwd.join("old/gone.txt")).unwrap();
            make(&e.cwd, &["docs/new.md"]);
            e.skin = Skin::Depth;
            e.chrome = true;
            e.syn = Syntax::mono();
            e.home = Some(e.cwd.clone()); // (the root is shown as `~`, not as a temporary directory's name)
        });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e)))); // (ignored: nothing asked)
    r.keys("ggjjljl"); // src/ opened, a.rs chosen
    r.keys("<CR>");
    assert!(
        r.status().contains("a.rs") || r.text().contains("L2"),
        "a.rs is open:\n{}",
        stripped(&r).0
    );
    r
}

#[test]
fn color_3_changed_added_and_deleted_lines_are_three_shapes_in_the_sign_column() {
    let r = scene("color-3-signs");
    let (shot, grid) = stripped(&r);
    let (changed, added, deleted, none) = (
        sign_of(&grid, &r, "L2"),
        sign_of(&grid, &r, "new"),
        sign_of(&grid, &r, "l7"),
        sign_of(&grid, &r, "l3"),
    );
    assert_eq!((changed.0, added.0, deleted.0, none.0), ('┆', '▎', '▁', ' '), "{shot}");
    golden("color_3_signs", &shot);
}

#[test]
fn color_3_a_folder_says_the_strongest_status_below_it_in_a_letter() {
    let r = scene("color-3-folders");
    let (shot, grid) = stripped(&r);
    for (folder, letter) in [("▸ docs/", 'U'), ("▸ old/", 'D'), ("▾ src/", 'M')] {
        let (y, _) = find(&grid, folder);
        let row: String = grid[y].iter().map(|c| c.0).collect();
        let panel: String = row.chars().take(usize::from(r.sim.geo().pw) + 1).collect();
        assert_eq!(panel.trim_end().chars().last(), Some(letter), "{folder}: {panel:?}\n{shot}");
        assert!(!panel.contains('●'), "no dot: {panel:?}");
    }
    // the open file's row is bold: its surface is not drawn
    let (y, x) = find(&grid, "a.rs");
    assert_eq!(grid[y][x].1 & BOLD, BOLD, "{shot}");
    golden("color_3_folders", &shot);
}

#[test]
fn color_3_a_tab_says_unsaved_with_a_bold_dot_and_changed_on_disk_with_an_arrow() {
    let mut r = scene("color-3-tabs");
    r.keys("<C-w>l:e src/b.rs\nx<Esc>:e src/a.rs\n"); // b.rs is unsaved, a.rs shown
    let b = r.sim.bufs.iter().find(|b| b.app.path.as_deref().is_some_and(|p| p.ends_with("b.rs")));
    let b = b.unwrap().id;
    r.send(SimEvent::App(b, disk(b"from the disk\n")));
    let (shot, grid) = stripped(&r);
    let (y, x) = find(&grid, "b.rs ");
    let bar: String = grid[y].iter().map(|c| c.0).collect();
    assert!(bar.contains("b.rs ● ↯"), "unsaved, and changed on disk: {bar}\n{shot}");
    assert_eq!(grid[y][x + 5], ('●', BOLD), "unsaved: a bold dot\n{shot}");
    assert_eq!(grid[y][x + 7].0, '↯', "{shot}");
    golden("color_3_tabs", &shot);
}

#[test]
fn color_3_a_word_from_the_disk_is_inverse_a_hidden_char_is_inverse_and_bold_and_the_mode_a_word() {
    let mut r = scene("color-3-words");
    let id = r.sim.bufs[0].id;
    let theirs = "l1\nL2\nl3\nl4\nl5\nnew\nl6\nl7\nl9\nl10 from the disk\n";
    r.send(SimEvent::App(id, disk(theirs.as_bytes())));
    let (shot, grid) = stripped(&r);
    let (y, x) = find(&grid, "from the disk");
    assert_eq!(grid[y][x].1, INVERSE, "a word that came from the disk\n{shot}");
    assert_eq!(grid[y][x - 2].1 & INVERSE, 0, "the text before it is not\n{shot}");
    // a hidden char: the right-to-left override, drawn as itself, in reverse and bold
    r.keys("gg$a\u{202e}<Esc>");
    let (shot, grid) = stripped(&r);
    let (y, x) = find(&grid, "l1");
    assert_eq!(grid[y][x + 2].1, INVERSE | BOLD, "a hidden char\n{shot}");
    golden("color_3_words", &shot);
    // the mode is a word, in a block
    let mut words = vec![];
    for keys in ["", "i", "<Esc>v", "<Esc>V", "<Esc>:"] {
        r.keys(keys);
        let (_, grid) = stripped(&r);
        let last: String = grid[grid.len() - 1].iter().map(|c| c.0).collect();
        words.push(last.split_whitespace().next().unwrap_or_default().to_string());
    }
    assert_eq!(words[..3], ["NORMAL", "INSERT", "VISUAL"], "{words:?}");
}

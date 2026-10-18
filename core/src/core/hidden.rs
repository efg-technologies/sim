//! SAFE-10
//! Which chars are never drawn as they are: the ones that are invisible, or only change how text is
//! read or drawn (against Trojan Source, ASCII smuggling and GlassWorm). The one table of them; what
//! the editor and the core draw as a form is decided by `view::show`, which asks it.
use std::sync::OnceLock;

/// The runs of such chars, made by `scripts/hidden.py` from the `UnicodeData.txt` of Unicode 18.0.0:
/// the format chars (Cf), the unassigned ones (Cn), the C1 controls and the invisible chars SAFE-10
/// names. Private use (Co) is not among them: it draws a glyph. In hex, sorted: the first char of a
/// run, then the first char after it.
const RUNS: &str = include_str!("hidden.tbl");

/// Whether `c` is one of them (SAFE-10). A few are drawn in a legitimate use (SAFE-11, `view::show`).
/// Every char of a screen is asked, so ASCII is never looked up, and the table is searched in the page of
/// 256 chars `c` is in (a few bounds; the first two planes have pages).
pub fn hidden(c: char) -> bool {
    if c < '\u{80}' {
        return false;
    }
    static TABLE: OnceLock<(Vec<u32>, Vec<usize>)> = OnceLock::new();
    let (bounds, pages) = TABLE.get_or_init(|| {
        let b: Vec<u32> =
            RUNS.split_ascii_whitespace().filter_map(|h| u32::from_str_radix(h, 16).ok()).collect();
        let below: Vec<usize> = (0..=512u32).map(|p| b.partition_point(|&x| x < p << 8)).collect();
        (b, below)
    });
    let (u, page) = (c as u32, c as usize >> 8);
    let in_runs = match pages.get(page + 1) {
        Some(&to) => pages[page] + bounds[pages[page]..to].partition_point(|&b| b <= u),
        None => bounds.partition_point(|&b| b <= u), // (the planes past the second: nearly none)
    };
    in_runs % 2 == 1
}

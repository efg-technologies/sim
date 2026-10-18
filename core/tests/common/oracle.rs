//! What SAFE-10 and SAFE-11 say, written out again from SPEC and from the Unicode Character
//! Database 18.0.0, so that the tests that use it do not trust the code's own table. Nothing here
//! depends on the core (sim's tests include this file too).
#![allow(dead_code, clippy::unwrap_used)]

/// The chars SAFE-10 names (U+2028 and U+2029 are its own addition).
#[rustfmt::skip]
pub const NAMED: &[(u32, u32)] = &[
    (0xAD, 0xAD), (0x34F, 0x34F), (0x61C, 0x61C), (0x115F, 0x1160), (0x17B4, 0x17B5), (0x180B, 0x180F),
    (0x200B, 0x200F), (0x2028, 0x2029), (0x202A, 0x202E), (0x2060, 0x2064), (0x2066, 0x206F),
    (0x3164, 0x3164), (0xFE00, 0xFE0F), (0xFEFF, 0xFEFF), (0xFFA0, 0xFFA0), (0xFFF9, 0xFFFB),
    (0x1D173, 0x1D17A), (0xE0000, 0xE007F), (0xE0100, 0xE01EF),
];

/// Every char of category Cf in Unicode 18.0.0.
#[rustfmt::skip]
pub const CF: &[(u32, u32)] = &[
    (0xAD, 0xAD), (0x600, 0x605), (0x61C, 0x61C), (0x6DD, 0x6DD), (0x70F, 0x70F), (0x890, 0x891),
    (0x8E2, 0x8E2), (0x180E, 0x180E), (0x200B, 0x200F), (0x202A, 0x202E), (0x2060, 0x2064),
    (0x2066, 0x206F), (0xFEFF, 0xFEFF), (0xFFF9, 0xFFFB), (0x110BD, 0x110BD), (0x110CD, 0x110CD),
    (0x13430, 0x1343F), (0x1BCA0, 0x1BCA3), (0x1D173, 0x1D17A), (0xE0001, 0xE0001), (0xE0020, 0xE007F),
];

/// The planes nothing has been assigned in (4 to 13), and the C1 controls.
pub const ELSEWHERE: &[(u32, u32)] = &[(0x40000, 0xDFFFF), (0x80, 0x9F)];

/// Private use (Co): not hidden, they draw a glyph (an icon of a Nerd Font, or a box).
pub const PRIVATE: &[(u32, u32)] = &[(0xE000, 0xF8FF), (0xF0000, 0xFFFFD), (0x100000, 0x10FFFD)];

/// Code points that no version of Unicode has assigned (or that were reserved for good).
#[rustfmt::skip]
pub const UNASSIGNED: &[u32] = &[
    0x378, 0x379, 0x38B, 0x2065, 0xFDD0, 0xFDEF, 0xFF00, 0xFFF0, 0xFFF8, 0xFFFE, 0xFFFF, 0xE0002, 0xE001F,
    0xE0080, 0xE00FF, 0xE01F0, 0xE0FFF, 0x1FFFE, 0x2FFFF, 0x3FFFF, 0xFFFFE, 0xFFFFF, 0x10FFFE, 0x10FFFF,
];

pub fn in_ranges(rs: &[(u32, u32)], u: u32) -> bool {
    rs.iter().any(|&(a, b)| (a..=b).contains(&u))
}

/// What must be hidden by the lists above.
pub fn must_hide(u: u32) -> bool {
    in_ranges(NAMED, u) || in_ranges(CF, u) || in_ranges(ELSEWHERE, u) || UNASSIGNED.contains(&u)
}

/// What is surely assigned and drawn, and so must not be touched. (Four letters are the fillers
/// that SAFE-10 names.)
pub fn must_show(c: char) -> bool {
    (c.is_alphanumeric() || c.is_uppercase() || c.is_lowercase())
        && !matches!(c, '\u{115f}' | '\u{1160}' | '\u{3164}' | '\u{ffa0}')
}

pub fn pict(c: char) -> bool {
    matches!(c as u32, 0xA9 | 0xAE | 0x203C | 0x2049 | 0x2122 | 0x2139 | 0x2190..=0x2BFF | 0x1F000..=0x1FAFF | 0x3030 | 0x303D | 0x3297 | 0x3299)
}

/// The three subdivision flags that Unicode recommends: England, Scotland, Wales.
fn rgi_flags() -> Vec<Vec<char>> {
    ["gbeng", "gbsct", "gbwls"]
        .map(|code| {
            let tags = code.chars().map(|c| char::from_u32(0xE0000 + c as u32).unwrap());
            std::iter::once('🏴').chain(tags).chain(['\u{e007f}']).collect()
        })
        .into()
}

/// Whether the hidden char `cs[i]` may stand raw in a row. Looser than the code about the ZWJ and
/// the selectors (any ZWJ between pictographs, one selector after a pictograph); exact about the
/// rest: an ideographic variation selector never, a tag only in one of the three RGI flags.
pub fn may_stand(cs: &[char], i: usize) -> bool {
    let (prev, next) = (i.checked_sub(1).map(|p| cs[p]), cs.get(i + 1).copied());
    match cs[i] as u32 {
        0x200D => prev.is_some_and(|p| pict(p) || p == '\u{fe0f}') && next.is_some_and(pict),
        0xFE0E => prev.is_some_and(pict),
        0xFE0F => {
            prev.is_some_and(pict)
                || prev.is_some_and(|p| p.is_ascii_digit() || "#*".contains(p))
                    && next == Some('\u{20e3}')
        }
        0xE0020..=0xE007F => rgi_flags().iter().any(|f| {
            (i.saturating_sub(6)..=i).any(|s| cs.get(s..s + 7).is_some_and(|w| w == f.as_slice()))
        }),
        _ => false,
    }
}

/// The first hidden char of `text` that stands raw without a reason.
pub fn raw_hidden(text: &str) -> Option<char> {
    let cs: Vec<char> = text.chars().collect();
    (0..cs.len())
        .find(|&i| {
            (must_hide(cs[i] as u32) || cs[i].is_control()) && cs[i] != '\n' && !may_stand(&cs, i)
        })
        .map(|i| cs[i])
}

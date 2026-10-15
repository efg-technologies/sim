//! PERF-3, VIS-7, VIS-10, CLI-4, VI-8, VI-9, VI-12
//! crossterm: diffed Frame output, OSC 52, cursor shape.
use crate::core::app::{Event, Key};
use crate::core::view::{Frame, Row, Shape, Style, width};
use crossterm::event::{Event as Ev, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::io::{self, Write};

/// Alternate screen, bracketed paste, kitty keyboard protocol (disambiguate escape codes).
pub const ENTER: &[u8] = b"\x1b[?1049h\x1b[?2004h\x1b[>1u";
/// Undoes `ENTER`, and gives the cursor back its default shape and visibility.
pub const LEAVE: &[u8] = b"\x1b[<1u\x1b[?2004l\x1b[?1049l\x1b[0 q\x1b[?25h";

pub fn enter(out: &mut impl Write) -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    out.write_all(ENTER)?;
    out.flush()
}

/// Restores the terminal. Errors are ignored: it also runs from the panic hook and after SIGHUP.
pub fn leave(out: &mut impl Write) {
    let _ = out.write_all(LEAVE).and_then(|()| out.flush());
    let _ = crossterm::terminal::disable_raw_mode();
}

/// Turns a crossterm event into an `Event`; keys si has no use for are dropped. A lone Esc needs
/// no timeout here: with the kitty flags on, the terminal sends Esc as `CSI 27 u`, and crossterm
/// reads a bare `0x1b` with no bytes behind it as Esc at once.
pub fn convert(ev: Ev) -> Option<Event> {
    match ev {
        Ev::Key(k)
            if k.kind != KeyEventKind::Release
                && !k
                    .modifiers
                    .intersects(KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::META) =>
        {
            let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
            #[rustfmt::skip]
            let key = match k.code {
                KeyCode::Esc => Key::Esc, KeyCode::Enter => Key::Enter, KeyCode::Tab => Key::Tab,
                KeyCode::Backspace => Key::Backspace, KeyCode::Delete => Key::Delete,
                KeyCode::Up => Key::Up, KeyCode::Down => Key::Down,
                KeyCode::Left => Key::Left, KeyCode::Right => Key::Right,
                KeyCode::Home => Key::Home, KeyCode::End => Key::End,
                KeyCode::PageUp => Key::PageUp, KeyCode::PageDown => Key::PageDown,
                KeyCode::Char('[') if ctrl => Key::Esc,
                KeyCode::Char(c) if ctrl => Key::Ctrl(c.to_ascii_lowercase()),
                KeyCode::Char(c) => Key::Char(c),
                _ => return None,
            };
            Some(Event::Key(key))
        }
        Ev::Paste(s) => Some(Event::Paste(s)),
        Ev::Resize(w, h) => Some(Event::Resize(w, h)),
        _ => None,
    }
}

/// `convert` for the event loop. Without the kitty protocol Alt+x is sent as ESC x, which is also
/// Esc followed at once by x (a fast typist, tmux send-keys, a slow link that batches bytes). si
/// has no Alt commands: it is Esc, then the key (VI-9).
pub fn convert_all(ev: Ev) -> Vec<Event> {
    if let Ev::Key(k) = ev
        && k.modifiers.contains(KeyModifiers::ALT)
        && matches!(k.code, KeyCode::Char(_) | KeyCode::Enter | KeyCode::Backspace | KeyCode::Tab)
    {
        let plain = KeyEvent { modifiers: k.modifiers - KeyModifiers::ALT, ..k };
        return [Some(Event::Key(Key::Esc)), convert(Ev::Key(plain))]
            .into_iter()
            .flatten()
            .collect();
    }
    convert(ev).into_iter().collect()
}

/// Draws Frames, rewriting only the rows that differ from the previous one.
pub struct Term<W: Write> {
    pub out: W,
    prev: Option<Frame>,
    shape: Option<Shape>,
    /// Inside tmux: the clipboard sequence also goes through its passthrough.
    tmux: bool,
}

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = c.iter().fold(0u32, |n, &b| n << 8 | u32::from(b)) << (8 * (3 - c.len()));
        let sextet = |i: usize| {
            if i <= c.len() { ABC[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' }
        };
        out.extend((0..4).map(sextet));
    }
    out
}

/// OSC 52, and the same wrapped for tmux (every ESC doubled) if `tmux`. tmux's default
/// `set-clipboard external` drops the plain one.
pub fn clipboard(text: &str, tmux: bool) -> Vec<u8> {
    let osc = format!("\x1b]52;c;{}\x1b\\", base64(text.as_bytes()));
    let mut out = osc.clone().into_bytes();
    if tmux {
        out.extend(format!("\x1bPtmux;{}\x1b\\", osc.replace('\x1b', "\x1b\x1b")).bytes());
    }
    out
}

impl<W: Write> Term<W> {
    pub fn new(out: W, tmux: bool) -> Term<W> {
        Term { out, prev: None, shape: None, tmux }
    }

    pub fn clipboard(&mut self, text: &str) -> io::Result<()> {
        self.out.write_all(&clipboard(text, self.tmux)).and_then(|()| self.out.flush())
    }

    pub fn draw(&mut self, f: Frame) -> io::Result<()> {
        let mut b: Vec<u8> = vec![];
        let same = self.prev.as_ref().filter(|p| (p.width, p.height) == (f.width, f.height));
        // A frame that rewrites anything is one synchronized update, so nothing tears.
        let sync = same.is_none_or(|p| p.rows != f.rows);
        if sync {
            b.extend(b"\x1b[?2026h");
        }
        if same.is_none() {
            b.extend(b"\x1b[2J");
        }
        for (i, row) in f.rows.iter().enumerate() {
            if same.is_some_and(|p| p.rows[i] == *row) {
                continue;
            }
            write!(b, "\x1b[{};1H", i + 1)?;
            paint(&mut b, row);
            b.extend(b"\x1b[0m\x1b[K");
        }
        write!(b, "\x1b[{};{}H", f.cursor.1 + 1, f.cursor.0 + 1)?;
        if self.shape != Some(f.shape) {
            write!(b, "\x1b[{} q", f.shape as u8)?;
            self.shape = Some(f.shape);
        }
        if sync {
            b.extend(b"\x1b[?2026l");
        }
        self.prev = Some(f);
        self.out.write_all(&b)?;
        self.out.flush()
    }
}

/// The text of a row with an SGR sequence wherever the style changes.
fn paint(b: &mut Vec<u8>, row: &Row) {
    let (mut now, mut col) = (None, 0u16);
    let cs: Vec<char> = row.text.chars().collect();
    for (i, &c) in cs.iter().enumerate() {
        let style =
            row.spans.iter().find(|(r, _)| r.contains(&col)).map_or(Style::Primary, |s| s.1);
        let underline = row.underline.iter().any(|r| r.contains(&col));
        if now != Some((style, underline)) {
            b.extend(b"\x1b[0");
            // The SGR parameters of Primary, Secondary, Change (reverse) and Deleted (strikethrough).
            b.extend(["", ";2", ";7", ";2;9"][style as usize].as_bytes());
            b.extend(if underline { ";4m" } else { "m" }.as_bytes());
            now = Some((style, underline));
        }
        b.extend(c.encode_utf8(&mut [0; 4]).as_bytes());
        col += width(&cs, i, usize::from(col)) as u16;
    }
}

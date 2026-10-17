//! GUI-6, GUI-11
//! The palette a terminal would have answered with. A window has no terminal to ask (OSC 11 for
//! the background, OSC 4 for the ANSI colors), so the GUI hands sim the replies as events, in the
//! order a terminal sends them. sim derives everything else (si's palette, the tones, the syntax
//! colors, the fog) from these six colors by its own APCA code, so the GUI chooses nothing but them.
use crate::api::{Rgb, SimEvent};

/// The six colors a terminal reports: the background and the ANSI colors sim asks for
/// (`sim::color::QUERY`: red, green, yellow, magenta, cyan).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Rgb,
    pub red: Rgb,
    pub green: Rgb,
    pub yellow: Rgb,
    pub magenta: Rgb,
    pub cyan: Rgb,
}

/// One Dark's colors (`#282c34` is one of VIS-4's eight backgrounds).
pub const DARK: Theme = Theme {
    name: "dark",
    bg: (0x28, 0x2c, 0x34),
    red: (0xe0, 0x6c, 0x75),
    green: (0x98, 0xc3, 0x79),
    yellow: (0xe5, 0xc0, 0x7b),
    magenta: (0xc6, 0x78, 0xdd),
    cyan: (0x56, 0xb6, 0xc2),
};

/// Catppuccin Latte's (`#eff1f5` is also one of VIS-4's eight).
pub const LIGHT: Theme = Theme {
    name: "light",
    bg: (0xef, 0xf1, 0xf5),
    red: (0xd2, 0x0f, 0x39),
    green: (0x40, 0xa0, 0x2b),
    yellow: (0xdf, 0x8e, 0x1d),
    magenta: (0x88, 0x39, 0xef),
    cyan: (0x17, 0x92, 0x99),
};

impl Theme {
    /// `dark` or `light`; anything else is the dark default.
    pub fn named(name: &str) -> Theme {
        if name.eq_ignore_ascii_case("light") { LIGHT } else { DARK }
    }

    /// From `SIM_GUI_THEME` (`dark`, `light`; unset or `auto` is the system's, given by the caller).
    pub fn from_env(system_light: Option<bool>) -> Theme {
        Theme::pick(std::env::var("SIM_GUI_THEME").ok().as_deref(), system_light)
    }

    /// The word of the environment first, then the system's, then dark.
    fn pick(word: Option<&str>, system_light: Option<bool>) -> Theme {
        match word {
            Some("light") => LIGHT,
            Some("dark") => DARK,
            _ if system_light == Some(true) => LIGHT,
            _ => DARK,
        }
    }

    /// GUI-6: what a terminal's answers become: one event with the background and the ANSI colors sim
    /// asks for (red, green, yellow, magenta, cyan: 1, 2, 3, 5, 6), as `SimEvent::Colors` says them.
    pub fn events(&self) -> Vec<SimEvent> {
        let mut ansi = [None; 8];
        for (i, c) in
            [(1, self.red), (2, self.green), (3, self.yellow), (5, self.magenta), (6, self.cyan)]
        {
            ansi[i] = Some(c);
        }
        vec![SimEvent::Colors(self.bg, ansi)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Syntax, Tones, apca, palette};

    /// What sim's `Syntax` and `Tones` make of a theme's event, as sim would (the same calls).
    fn derive(t: &Theme) -> (Syntax, Tones) {
        let mut syn = Syntax::new(true);
        for e in t.events() {
            let SimEvent::Colors(bg, ansi) = e else { unreachable!() };
            syn.bg(bg);
            ansi.iter().enumerate().for_each(|(i, c)| c.iter().for_each(|c| syn.set(i, *c)));
        }
        let tones = Tones::with(&palette(t.bg), &syn);
        (syn, tones)
    }

    #[test]
    fn gui_11_a_theme_answers_every_query_sim_makes() {
        for t in [DARK, LIGHT] {
            let (syn, _) = derive(&t);
            assert!(!syn.pending(), "{}: sim still waits for a reply", t.name);
        }
    }

    #[test]
    fn gui_11_a_theme_is_one_event_with_the_background_and_five_ansi_colors() {
        let ev = DARK.events();
        assert_eq!(ev.len(), 1);
        let SimEvent::Colors(bg, ansi) = ev[0] else { panic!("{:?}", ev[0]) };
        assert_eq!(bg, (0x28, 0x2c, 0x34));
        assert_eq!(
            ansi.iter().map(Option::is_some).collect::<Vec<_>>(),
            [false, true, true, true, false, true, true, false]
        );
        assert_eq!(ansi[1], Some((0xe0, 0x6c, 0x75)), "red is 1");
        assert_eq!(ansi[6], Some((0x56, 0xb6, 0xc2)), "cyan is 6");
    }

    #[test]
    fn gui_11_the_syntax_colors_read_on_both_themes() {
        // sim's own rule (SYN-1): syntax text at |Lc| >= 75 against the page
        for t in [DARK, LIGHT] {
            let (syn, _) = derive(&t);
            for look in [
                crate::api::Look::Kw,
                crate::api::Look::Str,
                crate::api::Look::Ty,
                crate::api::Look::Fun,
            ] {
                let c = syn.hue(look).expect("derived");
                assert!(apca(c, t.bg).abs() >= 75.0, "{} {look:?}: {c:?}", t.name);
            }
        }
    }

    #[test]
    fn gui_11_light_is_light_and_the_name_picks_the_theme() {
        assert!(LIGHT.bg.0 > 200 && DARK.bg.0 < 80);
        assert_eq!(Theme::named("LIGHT"), LIGHT);
        assert_eq!(Theme::named("anything"), DARK);
        // the environment's word, then the system's, then dark
        assert_eq!(Theme::pick(Some("dark"), Some(true)), DARK);
        assert_eq!(Theme::pick(Some("light"), Some(false)), LIGHT);
        assert_eq!(Theme::pick(Some("auto"), Some(true)), LIGHT);
        assert_eq!(Theme::pick(None, Some(true)), LIGHT);
        assert_eq!(Theme::pick(None, Some(false)), DARK);
        assert_eq!(Theme::pick(None, None), DARK);
    }
}

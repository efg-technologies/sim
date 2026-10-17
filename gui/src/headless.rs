//! GUI-4, GUI-14
//! sim without a window or a terminal: the shell running its jobs inline, keys typed from a script.
//! What the offscreen renders and the tests drive. The same input always gives the same screen.
use crate::api::*;
use crate::demo;
use crate::input::{notation, to_sim};
use crate::safe_io::PrivateDir;
use crate::session::{Exec, Session, Settings};
use crate::theme::Theme;
use std::path::{Path, PathBuf};

pub struct Headless {
    pub session: Session,
    pub dir: PathBuf,
    /// The directories made here (the journal's, the demo's): each with a name made of chance, at mode
    /// 0700, and removed when this is dropped.
    owned: Vec<PrivateDir>,
}

impl Headless {
    /// sim started on `args` in `dir`, with a private journal directory.
    pub fn open(
        dir: &Path,
        home: Option<&Path>,
        args: &[&str],
        size: (u16, u16),
        theme: &Theme,
        icons: bool,
    ) -> Headless {
        // SAFE-6: sim uses a journal directory only if it is private; this one is made fresh
        let state = PrivateDir::new("sim-gui-state-").ok();
        let cfg = Settings {
            state: state.as_ref().map(|d| d.path().to_path_buf()),
            history: None,
            home: home.map(Path::to_path_buf),
            icons,
            size,
        };
        let args: Vec<String> = args.iter().map(ToString::to_string).collect();
        let Some((session, _)) = Session::new(&args, dir, &cfg, theme, Exec::Inline) else {
            // a usage error of sim's command line: the caller gave arguments sim does not take
            return Headless::open(dir, home, &[], size, theme, icons);
        };
        Headless { session, dir: dir.to_path_buf(), owned: state.into_iter().collect() }
    }

    /// The demo project (see `demo`), made in a directory of its own, and sim started in it.
    pub fn demo(name: &str, args: &[&str], size: (u16, u16), theme: &Theme) -> Headless {
        Headless::demo_with(name, args, size, theme, false)
    }

    /// The project is `lantern` in a home of its own (a fresh private directory), so its root reads
    /// `~/lantern` on every machine. `name` says what it is for (it is not part of any path).
    pub fn demo_with(
        name: &str,
        args: &[&str],
        size: (u16, u16),
        theme: &Theme,
        icons: bool,
    ) -> Headless {
        let home = PrivateDir::new("sim-gui-demo-").ok();
        let Some(home) = home else {
            eprintln!("cannot make a private directory for the demo project {name}");
            return Headless::open(Path::new("/nonexistent"), None, args, size, theme, icons);
        };
        let canon =
            std::fs::canonicalize(home.path()).unwrap_or_else(|_| home.path().to_path_buf());
        let dir = canon.join("lantern");
        if let Err(e) = demo::create(&dir) {
            eprintln!("cannot make the demo project {name} in {}: {e}", dir.display());
        }
        let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
        let mut h = Headless::open(&dir, Some(&canon), args, size, theme, icons);
        h.owned.push(home);
        h
    }

    /// Types keys in the notation of `input::notation`.
    pub fn keys(&mut self, s: &str) -> &mut Headless {
        for ev in notation(s) {
            self.event(ev);
        }
        self
    }

    /// One terminal event, through the path of the binary into sim.
    pub fn event(&mut self, ev: crate::api::CtEvent) -> &mut Headless {
        for e in to_sim(ev) {
            self.session.send(e);
        }
        self
    }

    /// Moves the tree's cursor, a row at a time, to the row that says `name` (the cursor row is a layer of
    /// the screen, GUI-1). It stops where it is if no row says it.
    pub fn walk_to(&mut self, name: &str) -> &mut Headless {
        for _ in 0..80 {
            let s = self.screen();
            let y = s.frame.rows.iter().position(|r| r.text.contains(name));
            let at = s.layers.iter().find(|l| l.kind == LayerKind::Cursor);
            if y.is_none() || y == at.map(|l| usize::from(l.rect.y)) {
                break;
            }
            self.keys("j");
        }
        self
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> &mut Headless {
        self.session.send(SimEvent::Term(TermEvent::Resize(cols, rows)));
        self
    }

    /// The same palette another theme would have answered with (sim derives it all again).
    pub fn retheme(&mut self, theme: &Theme) -> &mut Headless {
        for e in theme.events() {
            self.session.send(e);
        }
        self
    }

    #[allow(clippy::expect_used)] // `Sim::new` renders a screen before it returns
    pub fn screen(&self) -> &Screen {
        self.session.screen().expect("sim has drawn a screen")
    }

    /// The text of the screen, one string for each row.
    pub fn text(&self) -> Vec<String> {
        self.screen().frame.rows.iter().map(|r| r.text.clone()).collect()
    }
}

impl Drop for Headless {
    fn drop(&mut self) {
        self.session.finish(); // (the directories go with `owned`, after the shell is done with them)
    }
}

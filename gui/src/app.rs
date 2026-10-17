//! GUI-5, GUI-10, GUI-13
//! The window: winit's event loop, a wgpu surface, and the pieces of this crate put together. The
//! events of the window go through `input` into sim; sim's screens go through the animator into the
//! renderer. The editor is sim's `Shell` with its jobs on threads, as the terminal binary has it.
//!
//! Not run by the tests (nothing here may open a window on a desktop that is not the user's to give);
//! what it is built from is: `input`, `anim`, `scene`, `gpu`, `session`.
use crate::anim::{Animator, reduced_from_env};
use crate::api::*;
use crate::font::{FontBook, FontConfig};
use crate::frame::Snapshot;
use crate::gpu::{self, Gpu, Renderer};
use crate::input::{Action, Input, KeyIn, to_sim};
use crate::overlay::{cursor_layer, preedit_layer};
use crate::scene::Plane;
use crate::session::{Clipboard, Exec, Session, Settings};
use crate::studio::metrics_of;
use crate::theme::Theme;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::Key;
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::{Theme as WinTheme, Window, WindowId};

pub enum UserEvent {
    Sim(SimEvent),
}

/// The system clipboard through the tools each system has (`wl-copy` and `wl-paste`, `xclip`,
/// `pbcopy` and `pbpaste`), run with a short timeout, so that no clipboard owner can hold the window.
/// A native one (smithay-clipboard, arboard) is the next step (NOTES.md).
pub struct SystemClipboard;

fn tools() -> (&'static [&'static str], &'static [&'static str]) {
    if cfg!(target_os = "macos") {
        (&["pbcopy"], &["pbpaste"])
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        (&["wl-copy"], &["wl-paste", "--no-newline"])
    } else {
        (
            &["xclip", "-selection", "clipboard", "-in"],
            &["xclip", "-selection", "clipboard", "-out"],
        )
    }
}

/// Runs a command with `input` on its stdin; its stdout, if it ended in time and well.
fn run(cmd: &[&str], input: Option<&[u8]>, wait: Duration) -> Option<Vec<u8>> {
    let (prog, args) = cmd.split_first()?;
    let mut child = Command::new(prog)
        .args(args)
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let (Some(bytes), Some(mut stdin)) = (input, child.stdin.take()) {
        use std::io::Write;
        let _ = stdin.write_all(bytes);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || drop(tx.send(child.wait_with_output())));
    rx.recv_timeout(wait).ok()?.ok().filter(|o| o.status.success()).map(|o| o.stdout)
}

impl Clipboard for SystemClipboard {
    fn set(&mut self, text: &str) {
        let _ = run(tools().0, Some(text.as_bytes()), Duration::from_millis(500));
    }

    fn get(&mut self) -> Option<String> {
        run(tools().1, None, Duration::from_millis(500)).and_then(|b| String::from_utf8(b).ok())
    }
}

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    gpu: Gpu,
    renderer: Renderer,
    fonts: FontBook,
    input: Input,
    anim: Animator,
    theme: Theme,
    auto_theme: bool,
    last: Instant,
    /// sim drew a screen the animator has not taken yet.
    dirty: bool,
    /// `session.renders` at the last redraw.
    seen: u64,
    session: Session,
    resolver: Resolver,
    /// Whether the input method is on (GUI-5).
    ime_on: bool,
}

pub struct App {
    args: Vec<String>,
    proxy: EventLoopProxy<UserEvent>,
    state: Option<State>,
    pub error: Option<String>,
}

/// Opens the window and runs until sim quits. `args` are sim's: `[+cmd] [dir | file]`.
pub fn run_window(args: Vec<String>) -> Result<(), String> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|e| format!("no window system: {e}"))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App { args, proxy: event_loop.create_proxy(), state: None, error: None };
    event_loop.run_app(&mut app).map_err(|e| format!("event loop: {e}"))?;
    match app.error.take() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

impl App {
    fn init(&mut self, el: &ActiveEventLoop) -> Result<State, String> {
        let attrs = Window::default_attributes()
            .with_title("sim")
            .with_inner_size(LogicalSize::new(1280.0, 820.0));
        let window =
            Arc::new(el.create_window(attrs).map_err(|e| format!("cannot open a window: {e}"))?);
        window.set_ime_allowed(false); // (GUI-5: on where a key is text, `sync_ime`)
        let scale = window.scale_factor();
        let instance = gpu::instance();
        let surface =
            instance.create_surface(window.clone()).map_err(|e| format!("no surface: {e}"))?;
        let hint = std::env::var("SIM_GUI_ADAPTER").ok();
        let adapter = gpu::pick(&instance, hint.as_deref(), Some(&surface))?;
        let (device, queue) = gpu::device(&adapter)?;
        let gpu = Gpu { instance, adapter, device, queue };
        let caps = surface.get_capabilities(&gpu.adapter);
        // not an sRGB format: sim's colors are blended as the numbers they are, as a terminal does
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .or(caps.formats.first().copied())
            .ok_or("the surface has no format")?;
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&gpu.device, &config);
        let renderer = Renderer::new(&gpu, format, 2048);
        let fonts = FontBook::new(&FontConfig::from_env(), scale as f32, 2048);
        let m = metrics_of(&fonts);
        let grid = m.grid_for(config.width, config.height);

        let home = std::env::var_os("HOME");
        let state_dir = (std::env::var("SIM_GUI_NO_STATE").is_err())
            .then(|| journal::state_dir(std::env::var_os("XDG_STATE_HOME"), home.clone()))
            .flatten();
        // STATE-1: one state directory, `sim`: the journal and the history are in it
        let history = state_dir.clone();
        let cfg = Settings {
            state: state_dir,
            history,
            home: home.map(Into::into),
            icons: fonts.has_icons && std::env::var("SIM_GUI_ICONS").map_or(true, |v| v != "0"),
            size: grid,
        };
        let system_light = window.theme().map(|t| t == WinTheme::Light);
        let theme = Theme::from_env(system_light);
        let auto_theme = !matches!(std::env::var("SIM_GUI_THEME").as_deref(), Ok("light" | "dark"));
        let cwd = std::env::current_dir().map_err(|e| format!("no working directory: {e}"))?;
        let Some((mut session, rx)) = Session::new(&self.args, &cwd, &cfg, &theme, Exec::Threads)
        else {
            return Err("usage: sim-gui [+cmd] [dir | file]".into());
        };
        session.clipboard = Box::new(SystemClipboard);
        // the threads' answers arrive in the event loop; a tick every second (SAFE-6)
        if let Some(rx) = rx {
            let p = self.proxy.clone();
            std::thread::spawn(move || {
                for ev in rx {
                    if p.send_event(UserEvent::Sim(ev)).is_err() {
                        break;
                    }
                }
            });
        }
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            for n in 1..=u64::MAX {
                std::thread::sleep(Duration::from_secs(1));
                if p.send_event(UserEvent::Sim(SimEvent::Tick(n * 1000))).is_err() {
                    break;
                }
            }
        });
        eprintln!(
            "sim-gui: {} on {}, font {}",
            env!("CARGO_PKG_VERSION"),
            gpu.describe(),
            fonts.family_name
        );
        let input = Input::new(m, grid);
        window.request_redraw();
        Ok(State {
            window,
            surface,
            config,
            gpu,
            renderer,
            fonts,
            input,
            anim: Animator::new(reduced_from_env()),
            theme,
            auto_theme,
            last: Instant::now(),
            dirty: true,
            seen: 0,
            session,
            resolver: Resolver::new(),
            ime_on: false,
        })
    }
}

impl State {
    fn m(&self) -> crate::metrics::Metrics {
        metrics_of(&self.fonts)
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.gpu.device, &self.config);
        let grid = self.m().grid_for(size.width, size.height);
        self.input.grid = grid;
        self.session.send(SimEvent::Term(TermEvent::Resize(grid.0, grid.1)));
        self.dirty = true;
        self.window.request_redraw();
    }

    /// Everything the input adapter asks for, done: events to sim, the clipboard's text.
    fn act(&mut self, actions: Vec<Action>) {
        if actions.is_empty() {
            return;
        }
        for a in actions {
            match a {
                Action::Send(ev) => {
                    for e in to_sim(ev) {
                        self.session.send(e);
                    }
                }
                Action::Paste => {
                    if let Some(text) = self.session.clipboard.get() {
                        for e in to_sim(CtEvent::Paste(text)) {
                            self.session.send(e);
                        }
                    }
                }
            }
        }
        self.dirty = true;
        self.window.request_redraw();
    }

    fn redraw(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f32().min(0.1);
        self.last = now;
        let m = self.m();
        if self.dirty {
            if let Some(s) = self.session.screen() {
                self.anim.set_target(Snapshot::take(s, &mut self.resolver), &m);
            }
            self.dirty = false;
            self.sync_ime();
        }
        let moving = self.anim.tick(dt, &m);
        let size = (self.config.width, self.config.height);
        let Some(scene) = self.anim.scene(m, size) else { return };
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.gpu.device, &self.config);
                self.window.request_redraw();
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut overlay = Plane::default();
        if let (Some(spec), Some(snap)) = (&scene.cursor, self.anim.snapshot()) {
            let (rect, block) = self.anim.cursor_now(spec, &m);
            let light = scene.floor[0] > 0.5;
            match &self.input.preedit {
                Some(p) => {
                    let (layer, width) =
                        preedit_layer(&p.text, (spec.rect.x, spec.rect.y), &snap.tones, &m, light);
                    overlay = layer;
                    self.window.set_ime_cursor_area(
                        PhysicalPosition::new(spec.rect.x as i32, spec.rect.y as i32),
                        PhysicalSize::new(width.max(1.0) as u32, m.cell_h as u32),
                    );
                }
                None => {
                    overlay = cursor_layer(spec, rect, block, &m);
                    self.window.set_ime_cursor_area(
                        PhysicalPosition::new(rect.x as i32, rect.y as i32),
                        PhysicalSize::new(rect.w.max(1.0) as u32, m.cell_h as u32),
                    );
                }
            }
        }
        self.renderer.render(&view, size, &scene, &overlay, &mut self.fonts);
        self.window.pre_present_notify();
        self.gpu.queue.present(frame);
        if moving || self.anim.moving(&m) {
            self.window.request_redraw();
        }
    }

    /// GUI-5: the input method is on where the next key is text (Insert mode, the command line, the
    /// picker's query), and off where a letter is a command: composing then would turn `j` into a word.
    fn sync_ime(&mut self) {
        let want = self.session.sim.wants_text();
        if want != self.ime_on {
            self.window.set_ime_allowed(want);
            self.ime_on = want;
        }
    }

    fn quit_if_done(&mut self, el: &ActiveEventLoop) {
        if self.session.quit.is_some() {
            self.session.finish();
            el.exit();
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        match self.init(el) {
            Ok(s) => self.state = Some(s),
            Err(e) => {
                self.error = Some(e);
                el.exit();
            }
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, ev: UserEvent) {
        let Some(st) = &mut self.state else { return };
        let UserEvent::Sim(e) = ev;
        st.session.send(e);
        // a tick that draws nothing (most of them) is not a frame
        if st.session.renders != st.seen {
            st.seen = st.session.renders;
            st.dirty = true;
            st.window.request_redraw();
        }
        st.quit_if_done(el);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        let Some(st) = &mut self.state else { return };
        match ev {
            WindowEvent::CloseRequested => {
                // as a hangup: what is unsaved goes to the journal, to be found again (SAFE-6)
                st.session.send(SimEvent::Signal(1));
                st.quit_if_done(el);
                if st.session.quit.is_none() {
                    st.session.finish();
                    el.exit();
                }
            }
            WindowEvent::Resized(size) => st.resize(size),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                st.fonts = FontBook::new(&FontConfig::from_env(), scale_factor as f32, 2048);
                let m = st.m();
                st.input.metrics = m;
                let size = st.window.inner_size();
                st.resize(size);
            }
            WindowEvent::ThemeChanged(t) => {
                if st.auto_theme {
                    st.theme = Theme::from_env(Some(t == WinTheme::Light));
                    for e in st.theme.events() {
                        st.session.send(e);
                    }
                    st.dirty = true;
                    st.window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(m) => st.input.mods = m.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                let bare: Key = event.key_without_modifiers();
                let actions = st.input.key(KeyIn {
                    key: &event.logical_key,
                    bare: Some(&bare),
                    text: event.text.as_deref(),
                    state: if event.state == ElementState::Pressed {
                        ElementState::Pressed
                    } else {
                        ElementState::Released
                    },
                });
                st.act(actions);
                st.quit_if_done(el);
            }
            WindowEvent::Ime(ime) => {
                let actions = st.input.ime(&ime);
                st.act(actions);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let actions = st.input.pointer(position.x, position.y);
                st.act(actions);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let actions = st.input.button(state, button);
                st.act(actions);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let actions = st.input.wheel(delta);
                st.act(actions);
            }
            WindowEvent::RedrawRequested => st.redraw(),
            _ => {}
        }
    }
}

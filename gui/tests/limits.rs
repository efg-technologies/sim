//! GUI-17
//! What the window may be made of and may do: a closed list of direct dependencies, no code that
//! speaks to a network, no window opened by anything but the binary, shaders that cannot hang the GPU
//! (no loops, no compute), and a terminal editor and core that never name a window. `scripts/check.sh`
//! holds the same lists against what cargo resolved (and the whole tree for a network crate).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers; the tests themselves may unwrap
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            files(&p, ext, out);
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push(p);
        }
    }
}

/// The lines of a `[section]` of a manifest, up to the next one.
fn section(toml: &str, name: &str) -> Vec<String> {
    let head = format!("[{name}]");
    toml.lines()
        .skip_while(|l| l.trim() != head)
        .skip(1)
        .take_while(|l| !l.starts_with('['))
        .map(str::to_string)
        .collect()
}

fn deps_of(toml: &str) -> Vec<(String, String)> {
    section(toml, "dependencies")
        .iter()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

#[test]
fn gui_17_the_direct_dependencies_are_exactly_the_allowlist() {
    let toml = read(&root().join("Cargo.toml"));
    let mut names: Vec<_> = deps_of(&toml).into_iter().map(|(k, _)| k).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "bytemuck",
            "cosmic-text",
            "crossterm",
            "png",
            "pollster",
            "sim",
            "sim_core",
            "wgpu",
            "winit"
        ]
    );
    // and nothing in the dev-dependencies either: the tests use the same crates
    assert!(
        section(&toml, "dev-dependencies")
            .iter()
            .all(|l| l.trim().is_empty() || l.starts_with('#'))
    );
}

#[test]
fn gui_17_the_gpu_is_vulkan_and_metal_with_wgsl_and_the_window_is_x11_and_wayland() {
    let deps = deps_of(&read(&root().join("Cargo.toml")));
    let feats = |name: &str| -> Vec<String> {
        let v = &deps.iter().find(|(k, _)| k == name).unwrap().1;
        let list = v.split("features = [").nth(1).unwrap_or("");
        let list = list.split(']').next().unwrap_or("");
        list.split('"').skip(1).step_by(2).map(str::to_string).collect()
    };
    let mut wgpu = feats("wgpu");
    wgpu.sort_unstable();
    assert_eq!(
        wgpu,
        ["metal", "parking_lot", "std", "vulkan", "wgsl"],
        "no GLES, no DX12, no WebGPU"
    );
    let mut winit = feats("winit");
    winit.sort_unstable();
    assert_eq!(winit, ["rwh_06", "wayland", "wayland-dlopen", "x11"]);
    assert_eq!(
        feats("crossterm"),
        ["events", "bracketed-paste"],
        "event types only: no terminal is read"
    );
    for (k, v) in &deps {
        assert!(!v.contains("git ="), "{k}: a dependency from a git repository");
    }
}

#[test]
fn gui_17_the_crate_is_not_published_and_forbids_unsafe_code() {
    let toml = read(&root().join("Cargo.toml"));
    assert!(section(&toml, "package").iter().any(|l| l.trim() == "publish = false"));
    assert!(section(&toml, "package").iter().any(|l| l.contains("MIT OR Apache-2.0")));
    let lints = section(&toml, "lints.rust");
    assert!(lints.iter().any(|l| l.replace(' ', "") == "unsafe_code=\"forbid\""));
    let lib = read(&root().join("src/lib.rs"));
    assert!(lib.contains("#![forbid(unsafe_code)]"));
}

#[test]
fn gui_17_no_source_speaks_to_a_network() {
    let mut all = vec![];
    files(&root().join("src"), "rs", &mut all);
    assert!(all.len() > 15);
    for p in all {
        let s = read(&p);
        for word in
            ["std::net", "TcpStream", "TcpListener", "UdpSocket", "reqwest", "hyper::", "ureq::"]
        {
            assert!(!s.contains(word), "{} names {word}", p.display());
        }
    }
}

#[test]
fn gui_17_only_the_window_module_opens_a_window() {
    let mut all = vec![];
    files(&root().join("src"), "rs", &mut all);
    for p in all {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let s = read(&p);
        let opens = s.contains("create_window") || s.contains("EventLoop::new");
        assert_eq!(opens, name == "app.rs", "{}: opens a window: {opens}", p.display());
    }
    // the tools and the tests draw offscreen
    let shots = read(&root().join("src/bin/gui-shots.rs"));
    assert!(!shots.contains("winit") && !shots.contains("create_surface"));
}

#[test]
fn gui_17_the_shaders_have_no_loops_and_no_compute_stage() {
    let mut all = vec![];
    files(&root().join("src/shaders"), "wgsl", &mut all);
    assert_eq!(all.len(), 3, "common, surface and glyph");
    for p in all {
        let s = read(&p);
        let code: String =
            s.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
        for word in
            ["loop", "for ", "for(", "while", "@compute", "workgroup", "atomic", "storageBarrier"]
        {
            assert!(!code.contains(word), "{} has `{word}`", p.display());
        }
    }
}

#[test]
fn gui_17_the_editor_and_the_core_never_name_a_window() {
    let up = root().join("..");
    for krate in ["core", "sim"] {
        let toml = read(&up.join(krate).join("Cargo.toml"));
        for word in ["winit", "wgpu", "cosmic-text", "sim-gui", "bytemuck", "pollster"] {
            assert!(!toml.contains(word), "{krate}/Cargo.toml names {word}");
        }
        let mut src = vec![];
        files(&up.join(krate).join("src"), "rs", &mut src);
        for p in src {
            let s = read(&p);
            for word in ["winit", "wgpu::", "cosmic_text", "sim_gui"] {
                assert!(!s.contains(word), "{} names {word}", p.display());
            }
        }
    }
    // the workspace has the window as a member and as no one's dependency
    let ws = read(&up.join("Cargo.toml"));
    assert!(ws.contains("\"gui\""));
}

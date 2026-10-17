//! GUI-15
//! The paths this crate makes in the shared temporary directory are not ones another local user can
//! prepare: the demo project and the journal of the offscreen renders are in directories with a name
//! made of chance, made by `mkdir` at mode 0700; the shots tool has no default output directory and
//! writes only into one that is the user's and that others cannot write to, through new files.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use sim_gui::headless::Headless;
use sim_gui::offscreen::write_png;
use sim_gui::safe_io::{PrivateDir, check_private};
use sim_gui::theme::DARK;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::Path;
use std::process::Command;

fn tool(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_gui-shots")).args(args).output().unwrap()
}

#[test]
fn gui_15_the_demo_project_is_in_a_private_directory_with_a_random_name() {
    let (a, b) = (
        Headless::demo("p1", &["README.md"], (80, 24), &DARK),
        Headless::demo("p2", &["README.md"], (80, 24), &DARK),
    );
    for h in [&a, &b] {
        let home = h.dir.parent().unwrap();
        check_private(home).unwrap(); // a real directory, ours, mode 0700
        let name = home.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            !name.contains(&std::process::id().to_string()),
            "the pid is not in the name: {name}"
        );
        assert!(!name.contains("p1") && !name.contains("p2"));
    }
    assert_ne!(a.dir, b.dir);
}

#[test]
fn gui_15_a_link_planted_at_the_old_predictable_names_is_not_followed() {
    // what the first version made: sim-gui-demo-{pid}-{n}-{name} and sim-gui-state-{pid}-{n}
    let victim = PrivateDir::new("sim-gui-victim-").unwrap();
    let tmp = std::env::temp_dir();
    let pid = std::process::id();
    let mut planted = vec![];
    for n in 0..40 {
        for name in [format!("sim-gui-demo-{pid}-{n}-planted"), format!("sim-gui-state-{pid}-{n}")]
        {
            let link = tmp.join(name);
            if symlink(victim.path(), &link).is_ok() {
                planted.push(link);
            }
        }
    }
    let h = Headless::demo("planted", &["README.md"], (80, 24), &DARK);
    assert!(!h.dir.starts_with(victim.path()));
    drop(h);
    let left = fs::read_dir(victim.path()).unwrap().count();
    for l in &planted {
        let _ = fs::remove_file(l);
    }
    assert_eq!(left, 0, "nothing was made, removed or written through a planted link");
}

#[test]
fn gui_15_the_shots_tool_has_no_default_output_directory() {
    let out = tool(&[]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("OUT_DIR") && err.contains("required"), "{err}");
    // options alone are not a directory either
    assert_eq!(tool(&["--time"]).status.code(), Some(2));
}

#[test]
fn gui_15_the_shots_tool_refuses_an_output_directory_others_can_write_to() {
    let parent = PrivateDir::new("sim-gui-out-").unwrap();
    let open = parent.path().join("open");
    fs::DirBuilder::new().mode(0o777).create(&open).unwrap();
    fs::set_permissions(&open, fs::Permissions::from_mode(0o777)).unwrap();
    let out = tool(&[open.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stderr).contains("other users can write"));
    assert_eq!(fs::read_dir(&open).unwrap().count(), 0, "it wrote nothing there");
}

#[test]
fn gui_15_a_png_is_written_to_a_new_file_and_a_link_at_its_target_is_replaced() {
    let d = PrivateDir::new("sim-gui-png-").unwrap();
    let victim = d.path().join("victim");
    fs::write(&victim, "precious").unwrap();
    let target = d.path().join("gui-x.png");
    symlink(&victim, &target).unwrap();
    write_png(&target, (2, 1), &[255, 0, 0, 255, 0, 255, 0, 255]).unwrap();
    assert_eq!(fs::read_to_string(&victim).unwrap(), "precious", "not written through the link");
    let md = fs::symlink_metadata(&target).unwrap();
    assert!(md.file_type().is_file());
    let ((w, h), px) = sim_gui::offscreen::read_png(Path::new(&target)).unwrap();
    assert_eq!((w, h), (2, 1));
    assert_eq!(&px[..4], &[255, 0, 0, 255]);
}

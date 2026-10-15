//! PROP-5, PROP-6
//! Running `sh -c` in its own process group, and cancelling the whole group.
use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread::{Builder, JoinHandle};

pub struct Output {
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

fn bg<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> io::Result<JoinHandle<T>> {
    Builder::new().spawn(f)
}

// Everything until EOF; what was read before an error is kept.
fn drain(pipe: Option<impl Read>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(mut p) = pipe {
        let _ = p.read_to_end(&mut buf);
    }
    buf
}

/// Runs `sh -c cmd` in its own process group, writes `input` to its stdin from a separate thread
/// (so a large input can't deadlock), collects stdout and stderr, and calls `done` once it exits.
/// Returns the process-group id for `cancel`.
pub fn spawn(
    cmd: &str,
    input: Vec<u8>,
    done: impl FnOnce(Output) + Send + 'static,
) -> io::Result<u32> {
    let mut child = Command::new("sh")
        .args(["-c", cmd])
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let pgid = child.id();
    let (stdin, stdout, stderr) = (child.stdin.take(), child.stdout.take(), child.stderr.take());
    let threads = (|| {
        bg(move || stdin.map(|mut s| s.write_all(&input)))?; // EPIPE: it did not want the input
        let err = bg(move || drain(stderr))?;
        bg(move || {
            let stdout = drain(stdout);
            let stderr = err.join().unwrap_or_default();
            let status = child.wait().ok().and_then(|s| s.code()); // also reaps it
            done(Output { status, stdout, stderr });
        })
    })();
    if let Err(e) = threads {
        cancel(pgid); // no threads left to wait for it
        return Err(e);
    }
    Ok(pgid)
}

/// Stops the whole process group (PROP-5 Decided): SIGTERM via `kill -TERM -- -<pgid>`
/// (std::process::Command; no libc), so children such as `claude -p` stop too.
/// Only for a group `spawn` returned whose `done` has not been called: the id may be reused after.
pub fn cancel(pgid: u32) {
    if pgid > 1 {
        // 0 and 1 would mean our own group and every process.
        let mut kill = Command::new("kill");
        let _ = kill
            .args(["-TERM", "--", &format!("-{pgid}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

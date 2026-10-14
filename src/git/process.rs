use std::{
    ffi::OsStr,
    io::{Read, Result as IoResult},
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};

const OUTPUT_LIMIT: usize = 2 * 1024 * 1024;

pub(super) struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub truncated: bool,
}

// Kill the whole process group: hooks may inherit the output pipes.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Ok(pid) = i32::try_from(self.0.id()) {
            let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
        }
        let _ = self.0.wait();
    }
}

fn capture(mut reader: impl Read) -> IoResult<(Vec<u8>, bool)> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Ok((bytes, truncated));
        }
        let retained = count.min(OUTPUT_LIMIT.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..retained]);
        truncated |= retained != count;
        // Keep draining even once capped, otherwise the child can block on write.
    }
}

pub(super) fn run(
    binary: &OsStr,
    directory: &Path,
    args: &[&OsStr],
    timeout: Duration,
) -> Result<Output> {
    let command_name = args
        .first()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let child = Command::new(binary)
        .current_dir(directory)
        .args(["--no-pager", "--literal-pathspecs", "-c", "color.ui=false"])
        .args(args)
        .env("LC_ALL", "C")
        .env("GIT_PAGER", "cat")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .with_context(|| format!("could not start {}", binary.to_string_lossy()))?;
    let mut child = ChildGuard(child);
    let stdout = child.0.stdout.take().context("missing Git stdout")?;
    let stderr = child.0.stderr.take().context("missing Git stderr")?;
    let out_reader = thread::spawn(move || capture(stdout));
    let err_reader = thread::spawn(move || capture(stderr));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().context("could not wait for Git")? {
            break status;
        }
        if started.elapsed() >= timeout {
            drop(child);
            let _ = out_reader.join();
            let _ = err_reader.join();
            bail!(
                "git {command_name} timed out after {}s; inspect repository state before retrying",
                timeout.as_secs()
            );
        }
        thread::sleep(Duration::from_millis(10));
    };
    // Reap stray hook descendants before joining pipe readers.
    drop(child);
    let (stdout, out_truncated) = out_reader
        .join()
        .map_err(|_| anyhow::anyhow!("Git stdout reader panicked"))??;
    let (stderr, err_truncated) = err_reader
        .join()
        .map_err(|_| anyhow::anyhow!("Git stderr reader panicked"))??;
    Ok(Output {
        status,
        stdout,
        stderr,
        truncated: out_truncated || err_truncated,
    })
}

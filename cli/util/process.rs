//! Bounded, synchronous external commands. Pipes are drained concurrently;
//! Unix process groups are terminated before joining readers or returning.
use anyhow::{Context, Error, bail};
use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

pub(crate) struct Limits {
    pub timeout: Duration,
    pub output_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            output_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Interrupted;
impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("external command interrupted")
    }
}
impl std::error::Error for Interrupted {}

pub(crate) fn run(command: &mut Command, limits: Limits) -> Result<String, Error> {
    let name = command.get_program().to_string_lossy().into_owned();
    let signals = Signals::new()?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = Running(
        command
            .spawn()
            .with_context(|| format!("unable to start {name}"))?,
        false,
    );
    let stdout = child.0.stdout.take().context("missing stdout pipe")?;
    let stderr = child.0.stderr.take().context("missing stderr pipe")?;
    let consumed = AtomicUsize::new(0);
    let exceeded = AtomicBool::new(false);
    let started = Instant::now();
    std::thread::scope(|scope| {
        let out = scope.spawn(|| capture(stdout, &consumed, &exceeded, limits.output_bytes));
        let err = scope.spawn(|| capture(stderr, &consumed, &exceeded, limits.output_bytes));
        let outcome = loop {
            if signals.interrupted() {
                break Err(Interrupted.into());
            }
            if exceeded.load(Ordering::Relaxed) {
                break Err(anyhow::anyhow!(
                    "{name} exceeded its {} byte output limit",
                    limits.output_bytes
                ));
            }
            if started.elapsed() >= limits.timeout {
                break Err(anyhow::anyhow!(
                    "{name} exceeded its {:?} deadline",
                    limits.timeout
                ));
            }
            match child.0.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => break Err(error.into()),
            }
        };
        // Descendants may keep stdout/stderr open after the direct child exits.
        child.terminate();
        let stdout = out
            .join()
            .map_err(|_| anyhow::anyhow!("stdout reader panicked"))??;
        let stderr = err
            .join()
            .map_err(|_| anyhow::anyhow!("stderr reader panicked"))??;
        let status = outcome?;
        if exceeded.load(Ordering::Relaxed) {
            bail!(
                "{name} exceeded its {} byte output limit",
                limits.output_bytes
            );
        }
        if !status.success() {
            bail!(
                "{name} failed ({status}):\n{}\n{}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
        }
        String::from_utf8(stdout).with_context(|| format!("{name} returned non-UTF-8 output"))
    })
}

fn capture(
    mut reader: impl Read,
    consumed: &AtomicUsize,
    exceeded: &AtomicBool,
    limit: usize,
) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let previous = consumed.fetch_add(count, Ordering::Relaxed);
        let retained = count.min(limit.saturating_sub(previous));
        output.extend_from_slice(&buffer[..retained]);
        if retained != count {
            exceeded.store(true, Ordering::Relaxed);
            break;
        }
    }
    Ok(output)
}

struct Running(Child, bool);
impl Running {
    fn terminate(&mut self) {
        if self.1 {
            return;
        }
        self.1 = true;
        #[cfg(unix)]
        // SAFETY: spawn created a new group whose ID is this still-owned child ID.
        // No shell interpolation or unrelated process-group target is involved.
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.terminate();
    }
}

// Temporarily intercept cancellation while owning a child process group. Restore
// previous dispositions before returning, including Tokio's if one was installed.
#[cfg(unix)]
static SIGNAL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[cfg(unix)]
static CANCELLED: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
extern "C" fn cancel(_: libc::c_int) {
    CANCELLED.store(true, Ordering::Relaxed);
}

struct Signals {
    #[cfg(unix)]
    previous: Vec<(libc::c_int, libc::sigaction)>,
    #[cfg(unix)]
    _lock: std::sync::MutexGuard<'static, ()>,
}
impl Signals {
    fn new() -> Result<Self, Error> {
        #[cfg(unix)]
        {
            let lock = SIGNAL_LOCK
                .lock()
                .map_err(|_| anyhow::anyhow!("command signal lock poisoned"))?;
            CANCELLED.store(false, Ordering::Relaxed);
            let mut guard = Self {
                previous: Vec::new(),
                _lock: lock,
            };
            for signal in [libc::SIGINT, libc::SIGTERM] {
                // SAFETY: fully initialized signal structures; handler performs
                // only a lock-free atomic store. Guard serializes registrations.
                unsafe {
                    let mut action: libc::sigaction = std::mem::zeroed();
                    let mut previous: libc::sigaction = std::mem::zeroed();
                    action.sa_sigaction = cancel as *const () as usize;
                    libc::sigemptyset(&mut action.sa_mask);
                    if libc::sigaction(signal, &action, &mut previous) != 0 {
                        return Err(std::io::Error::last_os_error().into());
                    }
                    guard.previous.push((signal, previous));
                }
            }
            Ok(guard)
        }
        #[cfg(not(unix))]
        {
            bail!("bounded paper execution currently requires Unix process-group support")
        }
    }
    fn interrupted(&self) -> bool {
        #[cfg(unix)]
        {
            CANCELLED.load(Ordering::Relaxed)
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        #[cfg(unix)]
        for (signal, previous) in self.previous.iter().rev() {
            // SAFETY: restore the disposition saved while holding SIGNAL_LOCK.
            unsafe {
                libc::sigaction(*signal, previous, std::ptr::null_mut());
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn shell(script: &str, timeout: Duration, output_bytes: usize) -> Result<String, Error> {
        run(
            Command::new("/bin/sh").args(["-c", script]),
            Limits {
                timeout,
                output_bytes,
            },
        )
    }
    #[test]
    fn captures_success_and_reports_failures() {
        assert_eq!(
            shell("printf hello", Duration::from_secs(2), 100).unwrap(),
            "hello"
        );
        assert!(
            shell("printf diagnostic >&2; exit 1", Duration::from_secs(2), 100)
                .unwrap_err()
                .to_string()
                .contains("diagnostic")
        );
    }
    #[test]
    fn deadlines_and_descendant_pipe_cleanup_are_bounded() {
        let start = Instant::now();
        assert!(
            shell("sleep 20", Duration::from_millis(100), 100)
                .unwrap_err()
                .to_string()
                .contains("deadline")
        );
        assert!(start.elapsed() < Duration::from_secs(3));
        let start = Instant::now();
        shell("sleep 20 & exit 0", Duration::from_secs(2), 100).unwrap();
        assert!(start.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn output_floods_and_interrupts_stop_children() {
        assert!(
            shell(
                "while :; do printf '0123456789'; done",
                Duration::from_secs(2),
                100
            )
            .unwrap_err()
            .to_string()
            .contains("output limit")
        );
        assert!(
            shell("kill -INT $PPID; sleep 20", Duration::from_secs(2), 100)
                .unwrap_err()
                .is::<Interrupted>()
        );
    }
}

//! The process I/O engine of [`CommandBuilder`](super::CommandBuilder).
//!
//! All run methods use [`execute`]. A thread writes stdin. A thread reads
//! each piped stream while the parent waits. Thus a full pipe never stops
//! the child, and a deadline can stop the wait.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, PipeReader, PipeWriter, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

/// The number of stderr bytes that a quiet `run` keeps for its error.
pub(super) const TAIL_BYTES: usize = 4096;

/// The time to wait for open streams after the child ends or times out.
const GRACE: Duration = Duration::from_millis(200);

/// The longest pause between two checks of a child with a timeout.
const MAX_POLL: Duration = Duration::from_millis(20);

/// The I/O options of a command.
#[derive(Default)]
pub(super) struct Options {
    pub(super) stdin: Option<Vec<u8>>,
    pub(super) quiet: bool,
    pub(super) tee: bool,
    pub(super) timeout: Option<Duration>,
    pub(super) stdout: StdoutTarget,
    pub(super) stderr_to_stdout: bool,
    pub(super) log_stdout: Option<PathBuf>,
    pub(super) log_stderr: Option<PathBuf>,
}

impl Options {
    /// Returns `true` when an option other than `quiet` is set.
    pub(super) const fn needs_engine(&self) -> bool {
        self.stdin.is_some()
            || self.tee
            || self.timeout.is_some()
            || !matches!(self.stdout, StdoutTarget::Terminal)
            || self.stderr_to_stdout
            || self.log_stdout.is_some()
            || self.log_stderr.is_some()
    }
}

impl fmt::Debug for Options {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Options")
            .field("stdin_bytes", &self.stdin.as_ref().map(Vec::len))
            .field("quiet", &self.quiet)
            .field("tee", &self.tee)
            .field("timeout", &self.timeout)
            .field("stdout", &self.stdout)
            .field("stderr_to_stdout", &self.stderr_to_stdout)
            .field("log_stdout", &self.log_stdout)
            .field("log_stderr", &self.log_stderr)
            .finish()
    }
}

/// Where stdout goes before the run method applies its default.
#[derive(Debug, Default)]
pub(super) enum StdoutTarget {
    /// The default of the run method.
    #[default]
    Terminal,
    /// A file (`>` or `>>`).
    File(Target),
    /// The place of stderr (`>&2`).
    Stderr,
}

/// A file that gets output.
#[derive(Debug, Clone)]
pub(super) struct Target {
    pub(super) path: PathBuf,
    pub(super) append: bool,
}

/// The defaults of a run method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// `run` and `status`: the terminal gets the output and gives stdin.
    Run,
    /// `success`: no output and no stdin.
    Silent,
    /// `output`, `output_lines`, `capture` and `output_raw`: smop keeps the
    /// output. Stdin is empty.
    Capture,
}

/// A stream of the terminal of this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Term {
    Stdout,
    Stderr,
}

/// How much of a stream smop keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keep {
    Nothing,
    All,
    /// The last [`TAIL_BYTES`] bytes.
    Tail,
}

/// The places that get one stream.
#[derive(Debug)]
struct Sinks {
    terminal: Option<Term>,
    keep: Keep,
    files: Vec<Target>,
}

impl Sinks {
    /// Gets the sinks of a stream for `mode` with no options.
    const fn base(mode: Mode, term: Term) -> Self {
        let (terminal, keep) = match mode {
            Mode::Run => (Some(term), Keep::Nothing),
            Mode::Silent => (None, Keep::Nothing),
            Mode::Capture => (None, Keep::All),
        };
        Self {
            terminal,
            keep,
            files: Vec::new(),
        }
    }
}

/// Which stream follows the other one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Merge {
    None,
    /// `2>&1`.
    StderrToStdout,
    /// `>&2`.
    StdoutToStderr,
}

/// The sinks of both streams after all options.
#[derive(Debug)]
struct Plan {
    stdout: Sinks,
    stderr: Sinks,
    merge: Merge,
}

/// Applies the options to the defaults of `mode`.
fn plan(opts: &Options, mode: Mode) -> Result<Plan> {
    let merge = if opts.stderr_to_stdout {
        Merge::StderrToStdout
    } else if matches!(opts.stdout, StdoutTarget::Stderr) {
        Merge::StdoutToStderr
    } else {
        Merge::None
    };
    if merge == Merge::StderrToStdout && opts.log_stderr.is_some() {
        bail!("log_stderr has no effect with stderr_to_stdout. Use log_stdout.");
    }
    if merge == Merge::StdoutToStderr && opts.log_stdout.is_some() {
        bail!("log_stdout has no effect with stdout_to_stderr. Use log_stderr.");
    }

    let mut stdout = Sinks::base(mode, Term::Stdout);
    let mut stderr = Sinks::base(mode, Term::Stderr);
    if opts.tee && mode == Mode::Capture {
        stdout.terminal = Some(Term::Stdout);
        stderr.terminal = Some(Term::Stderr);
    }
    if let StdoutTarget::File(target) = &opts.stdout {
        stdout = Sinks {
            terminal: None,
            keep: Keep::Nothing,
            files: vec![target.clone()],
        };
    }
    let log = |path: &PathBuf| Target {
        path: path.clone(),
        append: false,
    };
    stdout.files.extend(opts.log_stdout.iter().map(log));
    stderr.files.extend(opts.log_stderr.iter().map(log));
    if opts.quiet {
        stdout.terminal = None;
        stderr.terminal = None;
        if mode == Mode::Run {
            // Keep the end of stderr for the error of a failed run.
            let holder = match merge {
                Merge::StderrToStdout => &mut stdout,
                Merge::None | Merge::StdoutToStderr => &mut stderr,
            };
            if holder.keep == Keep::Nothing {
                holder.keep = Keep::Tail;
            }
        }
    }
    Ok(Plan {
        stdout,
        stderr,
        merge,
    })
}

/// The shared buffer that a reader thread fills.
type Buffer = Arc<Mutex<Vec<u8>>>;

/// The child side of one stream, before the spawn.
enum Wire {
    Inherit(Term),
    Null,
    File(File),
    Pipe(PipeWriter),
}

impl Wire {
    /// Gets the `Stdio` for the stream `own`.
    fn stdio(&self, own: Term) -> io::Result<Stdio> {
        Ok(match self {
            Self::Inherit(term) if *term == own => Stdio::inherit(),
            Self::Inherit(Term::Stdout) => Stdio::from(io::stdout()),
            Self::Inherit(Term::Stderr) => Stdio::from(io::stderr()),
            Self::Null => Stdio::null(),
            Self::File(file) => Stdio::from(file.try_clone()?),
            Self::Pipe(writer) => Stdio::from(writer.try_clone()?),
        })
    }
}

/// The parent side of a piped stream: what the reader thread does.
struct Pump {
    reader: PipeReader,
    terminal: Option<Term>,
    keep: Keep,
    files: Vec<(PathBuf, File)>,
    buffer: Buffer,
}

/// Opens a file target. Opens it before the spawn, as a shell does.
fn open(target: &Target) -> Result<File> {
    let mut options = OpenOptions::new();
    if target.append {
        options.append(true);
    } else {
        options.write(true).truncate(true);
    }
    options
        .create(true)
        .open(&target.path)
        .with_context(|| format!("Failed to open {}", target.path.display()))
}

/// Makes the wire of a stream, and its pump when it needs a pipe.
fn wire(sinks: &Sinks) -> Result<(Wire, Option<Pump>)> {
    match (sinks.keep, sinks.terminal, sinks.files.as_slice()) {
        (Keep::Nothing, Some(term), []) => Ok((Wire::Inherit(term), None)),
        (Keep::Nothing, None, []) => Ok((Wire::Null, None)),
        (Keep::Nothing, None, [target]) => Ok((Wire::File(open(target)?), None)),
        _ => {
            let files = sinks
                .files
                .iter()
                .map(|t| Ok((t.path.clone(), open(t)?)))
                .collect::<Result<_>>()?;
            let (reader, writer) = io::pipe().context("Failed to create a pipe")?;
            let pump = Pump {
                reader,
                terminal: sinks.terminal,
                keep: sinks.keep,
                files,
                buffer: Buffer::default(),
            };
            Ok((Wire::Pipe(writer), Some(pump)))
        }
    }
}

/// What a finished command gives.
#[derive(Debug)]
pub(super) struct Finished {
    pub(super) status: ExitStatus,
    /// `Some` when the timeout stopped the child.
    pub(super) timed_out: Option<Duration>,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
    /// `true` when stderr went into the stdout buffer.
    pub(super) stderr_in_stdout: bool,
    /// An error to read output or to write a log file.
    pub(super) io_error: Option<anyhow::Error>,
}

impl Finished {
    /// Gets the bytes that hold stderr.
    pub(super) fn stderr_bytes(&self) -> &[u8] {
        if self.stderr_in_stdout {
            &self.stdout
        } else {
            &self.stderr
        }
    }
}

/// Runs `command` with `opts` and the defaults of `mode`.
///
/// Returns `Err` only when the command does not start. `spawn_error` makes
/// that error.
pub(super) fn execute(
    mut command: Command,
    opts: Options,
    mode: Mode,
    spawn_error: impl FnOnce(io::Error) -> anyhow::Error,
) -> Result<Finished> {
    let plan = plan(&opts, mode)?;
    let (out_wire, out_pump, err_wire, err_pump) = match plan.merge {
        Merge::None => {
            let (out_wire, out_pump) = wire(&plan.stdout)?;
            let (err_wire, err_pump) = wire(&plan.stderr)?;
            (out_wire, out_pump, err_wire, err_pump)
        }
        Merge::StderrToStdout => {
            let (out_wire, out_pump) = wire(&plan.stdout)?;
            command.stderr(out_wire.stdio(Term::Stderr)?);
            (out_wire, out_pump, Wire::Null, None)
        }
        Merge::StdoutToStderr => {
            let (err_wire, err_pump) = wire(&plan.stderr)?;
            command.stdout(err_wire.stdio(Term::Stdout)?);
            (Wire::Null, None, err_wire, err_pump)
        }
    };
    if plan.merge != Merge::StdoutToStderr {
        command.stdout(out_wire.stdio(Term::Stdout)?);
    }
    if plan.merge != Merge::StderrToStdout {
        command.stderr(err_wire.stdio(Term::Stderr)?);
    }
    command.stdin(match (&opts.stdin, mode) {
        (Some(_), _) => Stdio::piped(),
        (None, Mode::Run) => Stdio::inherit(),
        (None, Mode::Silent | Mode::Capture) => Stdio::null(),
    });

    let spawned = command.spawn();
    // The command and the wires hold the write ends of the pipes. Close
    // them, or the readers never see the end of the stream.
    drop(command);
    drop((out_wire, err_wire));
    let mut child = spawned.map_err(spawn_error)?;

    if let (Some(input), Some(mut stdin)) = (opts.stdin, child.stdin.take()) {
        // A child can close stdin before it reads all. A shell ignores
        // that, so smop ignores write errors too.
        start(&mut child, "stdin", move || {
            let _ = stdin.write_all(&input);
        })?;
    }

    let (done_tx, done_rx) = mpsc::channel();
    let mut buffers = [Buffer::default(), Buffer::default()];
    let mut readers = 0;
    for (slot, pump) in [out_pump, err_pump].into_iter().enumerate() {
        let Some(pump) = pump else { continue };
        buffers[slot] = Arc::clone(&pump.buffer);
        let done = done_tx.clone();
        start(&mut child, "output", move || {
            let _ = done.send(pump.run());
        })?;
        readers += 1;
    }
    drop(done_tx);

    let deadline = opts.timeout.map(|t| Instant::now() + t);
    let (status, timed_out) = wait(&mut child, deadline)?;
    let timed_out = if timed_out { opts.timeout } else { None };
    let cutoff = deadline.map(|d| d.max(Instant::now()) + GRACE);
    let io_error = collect(&done_rx, readers, cutoff);

    let [stdout, stderr] = buffers.map(|b| std::mem::take(&mut *lock(&b)));
    Ok(Finished {
        status,
        timed_out,
        stdout,
        stderr,
        stderr_in_stdout: plan.merge == Merge::StderrToStdout,
        io_error,
    })
}

/// Starts a thread. If it can not start, stops the child.
fn start(child: &mut Child, name: &str, body: impl FnOnce() + Send + 'static) -> Result<()> {
    if let Err(err) = thread::Builder::new()
        .name(format!("smop-sh-{name}"))
        .spawn(body)
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(err).context("Failed to start an I/O thread");
    }
    Ok(())
}

/// Waits for the child. At the deadline, kills it and then reaps it.
///
/// Returns the status and `true` when the deadline stopped the child.
fn wait(child: &mut Child, deadline: Option<Instant>) -> Result<(ExitStatus, bool)> {
    let failed = "Failed to wait for the command";
    let Some(deadline) = deadline else {
        return Ok((child.wait().context(failed)?, false));
    };
    let mut pause = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait().context(failed)? {
            return Ok((status, false));
        }
        let now = Instant::now();
        if now >= deadline {
            // The child can end before the kill. Then `wait` still reaps it.
            let _ = child.kill();
            return Ok((child.wait().context(failed)?, true));
        }
        thread::sleep(pause.min(deadline - now));
        pause = (pause * 2).min(MAX_POLL);
    }
}

/// Waits for `readers` results until `cutoff`. Gets the first error.
///
/// A reader that is not done at the cutoff continues alone. A grandchild
/// that keeps the stream open causes this.
fn collect(
    done: &mpsc::Receiver<Result<()>>,
    readers: usize,
    cutoff: Option<Instant>,
) -> Option<anyhow::Error> {
    let mut first = None;
    for _ in 0..readers {
        let result = cutoff.map_or_else(
            || done.recv().ok(),
            |cutoff| {
                done.recv_timeout(cutoff.saturating_duration_since(Instant::now()))
                    .ok()
            },
        );
        match result {
            None => break,
            Some(Err(err)) if first.is_none() => first = Some(err),
            Some(_) => {}
        }
    }
    first
}

/// Locks a buffer. A reader does not panic while it holds the lock, so a
/// poisoned lock still holds good data.
fn lock(buffer: &Buffer) -> std::sync::MutexGuard<'_, Vec<u8>> {
    buffer.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Pump {
    /// Copies the stream to its sinks until the end of the stream.
    ///
    /// Continues to read after a sink fails, so the child never blocks.
    /// Returns the first error.
    fn run(mut self) -> Result<()> {
        let mut chunk = [0_u8; 8192];
        let mut first: Option<anyhow::Error> = None;
        loop {
            let n = match self.reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => n,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => {
                    first.get_or_insert_with(|| {
                        anyhow::Error::new(err).context("Failed to read output")
                    });
                    break;
                }
            };
            let bytes = &chunk[..n];
            keep(&mut lock(&self.buffer), self.keep, bytes);
            // A closed terminal must not stop the copy to other sinks.
            let _ = match self.terminal {
                Some(Term::Stdout) => write_flush(&mut io::stdout().lock(), bytes),
                Some(Term::Stderr) => write_flush(&mut io::stderr().lock(), bytes),
                None => Ok(()),
            };
            self.files
                .retain_mut(|(path, file)| match file.write_all(bytes) {
                    Ok(()) => true,
                    Err(err) => {
                        let path = path.display();
                        first.get_or_insert_with(|| {
                            anyhow::Error::new(err).context(format!("Failed to write {path}"))
                        });
                        false
                    }
                });
        }
        first.map_or(Ok(()), Err)
    }
}

/// Writes all bytes and flushes.
fn write_flush(out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    out.write_all(bytes)?;
    out.flush()
}

/// Adds `bytes` to `buffer` as `keep` tells.
fn keep(buffer: &mut Vec<u8>, keep: Keep, bytes: &[u8]) {
    match keep {
        Keep::Nothing => {}
        Keep::All => buffer.extend_from_slice(bytes),
        Keep::Tail => {
            buffer.extend_from_slice(bytes);
            // Cut only when the buffer is two times the limit, so the
            // copy cost per byte stays constant. Keep one byte more than
            // the limit, so `tail` sees that the text was cut.
            if buffer.len() > 2 * TAIL_BYTES {
                buffer.drain(..buffer.len() - (TAIL_BYTES + 1));
            }
        }
    }
}

/// Gets the last [`TAIL_BYTES`] bytes of `bytes` as text.
///
/// When it cuts, the text starts after the first line break, so it does not
/// start in a line.
pub(super) fn tail(bytes: &[u8]) -> String {
    let Some(cut) = bytes.len().checked_sub(TAIL_BYTES).filter(|&c| c > 0) else {
        return String::from_utf8_lossy(bytes).into_owned();
    };
    let rest = &bytes[cut..];
    // Do not use the last byte: a line break there would leave no text.
    let start = rest[..rest.len() - 1]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    String::from_utf8_lossy(&rest[start..]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn tail_of_short_text_is_all() {
        assert_eq!(tail(b"a\nb\n"), "a\nb\n");
    }

    #[test]
    fn tail_of_long_text_starts_at_a_line() {
        let mut bytes = vec![b'x'; TAIL_BYTES * 3];
        bytes.extend_from_slice(b"\nlast\n");
        assert_eq!(tail(&bytes), "last\n");
    }

    #[test]
    fn tail_of_one_long_line_keeps_its_end() {
        let mut bytes = vec![b'x'; TAIL_BYTES * 3];
        bytes.push(b'\n');
        let text = tail(&bytes);
        assert_eq!(text.len(), TAIL_BYTES);
        assert!(text.ends_with("x\n"));
    }

    #[test]
    fn plan_of_quiet_run_keeps_the_stderr_tail() {
        let opts = Options {
            quiet: true,
            ..Options::default()
        };
        let p = plan(&opts, Mode::Run).unwrap();
        assert_eq!(p.stdout.terminal, None);
        assert_eq!(p.stdout.keep, Keep::Nothing);
        assert_eq!(p.stderr.terminal, None);
        assert_eq!(p.stderr.keep, Keep::Tail);
    }

    #[test]
    fn plan_of_quiet_capture_keeps_all() {
        let opts = Options {
            quiet: true,
            tee: true,
            ..Options::default()
        };
        let p = plan(&opts, Mode::Capture).unwrap();
        assert_eq!(p.stdout.terminal, None);
        assert_eq!(p.stderr.keep, Keep::All);
    }

    #[test]
    fn plan_of_tee_run_is_the_terminal() {
        let opts = Options {
            tee: true,
            ..Options::default()
        };
        let p = plan(&opts, Mode::Run).unwrap();
        assert_eq!(p.stdout.terminal, Some(Term::Stdout));
        assert_eq!(p.stdout.keep, Keep::Nothing);
    }

    proptest! {
        /// The kept tail is a suffix of all bytes, with at least
        /// `TAIL_BYTES + 1` bytes when there are that many.
        #[test]
        fn keep_tail_is_a_long_enough_suffix(
            chunks in proptest::collection::vec(
                proptest::collection::vec(any::<u8>(), 0..3000), 0..12),
        ) {
            let mut buffer = Vec::new();
            let mut all = Vec::new();
            for chunk in &chunks {
                keep(&mut buffer, Keep::Tail, chunk);
                all.extend_from_slice(chunk);
            }
            prop_assert!(all.ends_with(&buffer));
            prop_assert!(buffer.len() >= all.len().min(TAIL_BYTES + 1));
            prop_assert!(buffer.len() <= 2 * TAIL_BYTES);
            let text = tail(&buffer);
            prop_assert_eq!(text, tail(&all));
        }
    }
}

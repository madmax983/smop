//! The process I/O engine of [`CommandBuilder`](super::CommandBuilder).
//!
//! All run methods use [`execute`]. `spawn` and `spawn_guarded` use
//! [`spawn`] and [`Running::finish`]. A thread writes stdin. A thread reads
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

use crate::atomic::Pending;

/// The number of stderr bytes that a quiet `run` keeps for its error.
pub(super) const TAIL_BYTES: usize = 4096;

/// The time to wait for open streams after the deadline. A quiet `run`
/// also waits this long for its stderr after the child ends.
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
    /// Returns `true` when an option is set.
    pub(super) const fn any(&self) -> bool {
        self.quiet
            || self.stdin.is_some()
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
    /// A temp file that replaces the file at the path when the command
    /// succeeds.
    Atomic(PathBuf),
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
    /// `run`: the terminal gets the output and gives stdin.
    Run,
    /// `spawn`: as `Run`, but stdin is empty.
    Spawn,
    /// `status`: as `Run`. A quiet `status` keeps no stderr.
    Status,
    /// `success`: no output and no stdin.
    Silent,
    /// `capture` and `output_raw`: smop keeps the output. Stdin is empty.
    Capture,
    /// `output` and `output_lines`: as `Capture`, but only stdout is the
    /// result. As `$(...)` does, smop does not wait long for stderr.
    Output,
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
    /// `true` when smop waits for the end of the stream only a short time
    /// after the child ends. A background process can keep such a stream
    /// open, and the result does not need all of it.
    best_effort: bool,
}

impl Sinks {
    /// Gets the sinks of a stream for `mode` with no options.
    const fn base(mode: Mode, term: Term) -> Self {
        let (terminal, keep) = match mode {
            Mode::Run | Mode::Spawn | Mode::Status => (Some(term), Keep::Nothing),
            Mode::Silent => (None, Keep::Nothing),
            Mode::Capture | Mode::Output => (None, Keep::All),
        };
        Self {
            terminal,
            keep,
            files: Vec::new(),
            best_effort: false,
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
    if opts.tee && matches!(mode, Mode::Capture | Mode::Output) {
        stdout.terminal = Some(Term::Stdout);
        stderr.terminal = Some(Term::Stderr);
    }
    if let StdoutTarget::File(target) = &opts.stdout {
        stdout = Sinks {
            terminal: None,
            keep: Keep::Nothing,
            files: vec![target.clone()],
            best_effort: false,
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
        if matches!(mode, Mode::Run | Mode::Spawn) {
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
    for sinks in [&mut stdout, &mut stderr] {
        sinks.best_effort = sinks.keep == Keep::Tail && sinks.files.is_empty();
    }
    if mode == Mode::Output && merge == Merge::None && stderr.files.is_empty() {
        stderr.best_effort = true;
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
    /// See [`Sinks::best_effort`].
    best_effort: bool,
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
                best_effort: sinks.best_effort,
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
    /// `true` when an interrupt stopped a child in its own group.
    pub(super) interrupted: bool,
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
/// Returns `Err` when an option or a file is bad, or when the command does
/// not start. `spawn_error` makes the error for the start.
pub(super) fn execute(
    command: Command,
    opts: Options,
    mode: Mode,
    spawn_error: impl FnOnce(io::Error) -> anyhow::Error,
) -> Result<Finished> {
    spawn(command, opts, mode, false, spawn_error)?.finish()
}

/// A message from a reader thread: the slot of the stream and its result.
type Done = (usize, Result<()>);

/// A child that runs, and the threads that move its I/O.
#[derive(Debug)]
pub(super) struct Running {
    child: Child,
    /// In a `Mutex` only so that `Running` is `Sync`. Only `finish` reads it.
    done: Mutex<mpsc::Receiver<Done>>,
    buffers: [Buffer; 2],
    readers: [bool; 2],
    best_effort: [bool; 2],
    deadline: Option<Instant>,
    timeout: Option<Duration>,
    stderr_in_stdout: bool,
    /// `true` when a stop also kills the process group of the child.
    tree: bool,
    /// The temp file of `stdout_atomic`.
    atomic: Option<(PathBuf, Pending)>,
}

/// Starts `command` with `opts` and the defaults of `mode`.
///
/// The I/O threads start at once, so a full pipe never stops the child,
/// also when no one waits. The timeout starts here. With `tree`, the child
/// gets its own process group, and a stop kills that group first (see
/// [`super::group`]).
///
/// Returns `Err` when an option or a file is bad, or when the command does
/// not start. `spawn_error` makes the error for the start.
pub(super) fn spawn(
    mut command: Command,
    mut opts: Options,
    mode: Mode,
    tree: bool,
    spawn_error: impl FnOnce(io::Error) -> anyhow::Error,
) -> Result<Running> {
    let atomic = if let StdoutTarget::Atomic(path) = &opts.stdout {
        let path = path.clone();
        let pending = Pending::new(&path)
            .with_context(|| format!("Failed to create a temp file for {}", path.display()))?;
        // The command writes the temp file, as with `stdout_file`.
        opts.stdout = StdoutTarget::File(Target {
            path: pending.temp_path().to_path_buf(),
            append: false,
        });
        Some((path, pending))
    } else {
        None
    };
    let plan = plan(&opts, mode)?;
    if tree {
        super::group::isolate(&mut command);
    }
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
        (None, Mode::Run | Mode::Status) => Stdio::inherit(),
        (None, Mode::Spawn | Mode::Silent | Mode::Capture | Mode::Output) => Stdio::null(),
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
        start(&mut child, tree, "stdin", move || {
            let _ = stdin.write_all(&input);
        })?;
    }

    let (done_tx, done) = mpsc::channel();
    let mut buffers = [Buffer::default(), Buffer::default()];
    let mut readers = [false; 2];
    let mut best_effort = [false; 2];
    for (slot, pump) in [out_pump, err_pump].into_iter().enumerate() {
        let Some(pump) = pump else { continue };
        buffers[slot] = Arc::clone(&pump.buffer);
        readers[slot] = true;
        best_effort[slot] = pump.best_effort;
        let done = done_tx.clone();
        start(&mut child, tree, "output", move || {
            let _ = done.send((slot, pump.run()));
        })?;
    }

    Ok(Running {
        child,
        done: Mutex::new(done),
        buffers,
        readers,
        best_effort,
        // A very large timeout is the same as no timeout.
        deadline: opts.timeout.and_then(|t| Instant::now().checked_add(t)),
        timeout: opts.timeout,
        stderr_in_stdout: plan.merge == Merge::StderrToStdout,
        tree,
        atomic,
    })
}

impl Running {
    /// Gets the child.
    pub(super) const fn child(&self) -> &Child {
        &self.child
    }

    /// Gets the child to kill or poll it.
    pub(super) const fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    /// Kills the child as [`stop`] does. Does not reap it.
    pub(super) fn stop(&mut self) -> io::Result<bool> {
        stop(&mut self.child, self.tree)
    }

    /// Waits for the child and its output. At the deadline, stops the
    /// child and then reaps it.
    pub(super) fn finish(mut self) -> Result<Finished> {
        let (status, stopped) = wait(&mut self.child, self.deadline, self.tree)?;
        let ended = Instant::now();
        // A cutoff too large for `Instant` is the same as no cutoff.
        let cutoff = if stopped == Stopped::Interrupt {
            Some(ended + GRACE)
        } else {
            self.deadline.and_then(|d| d.max(ended).checked_add(GRACE))
        };
        let mut cutoffs = [cutoff; 2];
        for (slot, best) in self.best_effort.into_iter().enumerate() {
            if best {
                let short = ended + GRACE;
                cutoffs[slot] = Some(cutoff.map_or(short, |c| c.min(short)));
            }
        }
        let done = self
            .done
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner);
        let (io_error, open) = collect(&done, self.readers, cutoffs);
        // Output that is still open at the cutoff is not complete. That is
        // a timeout too, unless the stream is best effort.
        let stuck = (0..2).any(|slot| open[slot] && !self.best_effort[slot]);
        if stuck && self.tree {
            // The child ended, but a process holds its stream. That process
            // is in the group, so the group id is still in use. Kill it.
            let _ = super::group::kill(&self.child);
        }

        let [stdout, stderr] = self.buffers.map(|b| std::mem::take(&mut *lock(&b)));
        let timed_out = if stopped == Stopped::Deadline || stuck {
            self.timeout
        } else {
            None
        };
        let interrupted = stopped == Stopped::Interrupt;
        let mut io_error = io_error;
        if let Some((path, pending)) = self.atomic.take() {
            // Only a full success replaces the target. Else the drop
            // removes the temp file.
            if status.success() && timed_out.is_none() && io_error.is_none() && !interrupted {
                io_error = pending
                    .commit()
                    .with_context(|| format!("Failed to replace {}", path.display()))
                    .err();
            }
        }
        Ok(Finished {
            status,
            timed_out,
            stdout,
            stderr,
            stderr_in_stdout: self.stderr_in_stdout,
            io_error,
            interrupted,
        })
    }
}

/// Kills the child. With `tree`, kills its process group first. Does not
/// reap the child, so its group id stays in use.
///
/// Gets the result of the group kill, or `Ok(true)` without `tree`. Ignores
/// the error of the direct kill: the child can end before it.
fn stop(child: &mut Child, tree: bool) -> io::Result<bool> {
    let grouped = if tree {
        super::group::kill(child)
    } else {
        Ok(true)
    };
    let _ = child.kill();
    grouped
}

/// Starts a thread. If it can not start, stops the child.
fn start(
    child: &mut Child,
    tree: bool,
    name: &str,
    body: impl FnOnce() + Send + 'static,
) -> Result<()> {
    if let Err(err) = thread::Builder::new()
        .name(format!("smop-sh-{name}"))
        .spawn(body)
    {
        let _ = stop(child, tree);
        let _ = child.wait();
        return Err(err).context("Failed to start an I/O thread");
    }
    Ok(())
}

/// Why [`wait`] stopped the child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stopped {
    /// The child ended on its own.
    No,
    /// The deadline stopped it.
    Deadline,
    /// An interrupt that a guard caught stopped it.
    Interrupt,
}

/// Waits for the child. At the deadline, stops it as [`stop`] does, and
/// then reaps it.
///
/// With `tree`, an interrupt that a `script::guard` caught also stops the
/// child. The child is in its own group, so the signal does not reach it.
/// Without the stop, a second Ctrl-C ends this process with no drop, and
/// the group runs on.
fn wait(child: &mut Child, deadline: Option<Instant>, tree: bool) -> Result<(ExitStatus, Stopped)> {
    let failed = "Failed to wait for the command";
    if deadline.is_none() && !tree {
        return Ok((child.wait().context(failed)?, Stopped::No));
    }
    let mut pause = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait().context(failed)? {
            return Ok((status, Stopped::No));
        }
        let now = Instant::now();
        let why = if tree && super::interrupted() {
            Some(Stopped::Interrupt)
        } else if deadline.is_some_and(|d| now >= d) {
            Some(Stopped::Deadline)
        } else {
            None
        };
        if let Some(why) = why {
            // The child can end before the kill. Then `wait` still reaps it,
            // and its own status counts. The kill comes before the reap, so
            // the group id is not free for reuse.
            let _ = stop(child, tree);
            let status = child.wait().context(failed)?;
            return Ok((status, if killed(status) { why } else { Stopped::No }));
        }
        let left = deadline.map_or(MAX_POLL, |d| d - now);
        thread::sleep(pause.min(left));
        pause = (pause * 2).min(MAX_POLL);
    }
}

/// Returns `true` when `status` can be the result of `Child::kill`.
pub(super) fn killed(status: ExitStatus) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        const SIGKILL: i32 = 9;
        status.signal() == Some(SIGKILL)
    }
    #[cfg(not(unix))]
    {
        // `TerminateProcess` sets exit code 1. Do not trust a 0.
        !status.success()
    }
}

/// Waits for each reader in `open` until its cutoff (`None`: no limit).
///
/// Returns the first error, and the readers that are still open. A reader
/// that is open at its cutoff continues alone. A grandchild that keeps the
/// stream open causes this.
fn collect(
    done: &mpsc::Receiver<Done>,
    mut open: [bool; 2],
    cutoffs: [Option<Instant>; 2],
) -> (Option<anyhow::Error>, [bool; 2]) {
    let mut waiting = open;
    let mut first = None;
    loop {
        let now = Instant::now();
        for slot in 0..2 {
            if cutoffs[slot].is_some_and(|c| c <= now) {
                waiting[slot] = false;
            }
        }
        if !waiting.contains(&true) {
            break;
        }
        let next = (0..2)
            .filter(|&slot| waiting[slot])
            .filter_map(|slot| cutoffs[slot])
            .min();
        let message = match next {
            None => done.recv().ok(),
            Some(at) => match done.recv_timeout(at.saturating_duration_since(now)) {
                Ok(message) => Some(message),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => None,
            },
        };
        // All senders are gone: no reader can send more.
        let Some((slot, result)) = message else { break };
        open[slot] = false;
        waiting[slot] = false;
        if let Err(err) = result {
            first.get_or_insert(err);
        }
    }
    (first, open)
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
    fn plan_of_quiet_status_keeps_no_stderr() {
        let opts = Options {
            quiet: true,
            ..Options::default()
        };
        let p = plan(&opts, Mode::Status).unwrap();
        assert_eq!(
            (p.stdout.keep, p.stderr.keep),
            (Keep::Nothing, Keep::Nothing)
        );
        assert_eq!((p.stdout.terminal, p.stderr.terminal), (None, None));
    }

    #[test]
    fn plan_of_quiet_merged_run_keeps_the_tail_in_stdout() {
        let opts = Options {
            quiet: true,
            stderr_to_stdout: true,
            ..Options::default()
        };
        let p = plan(&opts, Mode::Run).unwrap();
        assert_eq!(p.merge, Merge::StderrToStdout);
        assert_eq!(p.stdout.keep, Keep::Tail);
    }

    #[test]
    fn plan_of_a_log_of_a_merged_away_stream_is_an_error() {
        let opts = Options {
            stderr_to_stdout: true,
            log_stderr: Some(PathBuf::from("x")),
            ..Options::default()
        };
        assert!(plan(&opts, Mode::Capture).is_err());
    }

    #[test]
    fn plan_of_output_makes_only_stderr_best_effort() {
        let p = plan(&Options::default(), Mode::Output).unwrap();
        assert!(!p.stdout.best_effort);
        assert!(p.stderr.best_effort);
        let p = plan(&Options::default(), Mode::Capture).unwrap();
        assert!(!p.stdout.best_effort && !p.stderr.best_effort);
        let opts = Options {
            log_stderr: Some(PathBuf::from("x")),
            ..Options::default()
        };
        assert!(!plan(&opts, Mode::Output).unwrap().stderr.best_effort);
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
    fn plan_of_spawn_is_the_terminal_and_quiet_keeps_the_tail() {
        let p = plan(&Options::default(), Mode::Spawn).unwrap();
        assert_eq!(p.stdout.terminal, Some(Term::Stdout));
        assert_eq!(p.stderr.terminal, Some(Term::Stderr));
        assert_eq!(
            (p.stdout.keep, p.stderr.keep),
            (Keep::Nothing, Keep::Nothing)
        );
        let opts = Options {
            quiet: true,
            ..Options::default()
        };
        let p = plan(&opts, Mode::Spawn).unwrap();
        assert_eq!((p.stdout.terminal, p.stderr.terminal), (None, None));
        assert_eq!(p.stderr.keep, Keep::Tail);
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

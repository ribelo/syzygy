use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use futures::channel::mpsc;
use futures::io::AsyncRead;
use futures::io::AsyncReadExt;
use thiserror::Error;

const PROCESS_READ_CHUNK_BYTES: usize = 8 * 1024;
const DEFAULT_PROCESS_TERMINATION_GRACE: Duration = Duration::from_millis(500);
pub(crate) const PROCESS_CONTROL_CAPACITY: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessInput {
    Null,
    Inherit,
    Piped,
}

impl Default for ProcessInput {
    fn default() -> Self {
        Self::Null
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessFraming {
    Bytes { max_chunk_bytes: usize },
    Lines { max_line_bytes: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessOutput {
    Discard,
    Inherit,
    Capture { max_bytes: usize },
    Stream { framing: ProcessFraming },
}

impl Default for ProcessOutput {
    fn default() -> Self {
        Self::Discard
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessTerminationPolicy {
    Kill,
    CloseStdinThenKill { grace: Duration },
}

impl Default for ProcessTerminationPolicy {
    fn default() -> Self {
        Self::CloseStdinThenKill {
            grace: DEFAULT_PROCESS_TERMINATION_GRACE,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessSpec {
    program: OsString,
    args: Vec<OsString>,
    current_dir: Option<PathBuf>,
    clear_env: bool,
    envs: Vec<(OsString, OsString)>,
    stdin: ProcessInput,
    stdout: ProcessOutput,
    stderr: ProcessOutput,
    termination_policy: ProcessTerminationPolicy,
}

impl ProcessSpec {
    #[must_use]
    pub fn new(program: impl Into<OsString>) -> Self {
        let program = program.into();
        assert!(
            !program.is_empty(),
            "ProcessSpec::new requires a non-empty program name"
        );

        Self {
            program,
            args: Vec::new(),
            current_dir: None,
            clear_env: false,
            envs: Vec::new(),
            stdin: ProcessInput::default(),
            stdout: ProcessOutput::default(),
            stderr: ProcessOutput::default(),
            termination_policy: ProcessTerminationPolicy::default(),
        }
    }

    #[must_use]
    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn current_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(path.into());
        self
    }

    #[must_use]
    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.envs.push((key.into(), value.into()));
        self
    }

    #[must_use]
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        self.envs.extend(
            vars.into_iter()
                .map(|(key, value)| (key.into(), value.into())),
        );
        self
    }

    #[must_use]
    pub fn clear_env(mut self) -> Self {
        self.clear_env = true;
        self
    }

    #[must_use]
    pub fn stdin(mut self, stdin: ProcessInput) -> Self {
        self.stdin = stdin;
        self
    }

    #[must_use]
    pub fn stdout(mut self, stdout: ProcessOutput) -> Self {
        self.stdout = stdout;
        self
    }

    #[must_use]
    pub fn stderr(mut self, stderr: ProcessOutput) -> Self {
        self.stderr = stderr;
        self
    }

    #[must_use]
    pub fn termination_policy(mut self, policy: ProcessTerminationPolicy) -> Self {
        self.termination_policy = policy;
        self
    }

    #[must_use]
    pub fn program(&self) -> &OsStr {
        &self.program
    }

    #[must_use]
    pub fn args_slice(&self) -> &[OsString] {
        &self.args
    }

    #[must_use]
    pub fn current_dir_path(&self) -> Option<&Path> {
        self.current_dir.as_deref()
    }

    #[must_use]
    pub fn clear_env_enabled(&self) -> bool {
        self.clear_env
    }

    #[must_use]
    pub fn env_pairs(&self) -> &[(OsString, OsString)] {
        &self.envs
    }

    #[must_use]
    pub fn stdin_mode(&self) -> &ProcessInput {
        &self.stdin
    }

    #[must_use]
    pub fn stdout_mode(&self) -> &ProcessOutput {
        &self.stdout
    }

    #[must_use]
    pub fn stderr_mode(&self) -> &ProcessOutput {
        &self.stderr
    }

    #[must_use]
    pub fn termination_policy_ref(&self) -> &ProcessTerminationPolicy {
        &self.termination_policy
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedOutput {
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessExit {
    pub status: ExitStatus,
    pub stdout: Option<CapturedOutput>,
    pub stderr: Option<CapturedOutput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessFrame {
    Bytes(Vec<u8>),
    Line(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessUpdate {
    Stdout(ProcessFrame),
    Stderr(ProcessFrame),
    Exited(Result<ProcessExit, ProcessError>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ProcessErrorKind {
    #[error("spawn")]
    Spawn,
    #[error("wait")]
    Wait,
    #[error("stdout read")]
    StdoutRead,
    #[error("stderr read")]
    StderrRead,
    #[error("stdout frame too long")]
    StdoutFrameTooLong,
    #[error("stderr frame too long")]
    StderrFrameTooLong,
    #[error("stdin write")]
    StdinWrite,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("process {kind} failed for `{program}`: {message}")]
pub struct ProcessError {
    pub kind: ProcessErrorKind,
    pub program: String,
    pub message: String,
}

impl ProcessError {
    #[must_use]
    pub(crate) fn new(
        spec: &ProcessSpec,
        kind: ProcessErrorKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            program: spec.program.to_string_lossy().into_owned(),
            message: message.into(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ProcessControlMessage {
    Write(Vec<u8>),
    CloseStdin,
}

pub(crate) type ProcessControlReceiver = mpsc::Receiver<ProcessControlMessage>;
pub(crate) type ProcessControlSender = mpsc::Sender<ProcessControlMessage>;
type BoxProcessReader = Box<dyn AsyncRead + Unpin>;

struct CaptureDriver {
    reader: BoxProcessReader,
    max_bytes: usize,
    bytes: Vec<u8>,
    truncated: bool,
}

struct StreamDriver {
    reader: BoxProcessReader,
    framing: ProcessFraming,
    pending_frames: VecDeque<ProcessFrame>,
    line_buffer: Vec<u8>,
}

enum ProcessOutputDriverKind {
    Disabled,
    Capture(CaptureDriver),
    Stream(StreamDriver),
}

pub(crate) struct ProcessOutputDriver {
    kind: ProcessOutputDriverKind,
    read_error_kind: ProcessErrorKind,
    frame_error_kind: ProcessErrorKind,
    finished: bool,
}

impl ProcessOutputDriver {
    #[must_use]
    pub(crate) fn stdout(
        reader: Option<async_process::ChildStdout>,
        output: &ProcessOutput,
    ) -> Self {
        Self::new(reader.map(box_reader), output, ProcessErrorKind::StdoutRead)
    }

    #[must_use]
    pub(crate) fn stderr(
        reader: Option<async_process::ChildStderr>,
        output: &ProcessOutput,
    ) -> Self {
        Self::new(reader.map(box_reader), output, ProcessErrorKind::StderrRead)
    }

    #[must_use]
    fn new(
        reader: Option<BoxProcessReader>,
        output: &ProcessOutput,
        read_error_kind: ProcessErrorKind,
    ) -> Self {
        let frame_error_kind = frame_error_kind(read_error_kind);
        let kind = match output {
            ProcessOutput::Discard | ProcessOutput::Inherit => ProcessOutputDriverKind::Disabled,
            ProcessOutput::Capture { max_bytes } => {
                let reader = required_reader(reader, output);
                ProcessOutputDriverKind::Capture(CaptureDriver {
                    reader,
                    max_bytes: *max_bytes,
                    bytes: Vec::with_capacity((*max_bytes).min(PROCESS_READ_CHUNK_BYTES)),
                    truncated: false,
                })
            }
            ProcessOutput::Stream { framing } => {
                let reader = required_reader(reader, output);
                ProcessOutputDriverKind::Stream(StreamDriver {
                    reader,
                    framing: framing.clone(),
                    pending_frames: VecDeque::new(),
                    line_buffer: Vec::new(),
                })
            }
        };
        let finished = matches!(kind, ProcessOutputDriverKind::Disabled);

        Self {
            kind,
            read_error_kind,
            frame_error_kind,
            finished,
        }
    }

    #[must_use]
    pub(crate) fn should_poll(&self) -> bool {
        !self.finished
    }

    #[must_use]
    pub(crate) fn is_finished(&self) -> bool {
        self.finished
    }

    pub(crate) fn captured_output(&mut self) -> Option<CapturedOutput> {
        match &mut self.kind {
            ProcessOutputDriverKind::Capture(driver) => Some(CapturedOutput {
                bytes: std::mem::take(&mut driver.bytes),
                truncated: driver.truncated,
            }),
            ProcessOutputDriverKind::Disabled | ProcessOutputDriverKind::Stream(_) => None,
        }
    }

    pub(crate) async fn next_frame(
        &mut self,
        spec: &ProcessSpec,
    ) -> Result<Option<ProcessFrame>, ProcessError> {
        match &mut self.kind {
            ProcessOutputDriverKind::Disabled => Ok(None),
            ProcessOutputDriverKind::Capture(driver) => {
                let read = read_into_capture(driver, spec, self.read_error_kind).await?;
                self.finished = read == 0;
                Ok(None)
            }
            ProcessOutputDriverKind::Stream(driver) => {
                let frame =
                    read_into_stream(driver, spec, self.read_error_kind, self.frame_error_kind)
                        .await?;
                self.finished = frame.is_none() && !driver_has_pending_frames(driver);
                Ok(frame)
            }
        }
    }
}

#[must_use]
pub(crate) fn control_channel() -> (ProcessControlSender, ProcessControlReceiver) {
    mpsc::channel(PROCESS_CONTROL_CAPACITY)
}

pub(crate) fn spawn(spec: &ProcessSpec) -> Result<async_process::Child, ProcessError> {
    let mut command = async_process::Command::new(spec.program());
    command.args(spec.args_slice());
    command.kill_on_drop(true);
    command.reap_on_drop(true);
    apply_spec(&mut command, spec);
    command
        .spawn()
        .map_err(|err| ProcessError::new(spec, ProcessErrorKind::Spawn, err.to_string()))
}

fn apply_spec(command: &mut async_process::Command, spec: &ProcessSpec) {
    if let Some(current_dir) = spec.current_dir_path() {
        command.current_dir(current_dir);
    }
    if spec.clear_env_enabled() {
        command.env_clear();
    }
    command.envs(spec.env_pairs().iter().map(|(key, value)| (key, value)));
    command.stdin(stdio_for_input(spec.stdin_mode()));
    command.stdout(stdio_for_output(spec.stdout_mode()));
    command.stderr(stdio_for_output(spec.stderr_mode()));
}

fn stdio_for_input(stdin: &ProcessInput) -> Stdio {
    match stdin {
        ProcessInput::Null => Stdio::null(),
        ProcessInput::Inherit => Stdio::inherit(),
        ProcessInput::Piped => Stdio::piped(),
    }
}

fn stdio_for_output(output: &ProcessOutput) -> Stdio {
    match output {
        ProcessOutput::Discard => Stdio::null(),
        ProcessOutput::Inherit => Stdio::inherit(),
        ProcessOutput::Capture { .. } | ProcessOutput::Stream { .. } => Stdio::piped(),
    }
}

fn box_reader<R>(reader: R) -> BoxProcessReader
where
    R: AsyncRead + Unpin + 'static,
{
    Box::new(reader)
}

fn required_reader(reader: Option<BoxProcessReader>, output: &ProcessOutput) -> BoxProcessReader {
    match reader {
        Some(reader) => reader,
        None => panic!("process output {output:?} requires a piped reader"),
    }
}

#[must_use]
fn frame_error_kind(read_error_kind: ProcessErrorKind) -> ProcessErrorKind {
    match read_error_kind {
        ProcessErrorKind::StdoutRead => ProcessErrorKind::StdoutFrameTooLong,
        ProcessErrorKind::StderrRead => ProcessErrorKind::StderrFrameTooLong,
        other => panic!("unsupported frame error kind for {other:?}"),
    }
}

#[must_use]
fn driver_has_pending_frames(driver: &StreamDriver) -> bool {
    !driver.pending_frames.is_empty() || !driver.line_buffer.is_empty()
}

async fn read_into_capture(
    driver: &mut CaptureDriver,
    spec: &ProcessSpec,
    error_kind: ProcessErrorKind,
) -> Result<usize, ProcessError> {
    let mut chunk = vec![0_u8; PROCESS_READ_CHUNK_BYTES];
    let read = driver
        .reader
        .read(&mut chunk)
        .await
        .map_err(|err| ProcessError::new(spec, error_kind, err.to_string()))?;
    if read == 0 {
        return Ok(0);
    }

    let remaining = driver.max_bytes.saturating_sub(driver.bytes.len());
    let keep = remaining.min(read);
    driver.bytes.extend_from_slice(&chunk[..keep]);
    driver.truncated |= keep < read;
    Ok(read)
}

async fn read_into_stream(
    driver: &mut StreamDriver,
    spec: &ProcessSpec,
    read_error_kind: ProcessErrorKind,
    frame_error_kind: ProcessErrorKind,
) -> Result<Option<ProcessFrame>, ProcessError> {
    if let Some(frame) = driver.pending_frames.pop_front() {
        return Ok(Some(frame));
    }

    let mut chunk = vec![0_u8; read_buffer_len(&driver.framing)];
    let read = driver
        .reader
        .read(&mut chunk)
        .await
        .map_err(|err| ProcessError::new(spec, read_error_kind, err.to_string()))?;
    if read == 0 {
        return Ok(emit_line_buffer_on_eof(driver));
    }

    match &driver.framing {
        ProcessFraming::Bytes { .. } => Ok(Some(ProcessFrame::Bytes(chunk[..read].to_vec()))),
        ProcessFraming::Lines { max_line_bytes } => {
            push_line_frames(
                driver,
                &chunk[..read],
                *max_line_bytes,
                spec,
                frame_error_kind,
            )?;
            Ok(driver.pending_frames.pop_front())
        }
    }
}

#[must_use]
fn read_buffer_len(framing: &ProcessFraming) -> usize {
    match framing {
        ProcessFraming::Bytes { max_chunk_bytes } => (*max_chunk_bytes).max(1),
        ProcessFraming::Lines { .. } => PROCESS_READ_CHUNK_BYTES,
    }
}

fn emit_line_buffer_on_eof(driver: &mut StreamDriver) -> Option<ProcessFrame> {
    if driver.pending_frames.is_empty() && driver.line_buffer.is_empty() {
        return None;
    }

    if let Some(frame) = driver.pending_frames.pop_front() {
        return Some(frame);
    }

    Some(ProcessFrame::Line(std::mem::take(&mut driver.line_buffer)))
}

fn push_line_frames(
    driver: &mut StreamDriver,
    chunk: &[u8],
    max_line_bytes: usize,
    spec: &ProcessSpec,
    error_kind: ProcessErrorKind,
) -> Result<(), ProcessError> {
    assert!(
        max_line_bytes > 0,
        "line framing requires max_line_bytes > 0"
    );

    for byte in chunk {
        driver.line_buffer.push(*byte);
        if driver.line_buffer.len() > max_line_bytes {
            return Err(ProcessError::new(
                spec,
                error_kind,
                format!("line exceeded configured limit of {max_line_bytes} bytes"),
            ));
        }

        if *byte == b'\n' {
            driver
                .pending_frames
                .push_back(ProcessFrame::Line(std::mem::take(&mut driver.line_buffer)));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::{
        box_reader, frame_error_kind, push_line_frames, CapturedOutput, ProcessErrorKind,
        ProcessFraming, ProcessInput, ProcessOutput, ProcessSpec, ProcessTerminationPolicy,
        StreamDriver,
    };

    #[test]
    fn process_spec_defaults_are_safe() {
        let spec = ProcessSpec::new("git");

        assert_eq!(spec.stdout_mode(), &ProcessOutput::Discard);
        assert_eq!(spec.stderr_mode(), &ProcessOutput::Discard);
        assert_eq!(spec.stdin_mode(), &ProcessInput::Null);
        assert_eq!(
            spec.termination_policy_ref(),
            &ProcessTerminationPolicy::CloseStdinThenKill {
                grace: std::time::Duration::from_millis(500),
            }
        );
    }

    #[test]
    fn process_spec_builders_accumulate_arguments_and_env() {
        let spec = ProcessSpec::new("git")
            .arg("status")
            .args(["--short"])
            .env("A", "1")
            .envs([("B", "2")]);

        assert_eq!(spec.args_slice().len(), 2);
        assert_eq!(spec.env_pairs().len(), 2);
    }

    #[test]
    fn captured_output_tracks_truncation() {
        let captured = CapturedOutput {
            bytes: vec![1, 2, 3],
            truncated: true,
        };

        assert_eq!(captured.bytes.len(), 3);
        assert!(captured.truncated);
    }

    #[test]
    fn frame_error_kind_tracks_output_channel() {
        assert_eq!(
            frame_error_kind(ProcessErrorKind::StdoutRead),
            ProcessErrorKind::StdoutFrameTooLong
        );
        assert_eq!(
            frame_error_kind(ProcessErrorKind::StderrRead),
            ProcessErrorKind::StderrFrameTooLong
        );
    }

    #[test]
    fn line_framing_preserves_newlines() {
        let spec = ProcessSpec::new("cat");
        let mut driver = StreamDriver {
            reader: box_reader(futures::io::Cursor::new(Vec::<u8>::new())),
            framing: ProcessFraming::Lines { max_line_bytes: 16 },
            pending_frames: VecDeque::new(),
            line_buffer: Vec::new(),
        };

        push_line_frames(
            &mut driver,
            b"one\ntwo\n",
            16,
            &spec,
            ProcessErrorKind::StdoutFrameTooLong,
        )
        .unwrap();

        assert_eq!(
            driver.pending_frames.pop_front(),
            Some(super::ProcessFrame::Line(b"one\n".to_vec()))
        );
        assert_eq!(
            driver.pending_frames.pop_front(),
            Some(super::ProcessFrame::Line(b"two\n".to_vec()))
        );
    }

    #[test]
    fn line_framing_rejects_oversized_lines() {
        let spec = ProcessSpec::new("cat");
        let mut driver = StreamDriver {
            reader: box_reader(futures::io::Cursor::new(Vec::<u8>::new())),
            framing: ProcessFraming::Lines { max_line_bytes: 3 },
            pending_frames: VecDeque::new(),
            line_buffer: Vec::new(),
        };

        let err = push_line_frames(
            &mut driver,
            b"toolong",
            3,
            &spec,
            ProcessErrorKind::StdoutFrameTooLong,
        )
        .unwrap_err();

        assert_eq!(err.kind, ProcessErrorKind::StdoutFrameTooLong);
    }
}

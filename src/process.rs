use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};

use futures::future::join3;
use futures::io::{AsyncRead, AsyncReadExt};
use thiserror::Error;

const PROCESS_READ_CHUNK_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessInput {
    Null,
    Inherit,
}

impl Default for ProcessInput {
    fn default() -> Self {
        Self::Null
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessOutput {
    Discard,
    Inherit,
    Capture { max_bytes: usize },
}

impl Default for ProcessOutput {
    fn default() -> Self {
        Self::Discard
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
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("process {kind} failed for `{program}`: {message}")]
pub struct ProcessError {
    pub kind: ProcessErrorKind,
    pub program: String,
    pub message: String,
}

impl ProcessError {
    fn new(spec: &ProcessSpec, kind: ProcessErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            program: spec.program.to_string_lossy().into_owned(),
            message: message.into(),
        }
    }
}

pub(crate) async fn run(spec: ProcessSpec) -> Result<ProcessExit, ProcessError> {
    let stdout_mode = spec.stdout.clone();
    let stderr_mode = spec.stderr.clone();
    let mut child = spawn_child(&spec)?;
    let stdout = output_reader(
        child.stdout.take(),
        &spec,
        stdout_mode,
        ProcessErrorKind::StdoutRead,
    );
    let stderr = output_reader(
        child.stderr.take(),
        &spec,
        stderr_mode,
        ProcessErrorKind::StderrRead,
    );
    let status = async {
        child
            .status()
            .await
            .map_err(|err| ProcessError::new(&spec, ProcessErrorKind::Wait, err.to_string()))
    };
    let (status, stdout, stderr) = Box::pin(join3(status, stdout, stderr)).await;

    Ok(ProcessExit {
        status: status?,
        stdout: stdout?,
        stderr: stderr?,
    })
}

fn spawn_child(spec: &ProcessSpec) -> Result<async_process::Child, ProcessError> {
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
    }
}

fn stdio_for_output(output: &ProcessOutput) -> Stdio {
    match output {
        ProcessOutput::Discard => Stdio::null(),
        ProcessOutput::Inherit => Stdio::inherit(),
        ProcessOutput::Capture { .. } => Stdio::piped(),
    }
}

async fn output_reader<R>(
    reader: Option<R>,
    spec: &ProcessSpec,
    output: ProcessOutput,
    error_kind: ProcessErrorKind,
) -> Result<Option<CapturedOutput>, ProcessError>
where
    R: AsyncRead + Unpin,
{
    match output {
        ProcessOutput::Discard | ProcessOutput::Inherit => Ok(None),
        ProcessOutput::Capture { max_bytes } => {
            let Some(reader) = reader else {
                panic!("captured process output must expose a piped reader");
            };

            read_captured_output(reader, spec, max_bytes, error_kind)
                .await
                .map(Some)
        }
    }
}

async fn read_captured_output<R>(
    mut reader: R,
    spec: &ProcessSpec,
    max_bytes: usize,
    error_kind: ProcessErrorKind,
) -> Result<CapturedOutput, ProcessError>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(max_bytes.min(PROCESS_READ_CHUNK_BYTES));
    let mut chunk = [0_u8; PROCESS_READ_CHUNK_BYTES];
    let mut truncated = false;

    loop {
        let read = reader
            .read(&mut chunk)
            .await
            .map_err(|err| ProcessError::new(spec, error_kind, err.to_string()))?;
        if read == 0 {
            break;
        }

        let remaining = max_bytes.saturating_sub(bytes.len());
        let keep = remaining.min(read);
        bytes.extend_from_slice(&chunk[..keep]);
        truncated |= keep < read;
    }

    Ok(CapturedOutput { bytes, truncated })
}

#[cfg(test)]
mod tests {
    use super::{CapturedOutput, ProcessOutput, ProcessSpec};

    #[test]
    fn process_spec_defaults_are_safe() {
        let spec = ProcessSpec::new("git");

        assert_eq!(spec.stdout_mode(), &ProcessOutput::Discard);
        assert_eq!(spec.stderr_mode(), &ProcessOutput::Discard);
        assert_eq!(spec.stdin_mode(), &super::ProcessInput::Null);
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
}

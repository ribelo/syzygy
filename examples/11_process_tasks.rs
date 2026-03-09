use std::time::Duration;

use syzygy::prelude::*;

#[derive(Debug, Default, Model)]
struct AppModel {
    #[model(wrapper = Job)]
    job: AbortSlot,
    #[model(wrapper = Status)]
    status: String,
    #[model(wrapper = Output)]
    output: Vec<String>,
}

#[derive(Debug, Clone)]
enum Event {
    Start,
    Stop,
    Stdout(ProcessFrame),
    Completed(ProcessExit),
    Failed(ProcessError),
}

#[derive(Debug, Clone)]
enum Effect {
    EchoInput,
}

fn start(job: &mut Job, status: &mut Status) -> Command<Event, Effect> {
    **status = "running interactive process".into();
    job.start(Effect::EchoInput)
        .and(job.write(line_bytes("syzygy owns this process")))
        .and(job.write(line_bytes("stdin control is explicit")))
        .and(job.close_stdin())
}

fn stop(job: &mut Job, status: &mut Status) -> Command<Event, Effect> {
    **status = "cancelled".into();
    job.cancel()
}

fn stdout(frame: ProcessFrame, output: &mut Output) -> Command<Event, Effect> {
    let line = match frame {
        ProcessFrame::Bytes(bytes) | ProcessFrame::Line(bytes) => {
            String::from_utf8_lossy(&bytes).trim_end().to_owned()
        }
    };
    output.push(line);
    Command::none()
}

fn completed(exit: ProcessExit, job: &mut Job, status: &mut Status) -> Command<Event, Effect> {
    job.clear();
    **status = format!("finished with {}", exit.status);
    Command::none()
}

fn failed(error: ProcessError, job: &mut Job, status: &mut Status) -> Command<Event, Effect> {
    job.clear();
    **status = format!("process failed: {error}");
    Command::none()
}

fn handle_event(event: Event, ctx: &EventContext<AppModel>) -> Command<Event, Effect> {
    match event {
        Event::Start => handle!(start, ctx),
        Event::Stop => handle!(stop, ctx),
        Event::Stdout(frame) => handle!(stdout, ctx, frame),
        Event::Completed(exit) => handle!(completed, ctx, exit),
        Event::Failed(error) => handle!(failed, ctx, error),
    }
}

fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
    match effect {
        Effect::EchoInput => Task::process_interactive(process_spec(), |update| match update {
            ProcessUpdate::Stdout(frame) => Some(Command::event(Event::Stdout(frame))),
            ProcessUpdate::Exited(result) => Some(match result {
                Ok(exit) => Command::event(Event::Completed(exit)),
                Err(error) => Command::event(Event::Failed(error)),
            }),
            ProcessUpdate::Stderr(_) => None,
        }),
    }
}

fn process_spec() -> ProcessSpec {
    ProcessSpec::new(process_program())
        .args(process_args())
        .stdin(ProcessInput::Piped)
        .stdout(ProcessOutput::Stream {
            framing: ProcessFraming::Lines {
                max_line_bytes: 256,
            },
        })
        .termination_policy(ProcessTerminationPolicy::CloseStdinThenKill {
            grace: Duration::from_millis(50),
        })
}

#[cfg(windows)]
fn process_program() -> &'static str {
    "cmd"
}

#[cfg(not(windows))]
fn process_program() -> &'static str {
    "sh"
}

#[cfg(windows)]
fn process_args() -> [&'static str; 3] {
    ["/Q", "/C", "more"]
}

#[cfg(not(windows))]
fn process_args() -> [&'static str; 2] {
    ["-c", "cat"]
}

fn line_bytes(line: &str) -> Vec<u8> {
    #[cfg(windows)]
    {
        format!("{line}\r\n").into_bytes()
    }

    #[cfg(not(windows))]
    {
        format!("{line}\n").into_bytes()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = syzygy::runtime::Runtime::new()?;
    let mut app = Syzygy::builder::<Event, Effect>()
        .with_runtime(runtime)
        .model(AppModel::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()?;

    app.core().emit(Event::Start);
    if std::env::args().any(|arg| arg == "--cancel") {
        app.core().emit(Event::Stop);
    }
    app.run()?;

    println!("status: {}", app.model().status);
    println!("output:");
    for line in &app.model().output {
        println!("  {line}");
    }

    Ok(())
}

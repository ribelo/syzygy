use syzygy::prelude::*;

#[derive(Debug, Default, Model)]
struct AppModel {
    #[model(wrapper = Job)]
    job: AbortSlot,
    #[model(wrapper = Status)]
    status: String,
    #[model(wrapper = Output)]
    output: String,
}

#[derive(Debug, Clone)]
enum Event {
    Start,
    Stop,
    Completed(ProcessExit),
    Failed(ProcessError),
}

#[derive(Debug, Clone)]
enum Effect {
    InspectWorkspace,
}

fn start(job: &mut Job, status: &mut Status) -> Command<Event, Effect> {
    **status = "running process".into();
    job.start(Effect::InspectWorkspace)
}

fn stop(job: &mut Job, status: &mut Status) -> Command<Event, Effect> {
    **status = "cancelled".into();
    job.cancel()
}

fn completed(
    exit: ProcessExit,
    job: &mut Job,
    status: &mut Status,
    output: &mut Output,
) -> Command<Event, Effect> {
    job.clear();
    **status = format!("finished with {}", exit.status);
    **output = exit
        .stdout
        .as_ref()
        .map(|stdout| String::from_utf8_lossy(&stdout.bytes).into_owned())
        .unwrap_or_default();
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
        Event::Completed(exit) => handle!(completed, ctx, exit),
        Event::Failed(error) => handle!(failed, ctx, error),
    }
}

fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
    match effect {
        Effect::InspectWorkspace => Task::process(process_spec(), |result| match result {
            Ok(exit) => Command::event(Event::Completed(exit)),
            Err(error) => Command::event(Event::Failed(error)),
        }),
    }
}

#[cfg(windows)]
fn process_spec() -> ProcessSpec {
    ProcessSpec::new("cmd")
        .args(["/C", "echo syzygy owns this process"])
        .stdout(ProcessOutput::Capture { max_bytes: 256 })
}

#[cfg(not(windows))]
fn process_spec() -> ProcessSpec {
    ProcessSpec::new("sh")
        .args(["-c", "printf 'syzygy owns this process'"])
        .stdout(ProcessOutput::Capture { max_bytes: 256 })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = syzygy::runtime::Runtime::new()?;
    let mut app = Syzygy::builder::<Event, Effect>()
        .with_runtime(runtime)
        .model(AppModel::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()?;

    app.core().try_send(Event::Start)?;
    if std::env::args().any(|arg| arg == "--cancel") {
        app.core().try_send(Event::Stop)?;
    }
    app.run()?;

    println!("status: {}", app.model().status);
    println!("output: {}", app.model().output);

    Ok(())
}

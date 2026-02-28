use syzygy::prelude::*;

// -----------------------------------------------------------------------------
// Domain Types
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Todo {
    pub id: u32,
    pub text: String,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FilterType {
    #[default]
    All,
    Active,
    Completed,
}

// -----------------------------------------------------------------------------
// Application State (Model)
// -----------------------------------------------------------------------------

/// ## Why combine everything here?
/// This application shows how the TEA pattern scales to a realistic scenario.
/// It uses wrapper types (Example 01), `#[extract]` (Example 02), async effects (Example 04),
/// and error handling (Example 09).
#[derive(Debug, Model)]
pub struct TodoApp {
    pub todos: Vec<Todo>,
    pub next_id: u32,
    #[extract]
    pub filter: FilterType,
    pub sync_error: Option<String>,
}

impl Default for TodoApp {
    fn default() -> Self {
        Self {
            todos: Vec::new(),
            next_id: 1,
            filter: FilterType::All,
            sync_error: None,
        }
    }
}

// -----------------------------------------------------------------------------
// Events
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    // User Actions
    AddTodo(String),
    ToggleTodo(u32),
    DeleteTodo(u32),
    SetFilter(FilterType),

    // System Actions
    SyncFinished(Result<(), String>),
}

// -----------------------------------------------------------------------------
// Effects
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    SaveTodos(Vec<Todo>),
}

// -----------------------------------------------------------------------------
// Event Handlers
// -----------------------------------------------------------------------------

fn add_todo(text: String, todos: &mut Todos, next_id: &mut NextId) -> Command<Event, Effect> {
    if text.trim().is_empty() {
        return Command::none();
    }

    let new_todo = Todo {
        id: **next_id,
        text,
        completed: false,
    };

    todos.push(new_todo);
    **next_id += 1;

    Command::effect(Effect::SaveTodos((**todos).clone()))
}

fn toggle_todo(id: u32, todos: &mut Todos) -> Command<Event, Effect> {
    if let Some(todo) = todos.iter_mut().find(|t| t.id == id) {
        todo.completed = !todo.completed;
        Command::effect(Effect::SaveTodos((**todos).clone()))
    } else {
        Command::none()
    }
}

fn delete_todo(id: u32, todos: &mut Todos) -> Command<Event, Effect> {
    todos.retain(|t| t.id != id);
    Command::effect(Effect::SaveTodos((**todos).clone()))
}

fn set_filter(new_filter: FilterType, filter: &mut FilterType) -> Command<Event, Effect> {
    *filter = new_filter;
    Command::none()
}

fn sync_finished(result: Result<(), String>, sync_error: &mut SyncError) -> Command<Event, Effect> {
    match result {
        Ok(()) => **sync_error = None,
        Err(err) => **sync_error = Some(err),
    }
    Command::none()
}

fn handle_event(event: Event, ctx: &EventContext<TodoApp>) -> Command<Event, Effect> {
    match event {
        Event::AddTodo(text) => handle!(add_todo, text, ctx),
        Event::ToggleTodo(id) => handle!(toggle_todo, id, ctx),
        Event::DeleteTodo(id) => handle!(delete_todo, id, ctx),
        Event::SetFilter(f) => handle!(set_filter, f, ctx),
        Event::SyncFinished(res) => handle!(sync_finished, res, ctx),
    }
}

// -----------------------------------------------------------------------------
// Effect Handlers
// -----------------------------------------------------------------------------

fn save(todos: Vec<Todo>) -> Task<Event, Effect> {
    // In a real app, this would be Task::future saving to disk asynchronously
    Task::blocking(move || {
        // Simulate arbitrary failure to test error recovery
        if todos.len() > 10 {
            Command::event(Event::SyncFinished(Err("Too many items!".into())))
        } else {
            Command::event(Event::SyncFinished(Ok(())))
        }
    })
}

fn handle_effect(effect: Effect, ctx: &EffectContext<'_>) -> Task<Event, Effect> {
    match effect {
        Effect::SaveTodos(todos) => handle!(save, todos, ctx),
    }
}

// -----------------------------------------------------------------------------
// Main
// -----------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(TodoApp::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build();

    // Add a todo
    runner
        .core()
        .try_send_event(Event::AddTodo("Buy groceries".into()))?;
    runner.step()?; // handles add
    runner.step()?; // handles save effect task (blocking)

    assert_eq!(runner.model().todos.len(), 1);

    // Toggle the todo
    runner.core().try_send_event(Event::ToggleTodo(1))?;
    runner.step()?;
    runner.step()?;

    println!(
        "Todo App ran successfully! Total items: {}",
        runner.model().todos.len()
    );

    Ok(())
}

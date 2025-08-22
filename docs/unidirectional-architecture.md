# Unidirectional Architecture Decision

## Overview

Syzygy follows a strict **unidirectional data flow** pattern where events flow in one direction through the system. This document explains why we chose this architecture over bidirectional request/response patterns and the benefits it provides.

## The Decision: Unidirectional vs Bidirectional

### What We Chose: Unidirectional Flow

```
Event → Core → Command → Shell → Effect → [Optional Event Back to Core]
```

**Key Characteristics:**
- Events trigger updates in Core
- Core returns Commands describing effects to execute
- Shell executes effects and optionally sends new events back to Core
- No waiting, no responses, no complex chaining

### What We Rejected: Bidirectional Request/Response

```
Event → Core → Request → Shell → Response → Back to Command → More Requests...
```

**Rejected Characteristics:**
- Commands wait for responses from Shell
- Complex chaining based on response data
- Bidirectional communication channels
- Request/response correlation and matching

## Why Unidirectional?

### 1. Simplicity and Predictability

**Unidirectional (Simple):**
```rust
fn update(&self, event: Event, model: &mut Model) -> Command<Event, Effect> {
    match event {
        Event::LoadUser { id } => {
            Command::effect(Effect::HttpRequest { url: format!("/users/{}", id) })
        }
        Event::UserLoaded { user } => {
            model.current_user = Some(user);
            Command::events(vec![
                Event::UserProfileLoaded,
                Event::RefreshDashboard,
            ])
        }
    }
}
```

**Bidirectional (Complex):**
```rust
fn update(&self, event: Event, model: &mut Model) -> Command<Event, Effect> {
    match event {
        Event::LoadUser { id } => {
            Http::get(format!("/users/{}", id))
                .then_request(|user_result| {
                    match user_result {
                        Ok(user) => Http::get(format!("/users/{}/preferences", user.id)),
                        Err(_) => NoOpRequest::new(),
                    }
                })
                .then_send(|prefs_result| {
                    Event::UserAndPreferencesLoaded { user_result, prefs_result }
                })
        }
    }
}
```

### 2. Easier Testing

**Unidirectional Testing:**
```rust
#[test]
fn test_user_loading() {
    let mut model = Model::default();
    
    // Test the request
    let command = app.update(Event::LoadUser { id: 1 }, &mut model);
    let effects = extract_effects(command);
    assert_eq!(effects.len(), 1);
    assert!(matches!(effects[0], Effect::HttpRequest { .. }));
    
    // Test the response handling
    let command = app.update(Event::UserLoaded { user: test_user() }, &mut model);
    assert_eq!(model.current_user.unwrap().id, 1);
}
```

**Bidirectional Testing (Complex):**
```rust
#[test]
fn test_user_loading() {
    let mut model = Model::default();
    let command = app.update(Event::LoadUser { id: 1 }, &mut model);
    
    // Mock first request
    let request1 = extract_first_request(command);
    request1.resolve(Ok(test_user()));
    
    // Mock second request
    let request2 = extract_next_request(command);
    request2.resolve(Ok(test_preferences()));
    
    // Check final result
    let events = extract_events(command);
    assert!(matches!(events[0], Event::UserAndPreferencesLoaded { .. }));
}
```

### 3. No Race Conditions

**Unidirectional (Safe):**
- Effects execute independently
- Events flow back to Core when ready
- No coordination between concurrent effects required
- Natural backpressure through event channels

**Bidirectional (Race-Prone):**
- Multiple requests waiting for responses
- Complex correlation logic required
- Race conditions when requests complete out of order
- Deadlock potential with circular dependencies

### 4. Better Error Handling

**Unidirectional (Clean):**
```rust
// Effect handler
fn handle_effect(effect: Effect, event_sender: Sender<Event>) {
    match effect {
        Effect::HttpRequest { url } => {
            match http_client.get(&url).await {
                Ok(response) => {
                    let _ = event_sender.send(Event::DataLoaded { data: response.data });
                }
                Err(error) => {
                    let _ = event_sender.send(Event::Error { 
                        message: error.to_string() 
                    });
                }
            }
        }
    }
}
```

**Bidirectional (Complex):**
```rust
// Request handler with error correlation
fn handle_request(request: Request<Operation>) {
    match request.operation() {
        Operation::HttpGet { url } => {
            match http_client.get(url).await {
                Ok(response) => request.resolve(Ok(response)),
                Err(error) => request.resolve(Err(error)), // Now what?
            }
        }
    }
    // How do we handle errors in chained requests?
}
```

### 5. Scalability and Performance

**Unidirectional Benefits:**
- Effects can execute in parallel naturally
- No blocking on responses
- Simple channel-based communication
- Easy to add more effect handlers
- Natural load balancing

**Bidirectional Drawbacks:**
- Commands block waiting for responses
- Complex correlation overhead
- Serialization/deserialization of request/response data
- Memory overhead from pending requests
- Harder to parallelize

## Real-World Examples

### Loading Dashboard Data

**Unidirectional Approach:**
```rust
// User clicks "Refresh Dashboard"
Event::RefreshDashboard => {
    model.is_loading = true;
    Command::parallel(vec![
        Effect::HttpRequest { url: "/api/stats".to_string() },
        Effect::HttpRequest { url: "/api/notifications".to_string() },
        Effect::HttpRequest { url: "/api/recent-activity".to_string() },
    ])
}

// Each completes independently and sends events back
Event::StatsLoaded { stats } => {
    model.stats = Some(stats);
    Command::none() // Simple!
}

Event::NotificationsLoaded { notifications } => {
    model.notifications = notifications;
    Command::none() // Simple!
}

Event::RecentActivityLoaded { activity } => {
    model.recent_activity = activity;
    model.is_loading = false; // Last one finished
    Command::none() // Simple!
}
```

**Legacy Bidirectional Approach (Removed - shown for comparison):**
```rust
// OLD API - no longer available
Event::RefreshDashboard => {
    model.is_loading = true;
    Command::all([  // <- This API was removed
        Http::get("/api/stats").build(),
        Http::get("/api/notifications").build(),
        Http::get("/api/recent-activity").build(),
    ]).then_send(|results| {  // <- Complex coordination
        Event::DashboardDataLoaded {
            stats: results[0],
            notifications: results[1],
            activity: results[2],
        }
    })
}
```

**Current Syzygy Approach (Simple + Powerful):**
```rust
// Modern event-chaining pattern
Event::RefreshDashboard => {
    model.is_loading = true;
    // Effects run in parallel by default
    Command::batch([
        Command::effect(Effect::LoadStats),
        Command::effect(Effect::LoadNotifications),
        Command::effect(Effect::LoadRecentActivity),
    ])
}

// Handle each result independently as they arrive
Event::StatsLoaded { stats } => {
    model.stats = Some(stats);
    check_if_dashboard_complete(model)
}

Event::NotificationsLoaded { notifications } => {
    model.notifications = Some(notifications);
    check_if_dashboard_complete(model)
}

Event::ActivityLoaded { activity } => {
    model.recent_activity = Some(activity);
    check_if_dashboard_complete(model)
}

fn check_if_dashboard_complete(model: &Model) -> Command<Event, Effect> {
    if model.stats.is_some() && model.notifications.is_some() && model.recent_activity.is_some() {
        model.is_loading = false;
        Command::effect(Effect::UpdateDashboardUI)
    } else {
        Command::none()
    }
}
```

**Why the new approach is better:**
- ✅ **Simpler**: Each event handles one concern
- ✅ **More resilient**: Partial failures are handled naturally  
- ✅ **Better UX**: UI updates as data becomes available
- ✅ **Easier testing**: Each event can be tested independently
- ✅ **More flexible**: Easy to add loading states, retries, etc.
```

### User Authentication Flow

**Unidirectional Approach:**
```rust
Event::Login { credentials } => {
    Command::effect(Effect::HttpRequest {
        url: "/api/login".to_string(),
        method: "POST".to_string(),
        body: Some(serde_json::to_string(&credentials).unwrap()),
    })
}

Event::LoginSuccess { token, user } => {
    model.token = Some(token);
    model.current_user = Some(user);
    // Load preferences as separate effect
    Command::effect(Effect::HttpRequest {
        url: "/api/preferences".to_string(),
        headers: vec![("Authorization".to_string(), token.clone())],
    })
}

Event::PreferencesLoaded { preferences } => {
    model.preferences = Some(preferences);
    Command::event(Event::NavigateToDashboard)
}

Event::LoginFailed { error } => {
    model.error_message = Some(error);
    Command::none()
}
```

**Bidirectional Approach:**
```rust
Event::Login { credentials } => {
    Http::post("/api/login")
        .json(&credentials)
        .build()
        .then_request(|login_result| {
            match login_result {
                Ok(login_response) => {
                    Http::get("/api/preferences")
                        .header("Authorization", &login_response.token)
                        .build()
                }
                Err(_) => NoOpRequest::new() // Awkward error handling
            }
        })
        .then_send(|prefs_result| {
            // Now we need to somehow combine login_result and prefs_result
            // But login_result is not available here!
            Event::LoginComplete { /* ??? */ }
        })
}
```

## Migration Strategy

For teams coming from bidirectional patterns:

### 1. Identify Request Chains
Look for patterns like:
```rust
request1.then_request(|result1| {
    request2(result1).then_request(|result2| {
        request3(result1, result2)
    })
})
```

### 2. Break Into Events
Convert each step to an event:
```rust
// Step 1
Event::StartFlow => Command::effect(Effect1)

// Step 2  
Event::Step1Complete { result1 } => Command::effect(Effect2::new(result1))

// Step 3
Event::Step2Complete { result2 } => Command::effect(Effect3::new(result1, result2))
```

### 3. Store Intermediate State
Use the model to store intermediate results:
```rust
struct Model {
    flow_state: Option<FlowState>,
}

struct FlowState {
    step1_result: Option<Result1>,
    step2_result: Option<Result2>,
}
```

## Benefits Achieved

✅ **Simplicity**: Each event handler is simple and focused  
✅ **Testability**: Easy to test each step in isolation  
✅ **Debuggability**: Clear event flow through the system  
✅ **Scalability**: Effects can run in parallel naturally  
✅ **Reliability**: No complex coordination or race conditions  
✅ **Maintainability**: Easy to add new effects and events  
✅ **Performance**: No blocking, minimal overhead  

## Conclusion

The unidirectional architecture in Syzygy provides significant benefits over bidirectional request/response patterns:

1. **Simpler mental model** - Events flow one way through the system
2. **Easier testing** - Each step can be tested independently  
3. **Better error handling** - Errors are just events like anything else
4. **Natural parallelism** - Effects execute independently
5. **No race conditions** - No complex coordination required
6. **Better scalability** - Easy to add more effects and handlers

While bidirectional patterns can seem more powerful at first, they introduce significant complexity that outweighs their benefits. The unidirectional approach in Syzygy strikes the right balance between simplicity and capability, making it easier to build and maintain robust applications.

//! Benchmark: Syzygy Event Handler Approaches
//!
//! This benchmark compares multiple dispatch approaches for Syzygy event handling:
//! 1. Single big match function (baseline - static dispatch)
//! 2. HashMap<TypeId, fn> with function pointers
//! 3. EnumMap for array-based dispatch
//! 4. EventMap with zero-overhead dispatch
//!
//! All approaches:
//! - Use the same SyzygyEvent enum and Event types
//! - Perform identical work (increment processed_events + update counter)
//! - Use fair iteration patterns (references for most, cloning only where required)
//! - No unfair allocation overhead
//!
//! EventMap legitimately requires owned values for zero-copy dispatch,
//! so it uses .iter().cloned() which shows the real usage cost/benefit tradeoff.

#![feature(downcast_unchecked)]

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use enum_map::{Enum, EnumMap};
use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};
use std::marker::PhantomData;
use syzygy::dispatch::Dispatch;
use syzygy::event_map::EventMapBuilder;
use syzygy_macros::Event;

// ============================================================================
// Common Types and Model - Syzygy Style
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
enum SyzygyCommand {
    Log { message: String },
    Save { id: u64 },
}

// Event types for testing - realistic Syzygy events with minimal payload
#[derive(Clone, Debug)]
struct Event1 {
    id: u32,
}

#[derive(Clone, Debug)]
struct Event2 {
    value: u64,
}

#[derive(Clone, Debug)]
struct Event3 {
    flag: bool,
}

#[derive(Clone, Debug)]
struct Event4 {
    count: u16,
}

#[derive(Clone, Debug)]
struct Event5 {
    index: usize,
}

// Main event enum for match-based dispatch
#[derive(Clone, Debug, Event)]
enum SyzygyEvent {
    Event1(Event1),
    Event2(Event2),
    Event3(Event3),
    Event4(Event4),
    Event5(Event5),
}

// We use Syzygy's Dispatch directly - no custom result type needed

// ============================================================================
// Method 1: Single Big Match Function (Current Syzygy Approach)
// ============================================================================

fn syzygy_match_handler(
    event: SyzygyEvent,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;

    match event {
        SyzygyEvent::Event1(e) => {
            model.counter += e.id as u64;
            Dispatch::none()
        }
        SyzygyEvent::Event2(e) => {
            model.counter += e.value;
            Dispatch::none()
        }
        SyzygyEvent::Event3(_) => {
            model.counter += 3;
            Dispatch::none()
        }
        SyzygyEvent::Event4(e) => {
            model.counter += e.count as u64;
            Dispatch::none()
        }
        SyzygyEvent::Event5(e) => {
            model.counter += e.index as u64;
            Dispatch::none()
        }
    }
}

// ============================================================================
// Method 2: FxHashMap<TypeId, fn> with Unboxed Function Pointers
// ============================================================================

// Properly typed handler functions that get boxed in the dispatcher
fn typed_handle_event1(
    event: Event1,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += event.id as u64;
    Dispatch::none()
}

fn typed_handle_event2(
    event: Event2,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += event.value;
    Dispatch::none()
}

fn typed_handle_event3(
    _event: Event3,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += 3;
    Dispatch::none()
}

fn typed_handle_event4(
    event: Event4,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += event.count as u64;
    Dispatch::none()
}

fn typed_handle_event5(
    event: Event5,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += event.index as u64;
    Dispatch::none()
}

// Fair TypeId dispatcher working with proper SyzygyEvent variants
type BoxedHandlerFn = Box<dyn Fn(SyzygyEvent, &mut UnifiedModel) -> Dispatch<SyzygyEvent, SyzygyCommand>>;

struct SyzygyTypeIdDispatcher {
    handlers: FxHashMap<TypeId, BoxedHandlerFn>,
}

impl SyzygyTypeIdDispatcher {
    fn new() -> Self {
        Self {
            handlers: FxHashMap::default(),
        }
    }

    fn on<T: 'static + Clone>(
        &mut self,
        handler: fn(T, &mut UnifiedModel) -> Dispatch<SyzygyEvent, SyzygyCommand>,
    ) where
        T: Any,
    {
        let boxed_handler = Box::new(move |event: SyzygyEvent, model: &mut UnifiedModel| {
            // Extract the inner event data from the SyzygyEvent variant
            let inner_data = match TypeId::of::<T>() {
                id if id == TypeId::of::<Event1>() => {
                    if let SyzygyEvent::Event1(ref data) = event {
                        unsafe { &*(data as *const Event1 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event2>() => {
                    if let SyzygyEvent::Event2(ref data) = event {
                        unsafe { &*(data as *const Event2 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event3>() => {
                    if let SyzygyEvent::Event3(ref data) = event {
                        unsafe { &*(data as *const Event3 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event4>() => {
                    if let SyzygyEvent::Event4(ref data) = event {
                        unsafe { &*(data as *const Event4 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event5>() => {
                    if let SyzygyEvent::Event5(ref data) = event {
                        unsafe { &*(data as *const Event5 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                _ => return Dispatch::none(),
            };
            
            handler(inner_data.clone(), model)
        });
        self.handlers.insert(TypeId::of::<T>(), boxed_handler);
    }

    fn dispatch(
        &self,
        event: SyzygyEvent,
        model: &mut UnifiedModel,
    ) -> Dispatch<SyzygyEvent, SyzygyCommand> {
        let type_id = event.inner_type_id();

        if let Some(handler) = self.handlers.get(&type_id) {
            handler(event, model)
        } else {
            Dispatch::none()
        }
    }
}

// Array indexing approach removed - not dynamic for library use

// ============================================================================
// Unified Test Data Generation
// ============================================================================

// Unified model for all benchmarks
#[derive(Default)]
struct UnifiedModel {
    counter: u64,
    processed_events: u64,
}

// Common event generation that all benchmark methods will use
fn generate_unified_events(count: usize, pattern: &str) -> Vec<SyzygyEvent> {
    let mut events = Vec::with_capacity(count);
    let mut rng_state: u64 = 1; // Fixed seed for reproducibility

    for i in 0..count {
        let event_type = match pattern {
            "sequential" => i % 5, // Use only first 5 event types for consistency
            "random" => {
                rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
                (rng_state % 5) as usize
            }
            _ => panic!("Unknown pattern: {}", pattern),
        };

        let event = match event_type {
            0 => SyzygyEvent::Event1(Event1 { id: i as u32 }),
            1 => SyzygyEvent::Event2(Event2 { value: i as u64 }),
            2 => SyzygyEvent::Event3(Event3 { flag: (i % 2) == 0 }),
            3 => SyzygyEvent::Event4(Event4 {
                count: (i % 1000) as u16,
            }),
            4 => SyzygyEvent::Event5(Event5 { index: i }),
            _ => unreachable!(),
        };

        events.push(event);
    }

    events
}

// ============================================================================
// EventMap Handler Functions for SyzygyEvent
// ============================================================================

// Typed handlers for EventMap (take inner types directly)
fn handle_map_event1(
    data: Event1,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += data.id as u64;
    Dispatch::none()
}

fn handle_map_event2(
    data: Event2,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += data.value;
    Dispatch::none()
}

fn handle_map_event3(
    _data: Event3,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += 3;
    Dispatch::none()
}

fn handle_map_event4(
    data: Event4,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += data.count as u64;
    Dispatch::none()
}

fn handle_map_event5(
    data: Event5,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += data.index as u64;
    Dispatch::none()
}

// Handler functions for EventMap (take owned values - the key difference!)
fn handle_unsafe_event1(
    data: Event1,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += data.id as u64;
    Dispatch::none()
}

fn handle_unsafe_event2(
    data: Event2,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += data.value;
    Dispatch::none()
}

fn handle_unsafe_event3(
    _data: Event3,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += 3;
    Dispatch::none()
}

fn handle_unsafe_event4(
    data: Event4,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += data.count as u64;
    Dispatch::none()
}

fn handle_unsafe_event5(
    data: Event5,
    model: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    model.processed_events += 1;
    model.counter += data.index as u64;
    Dispatch::none()
}

// ============================================================================
// Additional Benchmark Implementations - EnumMap vs TypeId with SyzygyEvent
// ============================================================================

// --- Approach 1: EnumMap Dispatcher using SyzygyEvent ---

// Event type for enum-map indexing (unit variants only)
#[derive(Debug, Clone, Copy, Enum)]
enum SyzygyEventType {
    Event1,
    Event2,
    Event3,
    Event4,
    Event5,
}

impl SyzygyEvent {
    fn event_type(&self) -> SyzygyEventType {
        match self {
            SyzygyEvent::Event1(_) => SyzygyEventType::Event1,
            SyzygyEvent::Event2(_) => SyzygyEventType::Event2,
            SyzygyEvent::Event3(_) => SyzygyEventType::Event3,
            SyzygyEvent::Event4(_) => SyzygyEventType::Event4,
            SyzygyEvent::Event5(_) => SyzygyEventType::Event5,
        }
    }
}

// Fair EnumMap dispatcher with proper variant type handling
type BoxedEnumHandler<M, C> = Box<dyn Fn(SyzygyEvent, &mut M) -> Dispatch<SyzygyEvent, C>>;

pub struct SyzygyEnumMapDispatcher<M, C> {
    handlers: EnumMap<SyzygyEventType, Option<BoxedEnumHandler<M, C>>>,
    _phantom: PhantomData<(M, C)>,
}

impl<M: 'static, C: 'static> SyzygyEnumMapDispatcher<M, C> {
    pub fn new() -> Self {
        Self {
            handlers: EnumMap::default(),
            _phantom: PhantomData,
        }
    }

    pub fn on<T: 'static + Clone>(
        &mut self,
        variant: SyzygyEventType,
        handler: fn(T, &mut M) -> Dispatch<SyzygyEvent, C>,
    ) where
        T: Any,
    {
        let boxed_handler = Box::new(move |event: SyzygyEvent, model: &mut M| {
            // Extract the inner variant data from SyzygyEvent
            let inner_data = match TypeId::of::<T>() {
                id if id == TypeId::of::<Event1>() => {
                    if let SyzygyEvent::Event1(ref data) = event {
                        unsafe { &*(data as *const Event1 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event2>() => {
                    if let SyzygyEvent::Event2(ref data) = event {
                        unsafe { &*(data as *const Event2 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event3>() => {
                    if let SyzygyEvent::Event3(ref data) = event {
                        unsafe { &*(data as *const Event3 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event4>() => {
                    if let SyzygyEvent::Event4(ref data) = event {
                        unsafe { &*(data as *const Event4 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event5>() => {
                    if let SyzygyEvent::Event5(ref data) = event {
                        unsafe { &*(data as *const Event5 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                _ => return Dispatch::none(),
            };
            
            handler(inner_data.clone(), model)
        });
        self.handlers[variant] = Some(boxed_handler);
    }

    #[inline(always)]
    pub fn dispatch(&self, event: SyzygyEvent, model: &mut M) -> Dispatch<SyzygyEvent, C> {
        if let Some(handler) = &self.handlers[event.event_type()] {
            handler(event, model)
        } else {
            Dispatch::none()
        }
    }
}

// --- Approach 2: TypeId Dispatcher using SyzygyEvent (Specific Implementation) ---

impl SyzygyEvent {
    fn inner_type_id(&self) -> TypeId {
        match self {
            SyzygyEvent::Event1(_) => TypeId::of::<Event1>(),
            SyzygyEvent::Event2(_) => TypeId::of::<Event2>(),
            SyzygyEvent::Event3(_) => TypeId::of::<Event3>(),
            SyzygyEvent::Event4(_) => TypeId::of::<Event4>(),
            SyzygyEvent::Event5(_) => TypeId::of::<Event5>(),
        }
    }

    fn inner_as_any(&self) -> &dyn Any {
        match self {
            SyzygyEvent::Event1(e) => e,
            SyzygyEvent::Event2(e) => e,
            SyzygyEvent::Event3(e) => e,
            SyzygyEvent::Event4(e) => e,
            SyzygyEvent::Event5(e) => e,
        }
    }
}

// Fair TypeId dispatcher Alt with proper SyzygyEvent handling
type BoxedHandlerFnAlt<M, C> = Box<dyn Fn(SyzygyEvent, &mut M) -> Dispatch<SyzygyEvent, C>>;

pub struct SyzygyTypeIdDispatcherAlt<M, C> {
    handlers: FxHashMap<TypeId, BoxedHandlerFnAlt<M, C>>,
    _phantom: PhantomData<(M, C)>,
}

impl<M: 'static, C: 'static> SyzygyTypeIdDispatcherAlt<M, C> {
    pub fn new() -> Self {
        Self {
            handlers: FxHashMap::default(),
            _phantom: PhantomData,
        }
    }

    pub fn on<T: 'static + Clone>(
        &mut self,
        handler: fn(T, &mut M) -> Dispatch<SyzygyEvent, C>,
    ) where
        T: Any,
    {
        let boxed_handler = Box::new(move |event: SyzygyEvent, model: &mut M| {
            // Extract the inner event data from the SyzygyEvent variant
            let inner_data = match TypeId::of::<T>() {
                id if id == TypeId::of::<Event1>() => {
                    if let SyzygyEvent::Event1(ref data) = event {
                        unsafe { &*(data as *const Event1 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event2>() => {
                    if let SyzygyEvent::Event2(ref data) = event {
                        unsafe { &*(data as *const Event2 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event3>() => {
                    if let SyzygyEvent::Event3(ref data) = event {
                        unsafe { &*(data as *const Event3 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event4>() => {
                    if let SyzygyEvent::Event4(ref data) = event {
                        unsafe { &*(data as *const Event4 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event5>() => {
                    if let SyzygyEvent::Event5(ref data) = event {
                        unsafe { &*(data as *const Event5 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                _ => return Dispatch::none(),
            };
            
            handler(inner_data.clone(), model)
        });
        self.handlers.insert(TypeId::of::<T>(), boxed_handler);
    }

    #[inline]
    pub fn dispatch(&self, event: SyzygyEvent, model: &mut M) -> Dispatch<SyzygyEvent, C> {
        let type_id = event.inner_type_id();
        if let Some(handler) = self.handlers.get(&type_id) {
            handler(event, model)
        } else {
            Dispatch::none()
        }
    }
}

// --- Approach 3: "Good DX" TypeId Trait Dispatcher (Generic Library Implementation) ---

/// Trait that event enums must implement for the Good DX TypeId dispatcher
pub trait EventVariant {
    fn inner_type_id(&self) -> TypeId;
}

// Fair GoodDx TypeId dispatcher with proper SyzygyEvent handling
type BoxedHandlerFnGoodDx<E, M, C> = Box<dyn Fn(E, &mut M) -> Dispatch<E, C>>;

/// Generic TypeId dispatcher that works with any event type implementing EventVariant
pub struct GoodDxTypeIdDispatcher<E, M, C> {
    handlers: FxHashMap<TypeId, BoxedHandlerFnGoodDx<E, M, C>>,
    _phantom: PhantomData<(E, M, C)>,
}

impl<E: 'static, M: 'static, C: 'static> GoodDxTypeIdDispatcher<E, M, C>
{
    pub fn new() -> Self {
        Self {
            handlers: FxHashMap::default(),
            _phantom: PhantomData,
        }
    }

    pub fn on<T: 'static + Clone>(
        &mut self,
        handler: fn(T, &mut M) -> Dispatch<SyzygyEvent, SyzygyCommand>,
    ) where
        T: Any,
    {
        let boxed_handler = Box::new(move |event: SyzygyEvent, model: &mut M| {
            let inner_data = match TypeId::of::<T>() {
                id if id == TypeId::of::<Event1>() => {
                    if let SyzygyEvent::Event1(ref data) = event {
                        unsafe { &*(data as *const Event1 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event2>() => {
                    if let SyzygyEvent::Event2(ref data) = event {
                        unsafe { &*(data as *const Event2 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event3>() => {
                    if let SyzygyEvent::Event3(ref data) = event {
                        unsafe { &*(data as *const Event3 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event4>() => {
                    if let SyzygyEvent::Event4(ref data) = event {
                        unsafe { &*(data as *const Event4 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                id if id == TypeId::of::<Event5>() => {
                    if let SyzygyEvent::Event5(ref data) = event {
                        unsafe { &*(data as *const Event5 as *const T) }
                    } else {
                        return Dispatch::none();
                    }
                }
                _ => return Dispatch::none(),
            };
            
            handler(inner_data.clone(), model)
        });
        // Cast to store in the generic handler map
        let generic_handler = unsafe {
            std::mem::transmute::<
                BoxedHandlerFnGoodDx<SyzygyEvent, M, SyzygyCommand>,
                BoxedHandlerFnGoodDx<E, M, C>
            >(boxed_handler)
        };
        self.handlers.insert(TypeId::of::<T>(), generic_handler);
    }

    /// The key method: generic dispatch that works with any EventVariant
    #[inline]
    pub fn dispatch(&self, event: SyzygyEvent, model: &mut M) -> Dispatch<SyzygyEvent, SyzygyCommand> {
        let type_id = event.inner_type_id();
        if let Some(handler) = self.handlers.get(&type_id) {
            // Cast the handler to the specific type we need for SyzygyEvent
            let specific_handler = unsafe {
                std::mem::transmute::<
                    &BoxedHandlerFnGoodDx<E, M, C>,
                    &BoxedHandlerFnGoodDx<SyzygyEvent, M, SyzygyCommand>
                >(handler)
            };
            specific_handler(event, model)
        } else {
            Dispatch::none()
        }
    }
}

// User implements EventVariant for their event enum (this is what users would do)
impl EventVariant for SyzygyEvent {
    fn inner_type_id(&self) -> TypeId {
        match self {
            SyzygyEvent::Event1(_) => TypeId::of::<Event1>(),
            SyzygyEvent::Event2(_) => TypeId::of::<Event2>(),
            SyzygyEvent::Event3(_) => TypeId::of::<Event3>(),
            SyzygyEvent::Event4(_) => TypeId::of::<Event4>(),
            SyzygyEvent::Event5(_) => TypeId::of::<Event5>(),
        }
    }
}


// --- Handlers for EnumMap using SyzygyEvent (owned values) ---
fn handle_syzygy_e1_enum(
    e: SyzygyEvent,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    if let SyzygyEvent::Event1(v) = e {
        m.counter += v.id as u64;
    }
    Dispatch::none()
}

fn handle_syzygy_e2_enum(
    e: SyzygyEvent,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    if let SyzygyEvent::Event2(v) = e {
        m.counter += v.value;
    }
    Dispatch::none()
}

fn handle_syzygy_e3_enum(
    _e: SyzygyEvent,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    m.counter += 3;
    Dispatch::none()
}

fn handle_syzygy_e4_enum(
    e: SyzygyEvent,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    if let SyzygyEvent::Event4(v) = e {
        m.counter += v.count as u64;
    }
    Dispatch::none()
}

fn handle_syzygy_e5_enum(
    e: SyzygyEvent,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    if let SyzygyEvent::Event5(v) = e {
        m.counter += v.index as u64;
    }
    Dispatch::none()
}

// --- Handlers for TypeId using SyzygyEvent ---
fn handle_syzygy_e1_typeid(
    d: &dyn Any,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    let v = unsafe { d.downcast_ref_unchecked::<Event1>() };
    m.counter += v.id as u64;
    Dispatch::none()
}

fn handle_syzygy_e2_typeid(
    d: &dyn Any,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    let v = unsafe { d.downcast_ref_unchecked::<Event2>() };
    m.counter += v.value;
    Dispatch::none()
}

fn handle_syzygy_e3_typeid(
    _d: &dyn Any,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    m.counter += 3;
    Dispatch::none()
}

fn handle_syzygy_e4_typeid(
    d: &dyn Any,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    let v = unsafe { d.downcast_ref_unchecked::<Event4>() };
    m.counter += v.count as u64;
    Dispatch::none()
}

fn handle_syzygy_e5_typeid(
    d: &dyn Any,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    let v = unsafe { d.downcast_ref_unchecked::<Event5>() };
    m.counter += v.index as u64;
    Dispatch::none()
}

// --- Handlers for Good DX TypeId Dispatcher ---
fn handle_gooddx_e1_typeid(
    d: &dyn Any,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    let v = unsafe { d.downcast_ref_unchecked::<Event1>() };
    m.counter += v.id as u64;
    Dispatch::none()
}

fn handle_gooddx_e2_typeid(
    d: &dyn Any,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    let v = unsafe { d.downcast_ref_unchecked::<Event2>() };
    m.counter += v.value;
    Dispatch::none()
}

fn handle_gooddx_e3_typeid(
    _d: &dyn Any,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    m.counter += 3;
    Dispatch::none()
}

fn handle_gooddx_e4_typeid(
    d: &dyn Any,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    let v = unsafe { d.downcast_ref_unchecked::<Event4>() };
    m.counter += v.count as u64;
    Dispatch::none()
}

fn handle_gooddx_e5_typeid(
    d: &dyn Any,
    m: &mut UnifiedModel,
) -> Dispatch<SyzygyEvent, SyzygyCommand> {
    m.processed_events += 1;
    let v = unsafe { d.downcast_ref_unchecked::<Event5>() };
    m.counter += v.index as u64;
    Dispatch::none()
}

// ============================================================================
// Benchmark Functions
// ============================================================================

fn benchmark_dispatch_methods(c: &mut Criterion) {
    let mut group = c.benchmark_group("dispatch_methods_1000_events");

    let event_count = 1000;
    let sequential_events = generate_unified_events(event_count, "sequential");
    let random_events = generate_unified_events(event_count, "random");

    // Benchmark all dispatch methods for both patterns
    for (pattern_name, events) in [
        ("sequential", &sequential_events),
        ("random", &random_events),
    ] {
        // --- Match dispatch ---
        group.bench_with_input(
            BenchmarkId::new("match", pattern_name),
            &events,
            |b, events| {
                b.iter(|| {
                    let mut model = UnifiedModel::default();

                    for event in events.iter().cloned() {
                        let _result = syzygy_match_handler(black_box(event), black_box(&mut model));
                    }

                    black_box(model);
                });
            },
        );

        // --- TypeId HashMap dispatch ---
        let mut typeid_dispatcher = SyzygyTypeIdDispatcher::new();
        typeid_dispatcher.on::<Event1>(typed_handle_event1);
        typeid_dispatcher.on::<Event2>(typed_handle_event2);
        typeid_dispatcher.on::<Event3>(typed_handle_event3);
        typeid_dispatcher.on::<Event4>(typed_handle_event4);
        typeid_dispatcher.on::<Event5>(typed_handle_event5);

        group.bench_with_input(
            BenchmarkId::new("typeid_hashmap", pattern_name),
            &events,
            |b, events| {
                b.iter(|| {
                    let mut model = UnifiedModel::default();

                    for event in events.iter().cloned() {
                        // TypeId HashMap now takes owned events
                        let _result =
                            typeid_dispatcher.dispatch(black_box(event), black_box(&mut model));
                    }

                    black_box(model);
                });
            },
        );

        // --- EnumMap dispatch ---
        let mut enum_dispatcher = SyzygyEnumMapDispatcher::new();
        enum_dispatcher.on::<Event1>(SyzygyEventType::Event1, typed_handle_event1);
        enum_dispatcher.on::<Event2>(SyzygyEventType::Event2, typed_handle_event2);
        enum_dispatcher.on::<Event3>(SyzygyEventType::Event3, typed_handle_event3);
        enum_dispatcher.on::<Event4>(SyzygyEventType::Event4, typed_handle_event4);
        enum_dispatcher.on::<Event5>(SyzygyEventType::Event5, typed_handle_event5);

        group.bench_with_input(
            BenchmarkId::new("enummap", pattern_name),
            &events,
            |b, evts| {
                b.iter(|| {
                    let mut model = UnifiedModel::default();
                    for event in evts.iter().cloned() {
                        // EnumMap now takes owned events
                        enum_dispatcher.dispatch(black_box(event), black_box(&mut model));
                    }
                    black_box(model);
                });
            },
        );

        // --- TypeId Alt dispatch ---
        let mut typeid_alt_dispatcher = SyzygyTypeIdDispatcherAlt::new();
        typeid_alt_dispatcher.on::<Event1>(typed_handle_event1);
        typeid_alt_dispatcher.on::<Event2>(typed_handle_event2);
        typeid_alt_dispatcher.on::<Event3>(typed_handle_event3);
        typeid_alt_dispatcher.on::<Event4>(typed_handle_event4);
        typeid_alt_dispatcher.on::<Event5>(typed_handle_event5);

        group.bench_with_input(
            BenchmarkId::new("typeid_alt", pattern_name),
            &events,
            |b, evts| {
                b.iter(|| {
                    let mut model = UnifiedModel::default();
                    for event in evts.iter().cloned() {
                        // TypeId Alt now takes owned events
                        typeid_alt_dispatcher.dispatch(black_box(event), black_box(&mut model));
                    }
                    black_box(model);
                });
            },
        );

        // --- Good DX TypeId dispatch ---
        let mut gooddx_dispatcher: GoodDxTypeIdDispatcher<SyzygyEvent, UnifiedModel, SyzygyCommand> = GoodDxTypeIdDispatcher::new();
        gooddx_dispatcher.on::<Event1>(typed_handle_event1);
        gooddx_dispatcher.on::<Event2>(typed_handle_event2);
        gooddx_dispatcher.on::<Event3>(typed_handle_event3);
        gooddx_dispatcher.on::<Event4>(typed_handle_event4);
        gooddx_dispatcher.on::<Event5>(typed_handle_event5);

        group.bench_with_input(
            BenchmarkId::new("typeid_gooddx", pattern_name),
            &events,
            |b, evts| {
                b.iter(|| {
                    let mut model = UnifiedModel::default();
                    for event in evts.iter().cloned() {
                        // Good DX TypeId now takes owned events
                        gooddx_dispatcher.dispatch(black_box(event), black_box(&mut model));
                    }
                    black_box(model);
                });
            },
        );

        // --- EventMap with automatic variant indexing ---
        let event_map = EventMapBuilder::<SyzygyEvent, UnifiedModel, SyzygyCommand>::new()
            .on::<Event1>(handle_map_event1) // Variant index automatically determined
            .on::<Event2>(handle_map_event2)
            .on::<Event3>(handle_map_event3)
            .on::<Event4>(handle_map_event4)
            .on::<Event5>(handle_map_event5)
            .build();

        group.bench_with_input(
            BenchmarkId::new("eventmap_derive", pattern_name),
            &events,
            |b, evts| {
                b.iter(|| {
                    let mut model = UnifiedModel::default();
                    for event in evts.iter().cloned() {
                        // EventMap now takes owned events
                        unsafe { event_map.dispatch(black_box(event), black_box(&mut model)) };
                    }
                    black_box(model);
                });
            },
        );

        // --- EventMap with maximum performance ---
        // Note: EventMap requires owned values, so we must clone events for fair comparison
        let unsafe_event_map = EventMapBuilder::<SyzygyEvent, UnifiedModel, SyzygyCommand>::new()
            .on::<Event1>(handle_unsafe_event1)
            .on::<Event2>(handle_unsafe_event2)
            .on::<Event3>(handle_unsafe_event3)
            .on::<Event4>(handle_unsafe_event4)
            .on::<Event5>(handle_unsafe_event5)
            .build();

        group.bench_with_input(
            BenchmarkId::new("unsafe_eventmap", pattern_name),
            &events,
            |b, evts| {
                b.iter(|| {
                    let mut model = UnifiedModel::default();
                    for event in evts.iter().cloned() {
                        // EventMap consumes the event - shows real usage cost
                        unsafe {
                            unsafe_event_map.dispatch(black_box(event), black_box(&mut model));
                        }
                    }
                    black_box(model);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_dispatch_methods);
criterion_main!(benches);

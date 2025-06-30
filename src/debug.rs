//! Debugging and tracing utilities for Syzygy
//!
//! This module provides debugging tools to help understand what's happening
//! in your Syzygy application, including effect tracing and metrics.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock, atomic::{AtomicU64, Ordering}},
    time::{Duration, Instant},
};

/// Unique identifier for effects
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EffectId(u64);

impl EffectId {
    pub fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl std::fmt::Display for EffectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "effect-{}", self.0)
    }
}

/// Information about an effect execution
#[derive(Debug, Clone)]
pub struct EffectTrace {
    pub id: EffectId,
    pub name: String,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub status: EffectStatus,
}

/// Status of an effect execution
#[derive(Debug, Clone)]
pub enum EffectStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

impl EffectTrace {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: EffectId::next(),
            name: name.into(),
            start_time: Instant::now(),
            end_time: None,
            status: EffectStatus::Pending,
        }
    }

    pub fn start(&mut self) {
        self.status = EffectStatus::Running;
        self.start_time = Instant::now();
    }

    pub fn complete(&mut self) {
        self.status = EffectStatus::Completed;
        self.end_time = Some(Instant::now());
    }

    pub fn fail(&mut self, reason: impl Into<String>) {
        self.status = EffectStatus::Failed(reason.into());
        self.end_time = Some(Instant::now());
    }

    pub fn duration(&self) -> Option<Duration> {
        self.end_time.map(|end| end.duration_since(self.start_time))
    }
}

/// Debug tracer for effects
#[derive(Debug, Clone)]
pub struct EffectTracer {
    traces: Arc<Mutex<VecDeque<EffectTrace>>>,
    max_traces: usize,
    enabled: bool,
}

impl Default for EffectTracer {
    fn default() -> Self {
        Self::new(1000) // Keep last 1000 traces by default
    }
}

impl EffectTracer {
    pub fn new(max_traces: usize) -> Self {
        Self {
            traces: Arc::new(Mutex::new(VecDeque::with_capacity(max_traces))),
            max_traces,
            enabled: false,
        }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn trace_effect(&self, name: impl Into<String>) -> Option<EffectTrace> {
        if !self.enabled {
            return None;
        }

        let trace = EffectTrace::new(name);
        
        if let Ok(mut traces) = self.traces.lock() {
            if traces.len() >= self.max_traces {
                traces.pop_front();
            }
            traces.push_back(trace.clone());
        }

        Some(trace)
    }

    pub fn update_trace(&self, trace: EffectTrace) {
        if !self.enabled {
            return;
        }

        if let Ok(mut traces) = self.traces.lock() {
            if let Some(existing) = traces.iter_mut().find(|t| t.id == trace.id) {
                *existing = trace;
            }
        }
    }

    pub fn get_recent_traces(&self, count: usize) -> Vec<EffectTrace> {
        if let Ok(traces) = self.traces.lock() {
            traces.iter().rev().take(count).cloned().collect()
        } else {
            Vec::new()
        }
    }

    pub fn get_all_traces(&self) -> Vec<EffectTrace> {
        if let Ok(traces) = self.traces.lock() {
            traces.iter().cloned().collect()
        } else {
            Vec::new()
        }
    }

    pub fn clear(&self) {
        if let Ok(mut traces) = self.traces.lock() {
            traces.clear();
        }
    }

    pub fn print_summary(&self) {
        let traces = self.get_all_traces();
        
        println!("Effect Execution Summary:");
        println!("========================");
        println!("Total effects: {}", traces.len());
        
        let completed: Vec<_> = traces.iter()
            .filter(|t| matches!(t.status, EffectStatus::Completed))
            .collect();
        
        let failed: Vec<_> = traces.iter()
            .filter(|t| matches!(t.status, EffectStatus::Failed(_)))
            .collect();

        println!("Completed: {}", completed.len());
        println!("Failed: {}", failed.len());

        if !completed.is_empty() {
            let total_duration: Duration = completed.iter()
                .filter_map(|t| t.duration())
                .sum();
            
            let avg_duration = total_duration / completed.len() as u32;
            println!("Average duration: {:?}", avg_duration);
        }

        println!("\nRecent effects:");
        for trace in traces.iter().rev().take(10) {
            let duration = trace.duration()
                .map(|d| format!("{:?}", d))
                .unwrap_or_else(|| "running".to_string());
            
            println!("  {} [{}]: {:?} ({})", 
                trace.id, trace.name, trace.status, duration);
        }
    }
}

/// Metrics collector for Syzygy operations
#[derive(Debug)]
pub struct SyzygyMetrics {
    effects_dispatched: AtomicU64,
    effects_processed: AtomicU64,
    effects_failed: AtomicU64,
    resource_accesses: AtomicU64,
    model_updates: AtomicU64,
    start_time: Instant,
}

impl Default for SyzygyMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl SyzygyMetrics {
    pub fn new() -> Self {
        Self {
            effects_dispatched: AtomicU64::new(0),
            effects_processed: AtomicU64::new(0),
            effects_failed: AtomicU64::new(0),
            resource_accesses: AtomicU64::new(0),
            model_updates: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    pub fn effect_dispatched(&self) {
        self.effects_dispatched.fetch_add(1, Ordering::SeqCst);
    }

    pub fn effect_processed(&self) {
        self.effects_processed.fetch_add(1, Ordering::SeqCst);
    }

    pub fn effect_failed(&self) {
        self.effects_failed.fetch_add(1, Ordering::SeqCst);
    }

    pub fn resource_accessed(&self) {
        self.resource_accesses.fetch_add(1, Ordering::SeqCst);
    }

    pub fn model_updated(&self) {
        self.model_updates.fetch_add(1, Ordering::SeqCst);
    }

    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn effects_per_second(&self) -> f64 {
        let processed = self.effects_processed.load(Ordering::SeqCst) as f64;
        let uptime_secs = self.uptime().as_secs_f64();
        if uptime_secs > 0.0 {
            processed / uptime_secs
        } else {
            0.0
        }
    }

    pub fn print_stats(&self) {
        let uptime = self.uptime();
        println!("Syzygy Metrics:");
        println!("==============");
        println!("Uptime: {:?}", uptime);
        println!("Effects dispatched: {}", self.effects_dispatched.load(Ordering::SeqCst));
        println!("Effects processed: {}", self.effects_processed.load(Ordering::SeqCst));
        println!("Effects failed: {}", self.effects_failed.load(Ordering::SeqCst));
        println!("Resource accesses: {}", self.resource_accesses.load(Ordering::SeqCst));
        println!("Model updates: {}", self.model_updates.load(Ordering::SeqCst));
        println!("Effects per second: {:.2}", self.effects_per_second());
    }

    pub fn reset(&self) {
        self.effects_dispatched.store(0, Ordering::SeqCst);
        self.effects_processed.store(0, Ordering::SeqCst);
        self.effects_failed.store(0, Ordering::SeqCst);
        self.resource_accesses.store(0, Ordering::SeqCst);
        self.model_updates.store(0, Ordering::SeqCst);
    }
}

/// Global debug state using OnceLock for thread safety
static DEBUG_TRACER: OnceLock<Mutex<EffectTracer>> = OnceLock::new();
static DEBUG_METRICS: OnceLock<SyzygyMetrics> = OnceLock::new();

/// Enable global effect tracing
pub fn enable_tracing() {
    let tracer = DEBUG_TRACER.get_or_init(|| Mutex::new(EffectTracer::default()));
    if let Ok(mut tracer) = tracer.lock() {
        tracer.enable();
    }
}

/// Disable global effect tracing
pub fn disable_tracing() {
    if let Some(tracer) = DEBUG_TRACER.get() {
        if let Ok(mut tracer) = tracer.lock() {
            tracer.disable();
        }
    }
}

/// Execute a function with the global tracer
pub fn with_tracer<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&EffectTracer) -> R,
{
    DEBUG_TRACER.get()
        .and_then(|tracer| tracer.lock().ok())
        .map(|tracer| f(&*tracer))
}

/// Enable global metrics collection
pub fn enable_metrics() {
    DEBUG_METRICS.get_or_init(|| SyzygyMetrics::new());
}

/// Get the global metrics collector
pub fn metrics() -> Option<&'static SyzygyMetrics> {
    DEBUG_METRICS.get()
}

/// Print debug summary of current state
pub fn print_debug_summary() {
    println!("Syzygy Debug Summary");
    println!("===================");
    
    if let Some(has_tracer) = with_tracer(|tracer| {
        if tracer.is_enabled() {
            tracer.print_summary();
            true
        } else {
            println!("Effect tracing is disabled");
            false
        }
    }) {
        if !has_tracer {
            // Tracer exists but is disabled
        }
    } else {
        println!("Effect tracing not initialized");
    }

    println!();

    if let Some(metrics) = metrics() {
        metrics.print_stats();
    } else {
        println!("Metrics collection not enabled");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effect_trace() {
        let mut trace = EffectTrace::new("test_effect");
        assert_eq!(trace.name, "test_effect");
        assert!(matches!(trace.status, EffectStatus::Pending));
        assert!(trace.duration().is_none());

        trace.start();
        assert!(matches!(trace.status, EffectStatus::Running));

        trace.complete();
        assert!(matches!(trace.status, EffectStatus::Completed));
        assert!(trace.duration().is_some());
    }

    #[test]
    fn test_effect_tracer() {
        let mut tracer = EffectTracer::new(5);
        assert!(!tracer.is_enabled());

        tracer.enable();
        assert!(tracer.is_enabled());

        // Add some traces
        for i in 0..3 {
            tracer.trace_effect(format!("effect_{}", i));
        }

        let traces = tracer.get_all_traces();
        assert_eq!(traces.len(), 3);
        assert_eq!(traces[0].name, "effect_0");
        assert_eq!(traces[2].name, "effect_2");

        tracer.clear();
        assert_eq!(tracer.get_all_traces().len(), 0);
    }

    #[test] 
    fn test_metrics() {
        let metrics = SyzygyMetrics::new();
        
        metrics.effect_dispatched();
        metrics.effect_processed();
        metrics.resource_accessed();
        
        assert_eq!(metrics.effects_dispatched.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.effects_processed.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.resource_accesses.load(Ordering::SeqCst), 1);
    }
}
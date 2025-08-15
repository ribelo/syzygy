//! Benchmark: Model Storage Approaches
//!
//! This benchmark compares different ways of storing application models:
//! 1. Baseline: Single struct with all fields (no overhead)
//! 2. HashMap<TypeId, Box<dyn Any>>: Standard library HashMap with type erasure
//! 3. FxHashMap<TypeId, Box<dyn Any>>: Faster hash map using rustc-hash
//! 4. RapidHash HashMap: Using rapidhash for even faster hashing
//! 5. HeapLess LinearMap: Stack-allocated map for embedded applications
//!
//! All approaches store 12 different model types and perform 10,000 random reads.

#![feature(downcast_unchecked)]

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rustc_hash::FxHashMap;
use heapless::LinearMap;
use std::any::{Any, TypeId};
use std::collections::HashMap;

// ============================================================================
// Model Types - 12 different models with various data
// ============================================================================

#[derive(Debug, Clone)]
struct Model1 {
    value: u64,
    active: bool,
}

#[derive(Debug, Clone)]
struct Model2 {
    value: u64,
    name: String,
}

#[derive(Debug, Clone)]
struct Model3 {
    value: u64,
    count: i32,
}

#[derive(Debug, Clone)]
struct Model4 {
    value: u64,
    temperature: f32,
}

#[derive(Debug, Clone)]
struct Model5 {
    value: u64,
    items: Vec<String>,
}

#[derive(Debug, Clone)]
struct Model6 {
    value: u64,
    timestamp: u128,
}

#[derive(Debug, Clone)]
struct Model7 {
    value: u64,
    enabled: bool,
    priority: u8,
}

#[derive(Debug, Clone)]
struct Model8 {
    value: u64,
    coordinates: (f64, f64),
}

#[derive(Debug, Clone)]
struct Model9 {
    value: u64,
    status: String,
    retries: u16,
}

#[derive(Debug, Clone)]
struct Model10 {
    value: u64,
    data: Vec<u8>,
}

#[derive(Debug, Clone)]
struct Model11 {
    value: u64,
    config: Option<String>,
}

#[derive(Debug, Clone)]
struct Model12 {
    value: u64,
    metadata: HashMap<String, String>,
}

// Additional models for 32-model benchmark
#[derive(Debug, Clone)]
struct Model13 { value: u64, id: u32 }

#[derive(Debug, Clone)]
struct Model14 { value: u64, score: f64 }

#[derive(Debug, Clone)]
struct Model15 { value: u64, label: String }

#[derive(Debug, Clone)]
struct Model16 { value: u64, flags: Vec<bool> }

#[derive(Debug, Clone)]
struct Model17 { value: u64, weight: f32 }

#[derive(Debug, Clone)]
struct Model18 { value: u64, sequence: u64 }

#[derive(Debug, Clone)]
struct Model19 { value: u64, category: String }

#[derive(Debug, Clone)]
struct Model20 { value: u64, position: (i32, i32) }

#[derive(Debug, Clone)]
struct Model21 { value: u64, state: bool }

#[derive(Debug, Clone)]
struct Model22 { value: u64, duration: u128 }

#[derive(Debug, Clone)]
struct Model23 { value: u64, bytes: Vec<u8> }

#[derive(Debug, Clone)]
struct Model24 { value: u64, ratio: f64 }

#[derive(Debug, Clone)]
struct Model25 { value: u64, tags: Vec<String> }

#[derive(Debug, Clone)]
struct Model26 { value: u64, index: usize }

#[derive(Debug, Clone)]
struct Model27 { value: u64, offset: i64 }

#[derive(Debug, Clone)]
struct Model28 { value: u64, size: u32 }

#[derive(Debug, Clone)]
struct Model29 { value: u64, version: String }

#[derive(Debug, Clone)]
struct Model30 { value: u64, level: u8 }

#[derive(Debug, Clone)]
struct Model31 { value: u64, status_code: u16 }

#[derive(Debug, Clone)]
struct Model32 { value: u64, checksum: u64 }

// ============================================================================
// Baseline Approach - Single struct with all 12 models embedded
// ============================================================================

#[derive(Debug, Clone)]
struct BaselineModels {
    model1: Model1,
    model2: Model2,
    model3: Model3,
    model4: Model4,
    model5: Model5,
    model6: Model6,
    model7: Model7,
    model8: Model8,
    model9: Model9,
    model10: Model10,
    model11: Model11,
    model12: Model12,
}

// ============================================================================
// Baseline Approach - Single struct with all 32 models embedded
// ============================================================================

#[derive(Debug, Clone)]
struct BaselineModels32 {
    model1: Model1, model2: Model2, model3: Model3, model4: Model4,
    model5: Model5, model6: Model6, model7: Model7, model8: Model8,
    model9: Model9, model10: Model10, model11: Model11, model12: Model12,
    model13: Model13, model14: Model14, model15: Model15, model16: Model16,
    model17: Model17, model18: Model18, model19: Model19, model20: Model20,
    model21: Model21, model22: Model22, model23: Model23, model24: Model24,
    model25: Model25, model26: Model26, model27: Model27, model28: Model28,
    model29: Model29, model30: Model30, model31: Model31, model32: Model32,
}

impl BaselineModels {
    fn new() -> Self {
        Self {
            model1: Model1 { value: 1, active: true },
            model2: Model2 { value: 2, name: "test".to_string() },
            model3: Model3 { value: 3, count: 100 },
            model4: Model4 { value: 4, temperature: 20.5 },
            model5: Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] },
            model6: Model6 { value: 6, timestamp: 1234567890 },
            model7: Model7 { value: 7, enabled: false, priority: 3 },
            model8: Model8 { value: 8, coordinates: (10.0, 20.0) },
            model9: Model9 { value: 9, status: "running".to_string(), retries: 0 },
            model10: Model10 { value: 10, data: vec![1, 2, 3, 4] },
            model11: Model11 { value: 11, config: Some("config".to_string()) },
            model12: Model12 { 
                value: 12, 
                metadata: {
                    let mut map = HashMap::new();
                    map.insert("key".to_string(), "value".to_string());
                    map
                }
            },
        }
    }
}

// Extractor trait (like axum extractors)
trait Extract<'a, T> {
    fn extract(&'a self) -> &'a T;
}

// Implement extractor trait for each model type
impl<'a> Extract<'a, Model1> for BaselineModels {
    fn extract(&'a self) -> &'a Model1 {
        &self.model1
    }
}

impl<'a> Extract<'a, Model2> for BaselineModels {
    fn extract(&'a self) -> &'a Model2 {
        &self.model2
    }
}

impl<'a> Extract<'a, Model3> for BaselineModels {
    fn extract(&'a self) -> &'a Model3 {
        &self.model3
    }
}

impl<'a> Extract<'a, Model4> for BaselineModels {
    fn extract(&'a self) -> &'a Model4 {
        &self.model4
    }
}

impl<'a> Extract<'a, Model5> for BaselineModels {
    fn extract(&'a self) -> &'a Model5 {
        &self.model5
    }
}

impl<'a> Extract<'a, Model6> for BaselineModels {
    fn extract(&'a self) -> &'a Model6 {
        &self.model6
    }
}

impl<'a> Extract<'a, Model7> for BaselineModels {
    fn extract(&'a self) -> &'a Model7 {
        &self.model7
    }
}

impl<'a> Extract<'a, Model8> for BaselineModels {
    fn extract(&'a self) -> &'a Model8 {
        &self.model8
    }
}

impl<'a> Extract<'a, Model9> for BaselineModels {
    fn extract(&'a self) -> &'a Model9 {
        &self.model9
    }
}

impl<'a> Extract<'a, Model10> for BaselineModels {
    fn extract(&'a self) -> &'a Model10 {
        &self.model10
    }
}

impl<'a> Extract<'a, Model11> for BaselineModels {
    fn extract(&'a self) -> &'a Model11 {
        &self.model11
    }
}

impl<'a> Extract<'a, Model12> for BaselineModels {
    fn extract(&'a self) -> &'a Model12 {
        &self.model12
    }
}

impl BaselineModels {
    fn get_value_by_index(&self, index: usize) -> u64 {
        match index % 12 {
            0 => Extract::<Model1>::extract(self).value,
            1 => Extract::<Model2>::extract(self).value,
            2 => Extract::<Model3>::extract(self).value,
            3 => Extract::<Model4>::extract(self).value,
            4 => Extract::<Model5>::extract(self).value,
            5 => Extract::<Model6>::extract(self).value,
            6 => Extract::<Model7>::extract(self).value,
            7 => Extract::<Model8>::extract(self).value,
            8 => Extract::<Model9>::extract(self).value,
            9 => Extract::<Model10>::extract(self).value,
            10 => Extract::<Model11>::extract(self).value,
            11 => Extract::<Model12>::extract(self).value,
            _ => unreachable!(),
        }
    }
}

impl BaselineModels32 {
    fn new() -> Self {
        Self {
            model1: Model1 { value: 1, active: true },
            model2: Model2 { value: 2, name: "test".to_string() },
            model3: Model3 { value: 3, count: 100 },
            model4: Model4 { value: 4, temperature: 20.5 },
            model5: Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] },
            model6: Model6 { value: 6, timestamp: 1234567890 },
            model7: Model7 { value: 7, enabled: false, priority: 3 },
            model8: Model8 { value: 8, coordinates: (10.0, 20.0) },
            model9: Model9 { value: 9, status: "running".to_string(), retries: 0 },
            model10: Model10 { value: 10, data: vec![1, 2, 3, 4] },
            model11: Model11 { value: 11, config: Some("config".to_string()) },
            model12: Model12 { 
                value: 12, 
                metadata: {
                    let mut map = HashMap::new();
                    map.insert("key".to_string(), "value".to_string());
                    map
                }
            },
            model13: Model13 { value: 13, id: 1001 },
            model14: Model14 { value: 14, score: 95.5 },
            model15: Model15 { value: 15, label: "test".to_string() },
            model16: Model16 { value: 16, flags: vec![true, false, true] },
            model17: Model17 { value: 17, weight: 2.5 },
            model18: Model18 { value: 18, sequence: 12345 },
            model19: Model19 { value: 19, category: "A".to_string() },
            model20: Model20 { value: 20, position: (100, 200) },
            model21: Model21 { value: 21, state: true },
            model22: Model22 { value: 22, duration: 987654321 },
            model23: Model23 { value: 23, bytes: vec![0xFF, 0x00, 0xAA] },
            model24: Model24 { value: 24, ratio: 0.75 },
            model25: Model25 { value: 25, tags: vec!["tag1".to_string(), "tag2".to_string()] },
            model26: Model26 { value: 26, index: 42 },
            model27: Model27 { value: 27, offset: -100 },
            model28: Model28 { value: 28, size: 1024 },
            model29: Model29 { value: 29, version: "1.0.0".to_string() },
            model30: Model30 { value: 30, level: 5 },
            model31: Model31 { value: 31, status_code: 200 },
            model32: Model32 { value: 32, checksum: 0xDEADBEEF },
        }
    }

    fn get_value_by_index(&self, index: usize) -> u64 {
        match index % 32 {
            0 => self.model1.value, 1 => self.model2.value, 2 => self.model3.value, 3 => self.model4.value,
            4 => self.model5.value, 5 => self.model6.value, 6 => self.model7.value, 7 => self.model8.value,
            8 => self.model9.value, 9 => self.model10.value, 10 => self.model11.value, 11 => self.model12.value,
            12 => self.model13.value, 13 => self.model14.value, 14 => self.model15.value, 15 => self.model16.value,
            16 => self.model17.value, 17 => self.model18.value, 18 => self.model19.value, 19 => self.model20.value,
            20 => self.model21.value, 21 => self.model22.value, 22 => self.model23.value, 23 => self.model24.value,
            24 => self.model25.value, 25 => self.model26.value, 26 => self.model27.value, 27 => self.model28.value,
            28 => self.model29.value, 29 => self.model30.value, 30 => self.model31.value, 31 => self.model32.value,
            _ => unreachable!(),
        }
    }
}


// ============================================================================
// FxHashMap Approach - Faster hash map using rustc-hash
// ============================================================================

struct FxHashMapStorage {
    models: FxHashMap<TypeId, Box<dyn Any>>,
    type_ids: Vec<TypeId>,
}

impl FxHashMapStorage {
    fn new() -> Self {
        let mut models = FxHashMap::default();
        
        models.insert(TypeId::of::<Model1>(), Box::new(Model1 { value: 1, active: true }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model2>(), Box::new(Model2 { value: 2, name: "test".to_string() }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model3>(), Box::new(Model3 { value: 3, count: 100 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model4>(), Box::new(Model4 { value: 4, temperature: 20.5 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model5>(), Box::new(Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model6>(), Box::new(Model6 { value: 6, timestamp: 1234567890 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model7>(), Box::new(Model7 { value: 7, enabled: false, priority: 3 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model8>(), Box::new(Model8 { value: 8, coordinates: (10.0, 20.0) }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model9>(), Box::new(Model9 { value: 9, status: "running".to_string(), retries: 0 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model10>(), Box::new(Model10 { value: 10, data: vec![1, 2, 3, 4] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model11>(), Box::new(Model11 { value: 11, config: Some("config".to_string()) }) as Box<dyn Any>);
        
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());
        models.insert(TypeId::of::<Model12>(), Box::new(Model12 { value: 12, metadata }) as Box<dyn Any>);
        
        let type_ids = vec![
            TypeId::of::<Model1>(),
            TypeId::of::<Model2>(),
            TypeId::of::<Model3>(),
            TypeId::of::<Model4>(),
            TypeId::of::<Model5>(),
            TypeId::of::<Model6>(),
            TypeId::of::<Model7>(),
            TypeId::of::<Model8>(),
            TypeId::of::<Model9>(),
            TypeId::of::<Model10>(),
            TypeId::of::<Model11>(),
            TypeId::of::<Model12>(),
        ];
        
        Self { models, type_ids }
    }
    
    fn get_value_by_index(&self, index: usize) -> u64 {
        let type_id = self.type_ids[index % 12];
        if let Some(model) = self.models.get(&type_id) {
            match index % 12 {
                0 => unsafe { model.downcast_ref_unchecked::<Model1>() }.value,
                1 => unsafe { model.downcast_ref_unchecked::<Model2>() }.value,
                2 => unsafe { model.downcast_ref_unchecked::<Model3>() }.value,
                3 => unsafe { model.downcast_ref_unchecked::<Model4>() }.value,
                4 => unsafe { model.downcast_ref_unchecked::<Model5>() }.value,
                5 => unsafe { model.downcast_ref_unchecked::<Model6>() }.value,
                6 => unsafe { model.downcast_ref_unchecked::<Model7>() }.value,
                7 => unsafe { model.downcast_ref_unchecked::<Model8>() }.value,
                8 => unsafe { model.downcast_ref_unchecked::<Model9>() }.value,
                9 => unsafe { model.downcast_ref_unchecked::<Model10>() }.value,
                10 => unsafe { model.downcast_ref_unchecked::<Model11>() }.value,
                11 => unsafe { model.downcast_ref_unchecked::<Model12>() }.value,
                _ => unreachable!(),
            }
        } else {
            0
        }
    }
}

// ============================================================================
// MicroMap Approach - Small optimized map
// ============================================================================

use micromap::Map as MicroMap;

struct MicroMapStorage {
    models: MicroMap<TypeId, Box<dyn Any>, 16>, // Up to 16 entries
    type_ids: Vec<TypeId>,
}

impl MicroMapStorage {
    fn new() -> Self {
        let mut models = MicroMap::new();
        
        models.insert(TypeId::of::<Model1>(), Box::new(Model1 { value: 1, active: true }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model2>(), Box::new(Model2 { value: 2, name: "test".to_string() }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model3>(), Box::new(Model3 { value: 3, count: 100 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model4>(), Box::new(Model4 { value: 4, temperature: 20.5 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model5>(), Box::new(Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model6>(), Box::new(Model6 { value: 6, timestamp: 1234567890 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model7>(), Box::new(Model7 { value: 7, enabled: false, priority: 3 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model8>(), Box::new(Model8 { value: 8, coordinates: (10.0, 20.0) }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model9>(), Box::new(Model9 { value: 9, status: "running".to_string(), retries: 0 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model10>(), Box::new(Model10 { value: 10, data: vec![1, 2, 3, 4] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model11>(), Box::new(Model11 { value: 11, config: Some("config".to_string()) }) as Box<dyn Any>);
        
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());
        models.insert(TypeId::of::<Model12>(), Box::new(Model12 { value: 12, metadata }) as Box<dyn Any>);
        
        let type_ids = vec![
            TypeId::of::<Model1>(),
            TypeId::of::<Model2>(),
            TypeId::of::<Model3>(),
            TypeId::of::<Model4>(),
            TypeId::of::<Model5>(),
            TypeId::of::<Model6>(),
            TypeId::of::<Model7>(),
            TypeId::of::<Model8>(),
            TypeId::of::<Model9>(),
            TypeId::of::<Model10>(),
            TypeId::of::<Model11>(),
            TypeId::of::<Model12>(),
        ];
        
        Self { models, type_ids }
    }
    
    fn get_value_by_index(&self, index: usize) -> u64 {
        let type_id = self.type_ids[index % 12];
        if let Some(model) = self.models.get(&type_id) {
            match index % 12 {
                0 => unsafe { model.downcast_ref_unchecked::<Model1>() }.value,
                1 => unsafe { model.downcast_ref_unchecked::<Model2>() }.value,
                2 => unsafe { model.downcast_ref_unchecked::<Model3>() }.value,
                3 => unsafe { model.downcast_ref_unchecked::<Model4>() }.value,
                4 => unsafe { model.downcast_ref_unchecked::<Model5>() }.value,
                5 => unsafe { model.downcast_ref_unchecked::<Model6>() }.value,
                6 => unsafe { model.downcast_ref_unchecked::<Model7>() }.value,
                7 => unsafe { model.downcast_ref_unchecked::<Model8>() }.value,
                8 => unsafe { model.downcast_ref_unchecked::<Model9>() }.value,
                9 => unsafe { model.downcast_ref_unchecked::<Model10>() }.value,
                10 => unsafe { model.downcast_ref_unchecked::<Model11>() }.value,
                11 => unsafe { model.downcast_ref_unchecked::<Model12>() }.value,
                _ => unreachable!(),
            }
        } else {
            0
        }
    }
}

// ============================================================================
// IndexMap Approach - Growable map optimized for small collections
// ============================================================================

use indexmap::IndexMap;

struct IndexMapStorage {
    models: IndexMap<TypeId, Box<dyn Any>>,
    type_ids: Vec<TypeId>,
}

impl IndexMapStorage {
    fn new() -> Self {
        let mut models = IndexMap::new();
        
        models.insert(TypeId::of::<Model1>(), Box::new(Model1 { value: 1, active: true }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model2>(), Box::new(Model2 { value: 2, name: "test".to_string() }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model3>(), Box::new(Model3 { value: 3, count: 100 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model4>(), Box::new(Model4 { value: 4, temperature: 20.5 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model5>(), Box::new(Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model6>(), Box::new(Model6 { value: 6, timestamp: 1234567890 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model7>(), Box::new(Model7 { value: 7, enabled: false, priority: 3 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model8>(), Box::new(Model8 { value: 8, coordinates: (10.0, 20.0) }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model9>(), Box::new(Model9 { value: 9, status: "running".to_string(), retries: 0 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model10>(), Box::new(Model10 { value: 10, data: vec![1, 2, 3, 4] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model11>(), Box::new(Model11 { value: 11, config: Some("config".to_string()) }) as Box<dyn Any>);
        
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());
        models.insert(TypeId::of::<Model12>(), Box::new(Model12 { value: 12, metadata }) as Box<dyn Any>);
        
        let type_ids = vec![
            TypeId::of::<Model1>(),
            TypeId::of::<Model2>(),
            TypeId::of::<Model3>(),
            TypeId::of::<Model4>(),
            TypeId::of::<Model5>(),
            TypeId::of::<Model6>(),
            TypeId::of::<Model7>(),
            TypeId::of::<Model8>(),
            TypeId::of::<Model9>(),
            TypeId::of::<Model10>(),
            TypeId::of::<Model11>(),
            TypeId::of::<Model12>(),
        ];
        
        Self { models, type_ids }
    }
    
    fn get_value_by_index(&self, index: usize) -> u64 {
        let type_id = self.type_ids[index % 12];
        if let Some(model) = self.models.get(&type_id) {
            match index % 12 {
                0 => unsafe { model.downcast_ref_unchecked::<Model1>() }.value,
                1 => unsafe { model.downcast_ref_unchecked::<Model2>() }.value,
                2 => unsafe { model.downcast_ref_unchecked::<Model3>() }.value,
                3 => unsafe { model.downcast_ref_unchecked::<Model4>() }.value,
                4 => unsafe { model.downcast_ref_unchecked::<Model5>() }.value,
                5 => unsafe { model.downcast_ref_unchecked::<Model6>() }.value,
                6 => unsafe { model.downcast_ref_unchecked::<Model7>() }.value,
                7 => unsafe { model.downcast_ref_unchecked::<Model8>() }.value,
                8 => unsafe { model.downcast_ref_unchecked::<Model9>() }.value,
                9 => unsafe { model.downcast_ref_unchecked::<Model10>() }.value,
                10 => unsafe { model.downcast_ref_unchecked::<Model11>() }.value,
                11 => unsafe { model.downcast_ref_unchecked::<Model12>() }.value,
                _ => unreachable!(),
            }
        } else {
            0
        }
    }
}

// ============================================================================
// AHashMap Approach - High-performance growable HashMap
// ============================================================================

use ahash::AHashMap;

struct AHashMapStorage {
    models: AHashMap<TypeId, Box<dyn Any>>,
    type_ids: Vec<TypeId>,
}

impl AHashMapStorage {
    fn new() -> Self {
        let mut models = AHashMap::new();
        
        models.insert(TypeId::of::<Model1>(), Box::new(Model1 { value: 1, active: true }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model2>(), Box::new(Model2 { value: 2, name: "test".to_string() }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model3>(), Box::new(Model3 { value: 3, count: 100 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model4>(), Box::new(Model4 { value: 4, temperature: 20.5 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model5>(), Box::new(Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model6>(), Box::new(Model6 { value: 6, timestamp: 1234567890 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model7>(), Box::new(Model7 { value: 7, enabled: false, priority: 3 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model8>(), Box::new(Model8 { value: 8, coordinates: (10.0, 20.0) }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model9>(), Box::new(Model9 { value: 9, status: "running".to_string(), retries: 0 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model10>(), Box::new(Model10 { value: 10, data: vec![1, 2, 3, 4] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model11>(), Box::new(Model11 { value: 11, config: Some("config".to_string()) }) as Box<dyn Any>);
        
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());
        models.insert(TypeId::of::<Model12>(), Box::new(Model12 { value: 12, metadata }) as Box<dyn Any>);
        
        let type_ids = vec![
            TypeId::of::<Model1>(),
            TypeId::of::<Model2>(),
            TypeId::of::<Model3>(),
            TypeId::of::<Model4>(),
            TypeId::of::<Model5>(),
            TypeId::of::<Model6>(),
            TypeId::of::<Model7>(),
            TypeId::of::<Model8>(),
            TypeId::of::<Model9>(),
            TypeId::of::<Model10>(),
            TypeId::of::<Model11>(),
            TypeId::of::<Model12>(),
        ];
        
        Self { models, type_ids }
    }
    
    fn get_value_by_index(&self, index: usize) -> u64 {
        let type_id = self.type_ids[index % 12];
        if let Some(model) = self.models.get(&type_id) {
            match index % 12 {
                0 => unsafe { model.downcast_ref_unchecked::<Model1>() }.value,
                1 => unsafe { model.downcast_ref_unchecked::<Model2>() }.value,
                2 => unsafe { model.downcast_ref_unchecked::<Model3>() }.value,
                3 => unsafe { model.downcast_ref_unchecked::<Model4>() }.value,
                4 => unsafe { model.downcast_ref_unchecked::<Model5>() }.value,
                5 => unsafe { model.downcast_ref_unchecked::<Model6>() }.value,
                6 => unsafe { model.downcast_ref_unchecked::<Model7>() }.value,
                7 => unsafe { model.downcast_ref_unchecked::<Model8>() }.value,
                8 => unsafe { model.downcast_ref_unchecked::<Model9>() }.value,
                9 => unsafe { model.downcast_ref_unchecked::<Model10>() }.value,
                10 => unsafe { model.downcast_ref_unchecked::<Model11>() }.value,
                11 => unsafe { model.downcast_ref_unchecked::<Model12>() }.value,
                _ => unreachable!(),
            }
        } else {
            0
        }
    }
}

// ============================================================================
// Hybrid Approach - Linear search -> HashMap transition
// ============================================================================

enum HybridStorage {
    Small(MicroMap<TypeId, Box<dyn Any>, 8>), // MicroMap for ≤ 8 items
    Large(FxHashMap<TypeId, Box<dyn Any>>), // Hash map for > 8 items
}

struct HybridMapStorage {
    models: HybridStorage,
    type_ids: Vec<TypeId>,
}

impl HybridMapStorage {
    fn new() -> Self {
        // Since we have 12 items total, we exceed the 8-item threshold for MicroMap
        // So we'll use Large mode directly for this benchmark
        let mut large_map = FxHashMap::default();
        
        // Insert first 8 models
        large_map.insert(TypeId::of::<Model1>(), Box::new(Model1 { value: 1, active: true }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model2>(), Box::new(Model2 { value: 2, name: "test".to_string() }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model3>(), Box::new(Model3 { value: 3, count: 100 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model4>(), Box::new(Model4 { value: 4, temperature: 20.5 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model5>(), Box::new(Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model6>(), Box::new(Model6 { value: 6, timestamp: 1234567890 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model7>(), Box::new(Model7 { value: 7, enabled: false, priority: 3 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model8>(), Box::new(Model8 { value: 8, coordinates: (10.0, 20.0) }) as Box<dyn Any>);
        
        // Add remaining models
        large_map.insert(TypeId::of::<Model9>(), Box::new(Model9 { value: 9, status: "running".to_string(), retries: 0 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model10>(), Box::new(Model10 { value: 10, data: vec![1, 2, 3, 4] }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model11>(), Box::new(Model11 { value: 11, config: Some("config".to_string()) }) as Box<dyn Any>);
        
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());
        large_map.insert(TypeId::of::<Model12>(), Box::new(Model12 { value: 12, metadata }) as Box<dyn Any>);
        
        let type_ids = vec![
            TypeId::of::<Model1>(),
            TypeId::of::<Model2>(),
            TypeId::of::<Model3>(),
            TypeId::of::<Model4>(),
            TypeId::of::<Model5>(),
            TypeId::of::<Model6>(),
            TypeId::of::<Model7>(),
            TypeId::of::<Model8>(),
            TypeId::of::<Model9>(),
            TypeId::of::<Model10>(),
            TypeId::of::<Model11>(),
            TypeId::of::<Model12>(),
        ];
        
        Self { 
            models: HybridStorage::Large(large_map),
            type_ids 
        }
    }
    
    // Alternative constructor that uses Small storage for benchmarking
    fn new_small() -> Self {
        let mut micromap = MicroMap::new();
        
        // Insert only first 8 models to test the Small variant
        micromap.insert(TypeId::of::<Model1>(), Box::new(Model1 { value: 1, active: true }) as Box<dyn Any>);
        micromap.insert(TypeId::of::<Model2>(), Box::new(Model2 { value: 2, name: "test".to_string() }) as Box<dyn Any>);
        micromap.insert(TypeId::of::<Model3>(), Box::new(Model3 { value: 3, count: 100 }) as Box<dyn Any>);
        micromap.insert(TypeId::of::<Model4>(), Box::new(Model4 { value: 4, temperature: 20.5 }) as Box<dyn Any>);
        micromap.insert(TypeId::of::<Model5>(), Box::new(Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] }) as Box<dyn Any>);
        micromap.insert(TypeId::of::<Model6>(), Box::new(Model6 { value: 6, timestamp: 1234567890 }) as Box<dyn Any>);
        micromap.insert(TypeId::of::<Model7>(), Box::new(Model7 { value: 7, enabled: false, priority: 3 }) as Box<dyn Any>);
        micromap.insert(TypeId::of::<Model8>(), Box::new(Model8 { value: 8, coordinates: (10.0, 20.0) }) as Box<dyn Any>);
        
        let type_ids = vec![
            TypeId::of::<Model1>(),
            TypeId::of::<Model2>(),
            TypeId::of::<Model3>(),
            TypeId::of::<Model4>(),
            TypeId::of::<Model5>(),
            TypeId::of::<Model6>(),
            TypeId::of::<Model7>(),
            TypeId::of::<Model8>(),
        ];
        
        Self { models: HybridStorage::Small(micromap), type_ids }
    }
    
    // Constructor specifically for 4-model benchmark using Small variant
    fn new_4_models() -> Self {
        let mut micromap = MicroMap::new();
        
        // Insert only first 4 models to test the Small variant properly
        micromap.insert(TypeId::of::<Model1>(), Box::new(Model1 { value: 1, active: true }) as Box<dyn Any>);
        micromap.insert(TypeId::of::<Model2>(), Box::new(Model2 { value: 2, name: "test".to_string() }) as Box<dyn Any>);
        micromap.insert(TypeId::of::<Model3>(), Box::new(Model3 { value: 3, count: 100 }) as Box<dyn Any>);
        micromap.insert(TypeId::of::<Model4>(), Box::new(Model4 { value: 4, temperature: 20.5 }) as Box<dyn Any>);
        
        let type_ids = vec![
            TypeId::of::<Model1>(),
            TypeId::of::<Model2>(),
            TypeId::of::<Model3>(),
            TypeId::of::<Model4>(),
        ];
        
        Self { models: HybridStorage::Small(micromap), type_ids }
    }
    
    // Constructor for 12-model benchmark using Large variant
    fn new_12_models() -> Self {
        let mut large_map = FxHashMap::default();
        
        // Insert all 12 models
        large_map.insert(TypeId::of::<Model1>(), Box::new(Model1 { value: 1, active: true }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model2>(), Box::new(Model2 { value: 2, name: "test".to_string() }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model3>(), Box::new(Model3 { value: 3, count: 100 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model4>(), Box::new(Model4 { value: 4, temperature: 20.5 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model5>(), Box::new(Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model6>(), Box::new(Model6 { value: 6, timestamp: 1234567890 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model7>(), Box::new(Model7 { value: 7, enabled: false, priority: 3 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model8>(), Box::new(Model8 { value: 8, coordinates: (10.0, 20.0) }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model9>(), Box::new(Model9 { value: 9, status: "running".to_string(), retries: 0 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model10>(), Box::new(Model10 { value: 10, data: vec![1, 2, 3, 4] }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model11>(), Box::new(Model11 { value: 11, config: Some("config".to_string()) }) as Box<dyn Any>);
        
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());
        large_map.insert(TypeId::of::<Model12>(), Box::new(Model12 { value: 12, metadata }) as Box<dyn Any>);
        
        let type_ids = vec![
            TypeId::of::<Model1>(),
            TypeId::of::<Model2>(),
            TypeId::of::<Model3>(),
            TypeId::of::<Model4>(),
            TypeId::of::<Model5>(),
            TypeId::of::<Model6>(),
            TypeId::of::<Model7>(),
            TypeId::of::<Model8>(),
            TypeId::of::<Model9>(),
            TypeId::of::<Model10>(),
            TypeId::of::<Model11>(),
            TypeId::of::<Model12>(),
        ];
        
        Self { 
            models: HybridStorage::Large(large_map),
            type_ids 
        }
    }
    
    // Constructor for 32-model benchmark using Large variant
    fn new_32_models() -> Self {
        let mut large_map = FxHashMap::default();
        
        // Insert all 32 models
        large_map.insert(TypeId::of::<Model1>(), Box::new(Model1 { value: 1, active: true }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model2>(), Box::new(Model2 { value: 2, name: "test".to_string() }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model3>(), Box::new(Model3 { value: 3, count: 100 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model4>(), Box::new(Model4 { value: 4, temperature: 20.5 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model5>(), Box::new(Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model6>(), Box::new(Model6 { value: 6, timestamp: 1234567890 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model7>(), Box::new(Model7 { value: 7, enabled: false, priority: 3 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model8>(), Box::new(Model8 { value: 8, coordinates: (10.0, 20.0) }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model9>(), Box::new(Model9 { value: 9, status: "running".to_string(), retries: 0 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model10>(), Box::new(Model10 { value: 10, data: vec![1, 2, 3, 4] }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model11>(), Box::new(Model11 { value: 11, config: Some("config".to_string()) }) as Box<dyn Any>);
        
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());
        large_map.insert(TypeId::of::<Model12>(), Box::new(Model12 { value: 12, metadata }) as Box<dyn Any>);
        
        // Add models 13-32
        large_map.insert(TypeId::of::<Model13>(), Box::new(Model13 { value: 13, id: 13 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model14>(), Box::new(Model14 { value: 14, score: 14.0 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model15>(), Box::new(Model15 { value: 15, label: "fifteen".to_string() }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model16>(), Box::new(Model16 { value: 16, flags: vec![true, false] }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model17>(), Box::new(Model17 { value: 17, weight: 17.5 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model18>(), Box::new(Model18 { value: 18, sequence: 18 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model19>(), Box::new(Model19 { value: 19, category: "nineteen".to_string() }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model20>(), Box::new(Model20 { value: 20, position: (20, 20) }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model21>(), Box::new(Model21 { value: 21, state: true }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model22>(), Box::new(Model22 { value: 22, duration: 22 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model23>(), Box::new(Model23 { value: 23, bytes: vec![2, 3] }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model24>(), Box::new(Model24 { value: 24, ratio: 24.0 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model25>(), Box::new(Model25 { value: 25, tags: vec!["tag25".to_string()] }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model26>(), Box::new(Model26 { value: 26, index: 26 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model27>(), Box::new(Model27 { value: 27, offset: 27 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model28>(), Box::new(Model28 { value: 28, size: 28 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model29>(), Box::new(Model29 { value: 29, version: "v29".to_string() }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model30>(), Box::new(Model30 { value: 30, level: 30 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model31>(), Box::new(Model31 { value: 31, status_code: 31 }) as Box<dyn Any>);
        large_map.insert(TypeId::of::<Model32>(), Box::new(Model32 { value: 32, checksum: 32 }) as Box<dyn Any>);
        
        let type_ids = vec![
            TypeId::of::<Model1>(), TypeId::of::<Model2>(), TypeId::of::<Model3>(), TypeId::of::<Model4>(),
            TypeId::of::<Model5>(), TypeId::of::<Model6>(), TypeId::of::<Model7>(), TypeId::of::<Model8>(),
            TypeId::of::<Model9>(), TypeId::of::<Model10>(), TypeId::of::<Model11>(), TypeId::of::<Model12>(),
            TypeId::of::<Model13>(), TypeId::of::<Model14>(), TypeId::of::<Model15>(), TypeId::of::<Model16>(),
            TypeId::of::<Model17>(), TypeId::of::<Model18>(), TypeId::of::<Model19>(), TypeId::of::<Model20>(),
            TypeId::of::<Model21>(), TypeId::of::<Model22>(), TypeId::of::<Model23>(), TypeId::of::<Model24>(),
            TypeId::of::<Model25>(), TypeId::of::<Model26>(), TypeId::of::<Model27>(), TypeId::of::<Model28>(),
            TypeId::of::<Model29>(), TypeId::of::<Model30>(), TypeId::of::<Model31>(), TypeId::of::<Model32>(),
        ];
        
        Self { 
            models: HybridStorage::Large(large_map),
            type_ids 
        }
    }
    
    fn get_value_by_index(&self, index: usize) -> u64 {
        let type_id = self.type_ids[index % self.type_ids.len()];
        
        match &self.models {
            HybridStorage::Small(micromap) => {
                // MicroMap lookup
                if let Some(model) = micromap.get(&type_id) {
                    match index % self.type_ids.len() {
                        0 => unsafe { model.downcast_ref_unchecked::<Model1>() }.value,
                        1 => unsafe { model.downcast_ref_unchecked::<Model2>() }.value,
                        2 => unsafe { model.downcast_ref_unchecked::<Model3>() }.value,
                        3 => unsafe { model.downcast_ref_unchecked::<Model4>() }.value,
                        4 => unsafe { model.downcast_ref_unchecked::<Model5>() }.value,
                        5 => unsafe { model.downcast_ref_unchecked::<Model6>() }.value,
                        6 => unsafe { model.downcast_ref_unchecked::<Model7>() }.value,
                        7 => unsafe { model.downcast_ref_unchecked::<Model8>() }.value,
                        8 => unsafe { model.downcast_ref_unchecked::<Model9>() }.value,
                        9 => unsafe { model.downcast_ref_unchecked::<Model10>() }.value,
                        10 => unsafe { model.downcast_ref_unchecked::<Model11>() }.value,
                        11 => unsafe { model.downcast_ref_unchecked::<Model12>() }.value,
                        _ => unreachable!(),
                    }
                } else {
                    0
                }
            }
            HybridStorage::Large(hashmap) => {
                // Hash map lookup
                if let Some(model) = hashmap.get(&type_id) {
                    match index % self.type_ids.len() {
                        0 => unsafe { model.downcast_ref_unchecked::<Model1>() }.value,
                        1 => unsafe { model.downcast_ref_unchecked::<Model2>() }.value,
                        2 => unsafe { model.downcast_ref_unchecked::<Model3>() }.value,
                        3 => unsafe { model.downcast_ref_unchecked::<Model4>() }.value,
                        4 => unsafe { model.downcast_ref_unchecked::<Model5>() }.value,
                        5 => unsafe { model.downcast_ref_unchecked::<Model6>() }.value,
                        6 => unsafe { model.downcast_ref_unchecked::<Model7>() }.value,
                        7 => unsafe { model.downcast_ref_unchecked::<Model8>() }.value,
                        8 => unsafe { model.downcast_ref_unchecked::<Model9>() }.value,
                        9 => unsafe { model.downcast_ref_unchecked::<Model10>() }.value,
                        10 => unsafe { model.downcast_ref_unchecked::<Model11>() }.value,
                        11 => unsafe { model.downcast_ref_unchecked::<Model12>() }.value,
                        12 => unsafe { model.downcast_ref_unchecked::<Model13>() }.value,
                        13 => unsafe { model.downcast_ref_unchecked::<Model14>() }.value,
                        14 => unsafe { model.downcast_ref_unchecked::<Model15>() }.value,
                        15 => unsafe { model.downcast_ref_unchecked::<Model16>() }.value,
                        16 => unsafe { model.downcast_ref_unchecked::<Model17>() }.value,
                        17 => unsafe { model.downcast_ref_unchecked::<Model18>() }.value,
                        18 => unsafe { model.downcast_ref_unchecked::<Model19>() }.value,
                        19 => unsafe { model.downcast_ref_unchecked::<Model20>() }.value,
                        20 => unsafe { model.downcast_ref_unchecked::<Model21>() }.value,
                        21 => unsafe { model.downcast_ref_unchecked::<Model22>() }.value,
                        22 => unsafe { model.downcast_ref_unchecked::<Model23>() }.value,
                        23 => unsafe { model.downcast_ref_unchecked::<Model24>() }.value,
                        24 => unsafe { model.downcast_ref_unchecked::<Model25>() }.value,
                        25 => unsafe { model.downcast_ref_unchecked::<Model26>() }.value,
                        26 => unsafe { model.downcast_ref_unchecked::<Model27>() }.value,
                        27 => unsafe { model.downcast_ref_unchecked::<Model28>() }.value,
                        28 => unsafe { model.downcast_ref_unchecked::<Model29>() }.value,
                        29 => unsafe { model.downcast_ref_unchecked::<Model30>() }.value,
                        30 => unsafe { model.downcast_ref_unchecked::<Model31>() }.value,
                        31 => unsafe { model.downcast_ref_unchecked::<Model32>() }.value,
                        _ => unreachable!(),
                    }
                } else {
                    0
                }
            }
        }
    }
}

// ============================================================================
// HeapLess LinearMap Approach - Stack-allocated map for embedded applications
// ============================================================================

struct HeapLessStorage {
    models: LinearMap<TypeId, Box<dyn Any>, 16>, // Up to 16 entries on stack
    type_ids: Vec<TypeId>,
}

impl HeapLessStorage {
    fn new() -> Self {
        let mut models = LinearMap::new();
        
        let _ = models.insert(TypeId::of::<Model1>(), Box::new(Model1 { value: 1, active: true }) as Box<dyn Any>);
        let _ = models.insert(TypeId::of::<Model2>(), Box::new(Model2 { value: 2, name: "test".to_string() }) as Box<dyn Any>);
        let _ = models.insert(TypeId::of::<Model3>(), Box::new(Model3 { value: 3, count: 100 }) as Box<dyn Any>);
        let _ = models.insert(TypeId::of::<Model4>(), Box::new(Model4 { value: 4, temperature: 20.5 }) as Box<dyn Any>);
        let _ = models.insert(TypeId::of::<Model5>(), Box::new(Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] }) as Box<dyn Any>);
        let _ = models.insert(TypeId::of::<Model6>(), Box::new(Model6 { value: 6, timestamp: 1234567890 }) as Box<dyn Any>);
        let _ = models.insert(TypeId::of::<Model7>(), Box::new(Model7 { value: 7, enabled: false, priority: 3 }) as Box<dyn Any>);
        let _ = models.insert(TypeId::of::<Model8>(), Box::new(Model8 { value: 8, coordinates: (10.0, 20.0) }) as Box<dyn Any>);
        let _ = models.insert(TypeId::of::<Model9>(), Box::new(Model9 { value: 9, status: "running".to_string(), retries: 0 }) as Box<dyn Any>);
        let _ = models.insert(TypeId::of::<Model10>(), Box::new(Model10 { value: 10, data: vec![1, 2, 3, 4] }) as Box<dyn Any>);
        let _ = models.insert(TypeId::of::<Model11>(), Box::new(Model11 { value: 11, config: Some("config".to_string()) }) as Box<dyn Any>);
        
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());
        let _ = models.insert(TypeId::of::<Model12>(), Box::new(Model12 { value: 12, metadata }) as Box<dyn Any>);
        
        let type_ids = vec![
            TypeId::of::<Model1>(),
            TypeId::of::<Model2>(),
            TypeId::of::<Model3>(),
            TypeId::of::<Model4>(),
            TypeId::of::<Model5>(),
            TypeId::of::<Model6>(),
            TypeId::of::<Model7>(),
            TypeId::of::<Model8>(),
            TypeId::of::<Model9>(),
            TypeId::of::<Model10>(),
            TypeId::of::<Model11>(),
            TypeId::of::<Model12>(),
        ];
        
        Self { models, type_ids }
    }
    
    fn get_value_by_index(&self, index: usize) -> u64 {
        let type_id = self.type_ids[index % 12];
        if let Some(model) = self.models.get(&type_id) {
            match index % 12 {
                0 => unsafe { model.downcast_ref_unchecked::<Model1>() }.value,
                1 => unsafe { model.downcast_ref_unchecked::<Model2>() }.value,
                2 => unsafe { model.downcast_ref_unchecked::<Model3>() }.value,
                3 => unsafe { model.downcast_ref_unchecked::<Model4>() }.value,
                4 => unsafe { model.downcast_ref_unchecked::<Model5>() }.value,
                5 => unsafe { model.downcast_ref_unchecked::<Model6>() }.value,
                6 => unsafe { model.downcast_ref_unchecked::<Model7>() }.value,
                7 => unsafe { model.downcast_ref_unchecked::<Model8>() }.value,
                8 => unsafe { model.downcast_ref_unchecked::<Model9>() }.value,
                9 => unsafe { model.downcast_ref_unchecked::<Model10>() }.value,
                10 => unsafe { model.downcast_ref_unchecked::<Model11>() }.value,
                11 => unsafe { model.downcast_ref_unchecked::<Model12>() }.value,
                _ => unreachable!(),
            }
        } else {
            0
        }
    }
}

// ============================================================================
// 32-Model Storage Implementations for Testing Scale
// ============================================================================

struct FxHashMapStorage32 {
    models: FxHashMap<TypeId, Box<dyn Any>>,
    type_ids: Vec<TypeId>,
}

impl FxHashMapStorage32 {
    fn new() -> Self {
        let mut models = FxHashMap::default();
        
        // Insert all 32 models
        models.insert(TypeId::of::<Model1>(), Box::new(Model1 { value: 1, active: true }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model2>(), Box::new(Model2 { value: 2, name: "test".to_string() }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model3>(), Box::new(Model3 { value: 3, count: 100 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model4>(), Box::new(Model4 { value: 4, temperature: 20.5 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model5>(), Box::new(Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model6>(), Box::new(Model6 { value: 6, timestamp: 1234567890 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model7>(), Box::new(Model7 { value: 7, enabled: false, priority: 3 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model8>(), Box::new(Model8 { value: 8, coordinates: (10.0, 20.0) }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model9>(), Box::new(Model9 { value: 9, status: "running".to_string(), retries: 0 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model10>(), Box::new(Model10 { value: 10, data: vec![1, 2, 3, 4] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model11>(), Box::new(Model11 { value: 11, config: Some("config".to_string()) }) as Box<dyn Any>);
        
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());
        models.insert(TypeId::of::<Model12>(), Box::new(Model12 { value: 12, metadata }) as Box<dyn Any>);
        
        // Add models 13-32
        models.insert(TypeId::of::<Model13>(), Box::new(Model13 { value: 13, id: 1001 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model14>(), Box::new(Model14 { value: 14, score: 95.5 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model15>(), Box::new(Model15 { value: 15, label: "test".to_string() }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model16>(), Box::new(Model16 { value: 16, flags: vec![true, false, true] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model17>(), Box::new(Model17 { value: 17, weight: 2.5 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model18>(), Box::new(Model18 { value: 18, sequence: 12345 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model19>(), Box::new(Model19 { value: 19, category: "A".to_string() }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model20>(), Box::new(Model20 { value: 20, position: (100, 200) }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model21>(), Box::new(Model21 { value: 21, state: true }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model22>(), Box::new(Model22 { value: 22, duration: 987654321 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model23>(), Box::new(Model23 { value: 23, bytes: vec![0xFF, 0x00, 0xAA] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model24>(), Box::new(Model24 { value: 24, ratio: 0.75 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model25>(), Box::new(Model25 { value: 25, tags: vec!["tag1".to_string(), "tag2".to_string()] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model26>(), Box::new(Model26 { value: 26, index: 42 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model27>(), Box::new(Model27 { value: 27, offset: -100 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model28>(), Box::new(Model28 { value: 28, size: 1024 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model29>(), Box::new(Model29 { value: 29, version: "1.0.0".to_string() }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model30>(), Box::new(Model30 { value: 30, level: 5 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model31>(), Box::new(Model31 { value: 31, status_code: 200 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model32>(), Box::new(Model32 { value: 32, checksum: 0xDEADBEEF }) as Box<dyn Any>);
        
        let type_ids = vec![
            TypeId::of::<Model1>(), TypeId::of::<Model2>(), TypeId::of::<Model3>(), TypeId::of::<Model4>(),
            TypeId::of::<Model5>(), TypeId::of::<Model6>(), TypeId::of::<Model7>(), TypeId::of::<Model8>(),
            TypeId::of::<Model9>(), TypeId::of::<Model10>(), TypeId::of::<Model11>(), TypeId::of::<Model12>(),
            TypeId::of::<Model13>(), TypeId::of::<Model14>(), TypeId::of::<Model15>(), TypeId::of::<Model16>(),
            TypeId::of::<Model17>(), TypeId::of::<Model18>(), TypeId::of::<Model19>(), TypeId::of::<Model20>(),
            TypeId::of::<Model21>(), TypeId::of::<Model22>(), TypeId::of::<Model23>(), TypeId::of::<Model24>(),
            TypeId::of::<Model25>(), TypeId::of::<Model26>(), TypeId::of::<Model27>(), TypeId::of::<Model28>(),
            TypeId::of::<Model29>(), TypeId::of::<Model30>(), TypeId::of::<Model31>(), TypeId::of::<Model32>(),
        ];
        
        Self { models, type_ids }
    }
    
    fn get_value_by_index(&self, index: usize) -> u64 {
        let type_id = self.type_ids[index % 32];
        if let Some(model) = self.models.get(&type_id) {
            match index % 32 {
                0 => unsafe { model.downcast_ref_unchecked::<Model1>() }.value,
                1 => unsafe { model.downcast_ref_unchecked::<Model2>() }.value,
                2 => unsafe { model.downcast_ref_unchecked::<Model3>() }.value,
                3 => unsafe { model.downcast_ref_unchecked::<Model4>() }.value,
                4 => unsafe { model.downcast_ref_unchecked::<Model5>() }.value,
                5 => unsafe { model.downcast_ref_unchecked::<Model6>() }.value,
                6 => unsafe { model.downcast_ref_unchecked::<Model7>() }.value,
                7 => unsafe { model.downcast_ref_unchecked::<Model8>() }.value,
                8 => unsafe { model.downcast_ref_unchecked::<Model9>() }.value,
                9 => unsafe { model.downcast_ref_unchecked::<Model10>() }.value,
                10 => unsafe { model.downcast_ref_unchecked::<Model11>() }.value,
                11 => unsafe { model.downcast_ref_unchecked::<Model12>() }.value,
                12 => unsafe { model.downcast_ref_unchecked::<Model13>() }.value,
                13 => unsafe { model.downcast_ref_unchecked::<Model14>() }.value,
                14 => unsafe { model.downcast_ref_unchecked::<Model15>() }.value,
                15 => unsafe { model.downcast_ref_unchecked::<Model16>() }.value,
                16 => unsafe { model.downcast_ref_unchecked::<Model17>() }.value,
                17 => unsafe { model.downcast_ref_unchecked::<Model18>() }.value,
                18 => unsafe { model.downcast_ref_unchecked::<Model19>() }.value,
                19 => unsafe { model.downcast_ref_unchecked::<Model20>() }.value,
                20 => unsafe { model.downcast_ref_unchecked::<Model21>() }.value,
                21 => unsafe { model.downcast_ref_unchecked::<Model22>() }.value,
                22 => unsafe { model.downcast_ref_unchecked::<Model23>() }.value,
                23 => unsafe { model.downcast_ref_unchecked::<Model24>() }.value,
                24 => unsafe { model.downcast_ref_unchecked::<Model25>() }.value,
                25 => unsafe { model.downcast_ref_unchecked::<Model26>() }.value,
                26 => unsafe { model.downcast_ref_unchecked::<Model27>() }.value,
                27 => unsafe { model.downcast_ref_unchecked::<Model28>() }.value,
                28 => unsafe { model.downcast_ref_unchecked::<Model29>() }.value,
                29 => unsafe { model.downcast_ref_unchecked::<Model30>() }.value,
                30 => unsafe { model.downcast_ref_unchecked::<Model31>() }.value,
                31 => unsafe { model.downcast_ref_unchecked::<Model32>() }.value,
                _ => unreachable!(),
            }
        } else {
            0
        }
    }
}

struct MicroMapStorage32 {
    models: MicroMap<TypeId, Box<dyn Any>, 32>, // Fixed size for 32 models
    type_ids: Vec<TypeId>,
}

impl MicroMapStorage32 {
    fn new() -> Self {
        let mut models = MicroMap::new();
        
        // Insert all 32 models
        models.insert(TypeId::of::<Model1>(), Box::new(Model1 { value: 1, active: true }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model2>(), Box::new(Model2 { value: 2, name: "test".to_string() }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model3>(), Box::new(Model3 { value: 3, count: 100 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model4>(), Box::new(Model4 { value: 4, temperature: 20.5 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model5>(), Box::new(Model5 { value: 5, items: vec!["a".to_string(), "b".to_string()] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model6>(), Box::new(Model6 { value: 6, timestamp: 1234567890 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model7>(), Box::new(Model7 { value: 7, enabled: false, priority: 3 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model8>(), Box::new(Model8 { value: 8, coordinates: (10.0, 20.0) }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model9>(), Box::new(Model9 { value: 9, status: "running".to_string(), retries: 0 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model10>(), Box::new(Model10 { value: 10, data: vec![1, 2, 3, 4] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model11>(), Box::new(Model11 { value: 11, config: Some("config".to_string()) }) as Box<dyn Any>);
        
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());
        models.insert(TypeId::of::<Model12>(), Box::new(Model12 { value: 12, metadata }) as Box<dyn Any>);
        
        // Add models 13-32
        models.insert(TypeId::of::<Model13>(), Box::new(Model13 { value: 13, id: 1001 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model14>(), Box::new(Model14 { value: 14, score: 95.5 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model15>(), Box::new(Model15 { value: 15, label: "test".to_string() }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model16>(), Box::new(Model16 { value: 16, flags: vec![true, false, true] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model17>(), Box::new(Model17 { value: 17, weight: 2.5 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model18>(), Box::new(Model18 { value: 18, sequence: 12345 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model19>(), Box::new(Model19 { value: 19, category: "A".to_string() }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model20>(), Box::new(Model20 { value: 20, position: (100, 200) }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model21>(), Box::new(Model21 { value: 21, state: true }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model22>(), Box::new(Model22 { value: 22, duration: 987654321 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model23>(), Box::new(Model23 { value: 23, bytes: vec![0xFF, 0x00, 0xAA] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model24>(), Box::new(Model24 { value: 24, ratio: 0.75 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model25>(), Box::new(Model25 { value: 25, tags: vec!["tag1".to_string(), "tag2".to_string()] }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model26>(), Box::new(Model26 { value: 26, index: 42 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model27>(), Box::new(Model27 { value: 27, offset: -100 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model28>(), Box::new(Model28 { value: 28, size: 1024 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model29>(), Box::new(Model29 { value: 29, version: "1.0.0".to_string() }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model30>(), Box::new(Model30 { value: 30, level: 5 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model31>(), Box::new(Model31 { value: 31, status_code: 200 }) as Box<dyn Any>);
        models.insert(TypeId::of::<Model32>(), Box::new(Model32 { value: 32, checksum: 0xDEADBEEF }) as Box<dyn Any>);
        
        let type_ids = vec![
            TypeId::of::<Model1>(), TypeId::of::<Model2>(), TypeId::of::<Model3>(), TypeId::of::<Model4>(),
            TypeId::of::<Model5>(), TypeId::of::<Model6>(), TypeId::of::<Model7>(), TypeId::of::<Model8>(),
            TypeId::of::<Model9>(), TypeId::of::<Model10>(), TypeId::of::<Model11>(), TypeId::of::<Model12>(),
            TypeId::of::<Model13>(), TypeId::of::<Model14>(), TypeId::of::<Model15>(), TypeId::of::<Model16>(),
            TypeId::of::<Model17>(), TypeId::of::<Model18>(), TypeId::of::<Model19>(), TypeId::of::<Model20>(),
            TypeId::of::<Model21>(), TypeId::of::<Model22>(), TypeId::of::<Model23>(), TypeId::of::<Model24>(),
            TypeId::of::<Model25>(), TypeId::of::<Model26>(), TypeId::of::<Model27>(), TypeId::of::<Model28>(),
            TypeId::of::<Model29>(), TypeId::of::<Model30>(), TypeId::of::<Model31>(), TypeId::of::<Model32>(),
        ];
        
        Self { models, type_ids }
    }
    
    fn get_value_by_index(&self, index: usize) -> u64 {
        let type_id = self.type_ids[index % 32];
        if let Some(model) = self.models.get(&type_id) {
            match index % 32 {
                0 => unsafe { model.downcast_ref_unchecked::<Model1>() }.value,
                1 => unsafe { model.downcast_ref_unchecked::<Model2>() }.value,
                2 => unsafe { model.downcast_ref_unchecked::<Model3>() }.value,
                3 => unsafe { model.downcast_ref_unchecked::<Model4>() }.value,
                4 => unsafe { model.downcast_ref_unchecked::<Model5>() }.value,
                5 => unsafe { model.downcast_ref_unchecked::<Model6>() }.value,
                6 => unsafe { model.downcast_ref_unchecked::<Model7>() }.value,
                7 => unsafe { model.downcast_ref_unchecked::<Model8>() }.value,
                8 => unsafe { model.downcast_ref_unchecked::<Model9>() }.value,
                9 => unsafe { model.downcast_ref_unchecked::<Model10>() }.value,
                10 => unsafe { model.downcast_ref_unchecked::<Model11>() }.value,
                11 => unsafe { model.downcast_ref_unchecked::<Model12>() }.value,
                12 => unsafe { model.downcast_ref_unchecked::<Model13>() }.value,
                13 => unsafe { model.downcast_ref_unchecked::<Model14>() }.value,
                14 => unsafe { model.downcast_ref_unchecked::<Model15>() }.value,
                15 => unsafe { model.downcast_ref_unchecked::<Model16>() }.value,
                16 => unsafe { model.downcast_ref_unchecked::<Model17>() }.value,
                17 => unsafe { model.downcast_ref_unchecked::<Model18>() }.value,
                18 => unsafe { model.downcast_ref_unchecked::<Model19>() }.value,
                19 => unsafe { model.downcast_ref_unchecked::<Model20>() }.value,
                20 => unsafe { model.downcast_ref_unchecked::<Model21>() }.value,
                21 => unsafe { model.downcast_ref_unchecked::<Model22>() }.value,
                22 => unsafe { model.downcast_ref_unchecked::<Model23>() }.value,
                23 => unsafe { model.downcast_ref_unchecked::<Model24>() }.value,
                24 => unsafe { model.downcast_ref_unchecked::<Model25>() }.value,
                25 => unsafe { model.downcast_ref_unchecked::<Model26>() }.value,
                26 => unsafe { model.downcast_ref_unchecked::<Model27>() }.value,
                27 => unsafe { model.downcast_ref_unchecked::<Model28>() }.value,
                28 => unsafe { model.downcast_ref_unchecked::<Model29>() }.value,
                29 => unsafe { model.downcast_ref_unchecked::<Model30>() }.value,
                30 => unsafe { model.downcast_ref_unchecked::<Model31>() }.value,
                31 => unsafe { model.downcast_ref_unchecked::<Model32>() }.value,
                _ => unreachable!(),
            }
        } else {
            0
        }
    }
}

// ============================================================================
// Random Number Generator for Reproducible Tests
// ============================================================================

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
        self.state
    }
    
    fn gen_range(&mut self, max: usize) -> usize {
        (self.next() % max as u64) as usize
    }
}

// ============================================================================
// Benchmark Functions
// ============================================================================

fn benchmark_model_storage_12_models(c: &mut Criterion) {
    let mut group = c.benchmark_group("model_storage_12_models_10k_reads");
    
    const NUM_READS: usize = 10_000;
    
    // Generate random indices for consistent access pattern across all benchmarks
    let mut rng = SimpleRng::new(42); // Fixed seed for reproducibility
    let random_indices: Vec<usize> = (0..NUM_READS).map(|_| rng.gen_range(12)).collect();
    
    // Baseline: Single struct with extractor pattern
    let baseline = BaselineModels::new();
    group.bench_function("baseline_extractor", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(baseline.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // FxHashMap approach (winner from previous tests)
    let fx_hashmap_storage = FxHashMapStorage::new();
    group.bench_function("fx_hashmap", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(fx_hashmap_storage.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // MicroMap approach
    let micromap_storage = MicroMapStorage::new();
    group.bench_function("micromap", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(micromap_storage.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // IndexMap approach (growable, optimized for small collections)
    let indexmap_storage = IndexMapStorage::new();
    group.bench_function("indexmap", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(indexmap_storage.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // AHashMap approach (high-performance growable)
    let ahashmap_storage = AHashMapStorage::new();
    group.bench_function("ahashmap", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(ahashmap_storage.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // Hybrid approach (linear -> hash transition) - using Large variant for 12 models
    let hybrid_storage = HybridMapStorage::new_12_models();
    group.bench_function("hybrid", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(hybrid_storage.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // HeapLess LinearMap approach
    let heapless_storage = HeapLessStorage::new();
    group.bench_function("heapless_linearmap", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(heapless_storage.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    group.finish();
}

fn benchmark_model_storage_4_models(c: &mut Criterion) {
    let mut group = c.benchmark_group("model_storage_4_models_10k_reads");
    
    const NUM_READS: usize = 10_000;
    
    // Generate random indices for 4 models only
    let mut rng = SimpleRng::new(42); // Same seed for reproducibility
    let random_indices: Vec<usize> = (0..NUM_READS).map(|_| rng.gen_range(4)).collect();
    
    // Baseline: Single struct with extractor pattern (only first 4 models)
    let baseline = BaselineModels::new();
    group.bench_function("baseline_extractor", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(baseline.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // FxHashMap approach (only first 4 models)
    let fx_hashmap_storage = FxHashMapStorage::new();
    group.bench_function("fx_hashmap", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(fx_hashmap_storage.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // MicroMap approach (only first 4 models)
    let micromap_storage = MicroMapStorage::new();
    group.bench_function("micromap", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(micromap_storage.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // IndexMap approach (growable, optimized for small collections)
    let indexmap_storage = IndexMapStorage::new();
    group.bench_function("indexmap", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(indexmap_storage.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // AHashMap approach (high-performance growable)
    let ahashmap_storage = AHashMapStorage::new();
    group.bench_function("ahashmap", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(ahashmap_storage.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // Hybrid approach (linear -> hash transition) - using Small variant for 4 models
    let hybrid_storage = HybridMapStorage::new_4_models();
    group.bench_function("hybrid", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(hybrid_storage.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // HeapLess LinearMap approach (only first 4 models)
    let heapless_storage = HeapLessStorage::new();
    group.bench_function("heapless_linearmap", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(heapless_storage.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    group.finish();
}

fn benchmark_model_storage_32_models(c: &mut Criterion) {
    let mut group = c.benchmark_group("model_storage_32_models_10k_reads");
    
    const NUM_READS: usize = 10_000;
    
    // Generate random indices for 32 models
    let mut rng = SimpleRng::new(42); // Same seed for reproducibility
    let random_indices: Vec<usize> = (0..NUM_READS).map(|_| rng.gen_range(32)).collect();
    
    // Baseline: Single struct with all 32 models
    let baseline32 = BaselineModels32::new();
    group.bench_function("baseline_extractor", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(baseline32.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // FxHashMap approach with 32 models
    let fx_hashmap_storage32 = FxHashMapStorage32::new();
    group.bench_function("fx_hashmap", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(fx_hashmap_storage32.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // MicroMap approach with 32 models (testing your assertion that it will be slower)
    let micromap_storage32 = MicroMapStorage32::new();
    group.bench_function("micromap", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(micromap_storage32.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    // Hybrid approach with 32 models (using Large variant)
    let hybrid_storage32 = HybridMapStorage::new_32_models();
    group.bench_function("hybrid", |b| {
        b.iter(|| {
            let mut counter = 0u64;
            for &index in &random_indices {
                counter += black_box(hybrid_storage32.get_value_by_index(black_box(index)));
            }
            black_box(counter);
        });
    });
    
    group.finish();
}

criterion_group!(benches, benchmark_model_storage_12_models, benchmark_model_storage_4_models, benchmark_model_storage_32_models);
criterion_main!(benches);
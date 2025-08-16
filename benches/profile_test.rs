use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::any::TypeId;
use syzygy::model_chain::{ErasedModelChain, NoModels, ModelChainBuilder};

#[derive(Debug, Clone)]
struct Model1 { value: u64, active: bool }

#[derive(Debug, Clone)]
struct Model2 { value: u64, name: String }

#[derive(Debug, Clone)]
struct Model3 { value: u64, count: i32 }

#[derive(Debug, Clone)]
struct Model4 { value: u64, temperature: f32 }

#[derive(Debug, Clone)]
struct Model5 { value: u64, items: Vec<String> }

fn setup_erased_chain() -> ErasedModelChain {
    let chain = NoModels::default()
        .with_model(Model1 { value: 1, active: true })
        .with_model(Model2 { value: 2, name: "test".to_string() })
        .with_model(Model3 { value: 3, count: 100 })
        .with_model(Model4 { value: 4, temperature: 20.5 })
        .with_model(Model5 { value: 5, items: vec!["a".to_string()] });
    
    Box::new(chain)
}

fn benchmark_chain_access(c: &mut Criterion) {
    let chain = setup_erased_chain();
    
    c.bench_function("erased_chain_single_access", |b| {
        b.iter(|| {
            let model1 = black_box(chain.find_model::<Model1>().unwrap());
            let model2 = black_box(chain.find_model::<Model2>().unwrap());
            let model3 = black_box(chain.find_model::<Model3>().unwrap());
            let model4 = black_box(chain.find_model::<Model4>().unwrap());
            let model5 = black_box(chain.find_model::<Model5>().unwrap());
            
            black_box((model1.value, model2.value, model3.value, model4.value, model5.value));
        });
    });

    // Compare with direct access
    let models = (
        Model1 { value: 1, active: true },
        Model2 { value: 2, name: "test".to_string() },
        Model3 { value: 3, count: 100 },
        Model4 { value: 4, temperature: 20.5 },
        Model5 { value: 5, items: vec!["a".to_string()] },
    );
    
    c.bench_function("direct_access", |b| {
        b.iter(|| {
            black_box((models.0.value, models.1.value, models.2.value, models.3.value, models.4.value));
        });
    });
}

criterion_group!(benches, benchmark_chain_access);
criterion_main!(benches);
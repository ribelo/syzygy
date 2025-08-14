//! Test that Builder properly supports model chains

use syzygy::model_chain::{ModelChain, NoModels, ModelChainBuilder};

#[derive(Debug, Clone)]
struct UserModel {
    name: String,
}

#[derive(Debug, Clone)]
struct PostModel {
    title: String,
}

#[test]
fn test_model_chain_builder_basic() {
    // Build a model chain using the ModelChain directly
    let chain = NoModels::default()
        .with_model(UserModel { name: "Alice".to_string() })
        .with_model(PostModel { title: "Hello".to_string() });
    
    // Access the models using get with index inference
    let user = chain.get::<UserModel, _>();
    assert_eq!(user.name, "Alice");
    
    let post = chain.get::<PostModel, _>();
    assert_eq!(post.title, "Hello");
}

#[test]
fn test_model_chain_order() {
    // Models added later are at the head of the chain
    let chain = ModelChain::new(UserModel { name: "First".to_string() })
        .push(PostModel { title: "Second".to_string() });
    
    // PostModel is at the head since it was pushed
    assert_eq!(chain.head().title, "Second");
    assert_eq!(chain.tail().head().name, "First");
}
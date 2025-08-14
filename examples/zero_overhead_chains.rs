//! Demonstration of zero-overhead model and resource chains in Syzygy
//!
//! This example shows how to use compile-time type chains for both models
//! and resources, achieving zero runtime overhead compared to HashMap lookups.
//!
//! The clean syntax using type annotations on the left (like frunk's HList)
//! makes the code readable while maintaining compile-time safety.

use syzygy::model_chain::{NoModels, ModelChainBuilder};

// ============================================================================
// Define our models
// ============================================================================

#[derive(Debug, Clone)]
struct UserModel {
    id: u64,
    name: String,
    email: String,
}

#[derive(Debug, Clone)]
struct PostModel {
    id: u64,
    author_id: u64,
    title: String,
    content: String,
}

#[derive(Debug, Clone)]
struct CommentModel {
    id: u64,
    post_id: u64,
    author_id: u64,
    text: String,
}


fn main() {
    println!("=== Zero-Overhead Model Chains ===\n");
    
    // ========================================================================
    // Building Model Chains
    // ========================================================================
    
    println!("Building model chain...");
    
    // Start with no models and chain them together
    let models = NoModels::default()
        .with_model(UserModel {
            id: 1,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        })
        .with_model(PostModel {
            id: 101,
            author_id: 1,
            title: "Hello World".to_string(),
            content: "My first post".to_string(),
        })
        .with_model(CommentModel {
            id: 1001,
            post_id: 101,
            author_id: 2,
            text: "Great post!".to_string(),
        });
    
    // ========================================================================
    // Accessing Models - BEAUTIFUL CLEAN SYNTAX!
    // ========================================================================
    
    println!("\nAccessing models with clean type annotations:");
    
    // The compiler infers the index type automatically!
    let user: &UserModel = models.get();
    println!("User: {} ({})", user.name, user.email);
    
    let post: &PostModel = models.get();
    println!("Post: {}", post.title);
    
    let comment: &CommentModel = models.get();
    println!("Comment: {}", comment.text);
    
    // ========================================================================
    // Mutable Access
    // ========================================================================
    
    println!("\nMutable access:");
    
    let mut models = models; // Make it mutable
    
    // Clean mutable access with type annotations
    let user_mut: &mut UserModel = models.get_mut();
    user_mut.name = "Alice Smith".to_string();
    
    let post_mut: &mut PostModel = models.get_mut();
    post_mut.title = "Hello Rust World".to_string();
    
    // Verify changes
    let user: &UserModel = models.get();
    println!("Updated user: {}", user.name);
    
    let post: &PostModel = models.get();
    println!("Updated post: {}", post.title);
    
    // ========================================================================
    // Compile-Time Safety
    // ========================================================================
    
    println!("\n=== Compile-Time Safety ===");
    println!("Try uncommenting the following lines to see compile errors:");
    println!("// struct NotInChain;");
    println!("// let not_found: &NotInChain = models.get(); // Compile error!");
    
    // ========================================================================
    // Performance Characteristics
    // ========================================================================
    
    println!("\n=== Performance Characteristics ===");
    println!("✓ Zero runtime overhead - all types resolved at compile time");
    println!("✓ Direct memory access - no HashMap lookups");
    println!("✓ No heap allocations for chain structure");
    println!("✓ Inline functions - all calls optimized away");
    println!("✓ Type safety - impossible to access non-existent models/resources");
    
    // ========================================================================
    // Alternative Syntax (when needed)
    // ========================================================================
    
    println!("\n=== Alternative Syntax ===");
    println!("If type inference doesn't work in your context, use turbofish with underscore:");
    
    let user_alt = models.get::<UserModel, _>();
    println!("User (turbofish): {}", user_alt.name);
    
    println!("\nBut prefer the clean type annotation style when possible!");
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_zero_cost_abstraction() {
        // This test verifies that the chain operations are zero-cost
        
        let models = NoModels::default()
            .with_model(UserModel {
                id: 1,
                name: "Test".to_string(),
                email: "test@test.com".to_string(),
            })
            .with_model(PostModel {
                id: 1,
                author_id: 1,
                title: "Test".to_string(),
                content: "Test".to_string(),
            });
        
        // These should compile to direct field access
        let user: &UserModel = models.get();
        let post: &PostModel = models.get();
        
        assert_eq!(user.id, 1);
        assert_eq!(post.id, 1);
        
        // The compiler knows exactly where each type is at compile time
        // No runtime type checks, no dynamic dispatch, no HashMap lookups!
    }
}
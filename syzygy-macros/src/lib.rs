use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, DeriveInput, Data, Fields, Field, Meta
};

/// Derive macro for generating container field extractors
/// 
/// Generates `FromContainer` implementations for fields marked with:
/// - `#[extract]` - Uses the field's type directly 
/// - `#[extract(as = Name)]` - Creates a newtype wrapper
///
/// # Example
/// 
/// ```rust
/// #[derive(ModelExtractors)]
/// struct AppModel {
///     #[extract]
///     users: UserModel,
///     
///     #[extract(as = AuthUser)]
///     auth_user: User,
///     
///     #[extract(as = TargetUser)]
///     target_user: User,
///     
///     posts: PostModel,  // Not extracted
/// }
/// ```
#[proc_macro_derive(ModelExtractors, attributes(extract))]
pub fn derive_model_extractors(input: TokenStream) -> TokenStream {
    derive_extractors(input)
}

/// Derive macro for generating resource field extractors
/// 
/// Generates `FromContainer` implementations for fields marked with:
/// - `#[extract]` - Uses the field's type directly 
/// - `#[extract(as = Name)]` - Creates a newtype wrapper
///
/// # Example
/// 
/// ```rust
/// #[derive(ResourceExtractors)]
/// struct AppResources {
///     #[extract]
///     database: Database,
///     
///     #[extract(as = ApiClient)]
///     api_client: HttpClient,
///     
///     #[extract(as = AdminApi)]
///     admin_api: HttpClient,
///     
///     cache: Cache,  // Not extracted
/// }
/// ```
#[proc_macro_derive(ResourceExtractors, attributes(extract))]
pub fn derive_resource_extractors(input: TokenStream) -> TokenStream {
    derive_extractors(input)
}

fn derive_extractors(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    
    let struct_name = &input.ident;
    let data = match &input.data {
        Data::Struct(data) => data,
        _ => panic!("ModelExtractors can only be derived for structs"),
    };
    
    let fields = match &data.fields {
        Fields::Named(fields) => &fields.named,
        _ => panic!("ModelExtractors requires named fields"),
    };
    
    let mut generated = quote! {};
    let mut type_usage_count = std::collections::HashMap::new();
    
    // First pass: count field types to detect duplicates
    for field in fields {
        if has_extract_attr(field) {
            let field_type = &field.ty;
            let type_str = quote!(#field_type).to_string();
            *type_usage_count.entry(type_str).or_insert(0) += 1;
        }
    }
    
    // Second pass: generate extractors
    for field in fields {
        if let Some(attr) = get_extract_attr(field) {
            let field_name = field.ident.as_ref().unwrap();
            let field_type = &field.ty;
            let type_str = quote!(#field_type).to_string();
            
            match attr {
                ExtractAttr::Direct => {
                    // Check for type collision
                    if type_usage_count[&type_str] > 1 {
                        panic!("Field '{}' has type {} which appears multiple times. Use #[extract(as = \"Name\")] to disambiguate", 
                               field_name, type_str);
                    }
                    
                    // Generate direct FromContainer impl
                    let impl_block = quote! {
                        impl syzygy::extract::FromContainer<#struct_name> for #field_type
                        where
                            #field_type: Clone,
                        {
                            fn from_container(container: &#struct_name) -> Self {
                                container.#field_name.clone()
                            }
                        }
                    };
                    generated.extend(impl_block);
                }
                ExtractAttr::As(wrapper_name) => {
                    let wrapper_ident = syn::Ident::new(&wrapper_name, proc_macro2::Span::call_site());
                    
                    // Generate wrapper type with Deref and FromContainer impls
                    let impl_block = quote! {
                        #[derive(Debug, Clone)]
                        pub struct #wrapper_ident(pub #field_type);
                        
                        impl std::ops::Deref for #wrapper_ident {
                            type Target = #field_type;
                            
                            fn deref(&self) -> &Self::Target {
                                &self.0
                            }
                        }
                        
                        impl std::ops::DerefMut for #wrapper_ident {
                            fn deref_mut(&mut self) -> &mut Self::Target {
                                &mut self.0
                            }
                        }
                        
                        impl syzygy::extract::FromContainer<#struct_name> for #wrapper_ident {
                            fn from_container(container: &#struct_name) -> Self {
                                #wrapper_ident(container.#field_name.clone())
                            }
                        }
                    };
                    generated.extend(impl_block);
                }
            }
        }
    }
    
    generated.into()
}


#[derive(Debug)]
enum ExtractAttr {
    Direct,
    As(String),
}

fn has_extract_attr(field: &Field) -> bool {
    field.attrs.iter().any(|attr| {
        attr.path().is_ident("extract")
    })
}

fn get_extract_attr(field: &Field) -> Option<ExtractAttr> {
    for attr in &field.attrs {
        if attr.path().is_ident("extract") {
            return match &attr.meta {
                Meta::Path(_) => Some(ExtractAttr::Direct),
                Meta::List(meta) => {
                    // Parse extract(as = Name) format
                    let content = meta.tokens.to_string();
                    
                    // Look for "as = identifier" pattern
                    if content.starts_with("as = ") {
                        let ident_str = content[5..].trim();
                        
                        // Validate it's a valid identifier (basic check)
                        if ident_str.chars().all(|c| c.is_alphanumeric() || c == '_') 
                           && !ident_str.starts_with(char::is_numeric)
                           && !ident_str.is_empty() {
                            return Some(ExtractAttr::As(ident_str.to_string()));
                        }
                    }
                    
                    panic!("Invalid extract attribute format. Use #[extract] or #[extract(as = Name)]");
                }
                _ => panic!("Invalid extract attribute format. Use #[extract] or #[extract(as = Name)]"),
            };
        }
    }
    None
}
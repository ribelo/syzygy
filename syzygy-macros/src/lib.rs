use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, DeriveInput, Data, Fields, Field, Meta
};
use std::collections::HashSet;

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

/// Derive macro for Event trait implementation
/// 
/// Generates the Event trait implementation for enums with typed variants.
/// All variant types must be unique - no two variants can have the same inner type.
/// 
/// Also generates From<T> implementations for each variant type.
/// 
/// # Example
/// 
/// ```rust
/// #[derive(Event)]
/// enum AppEvent {
///     UserCreated(UserCreatedData),
///     UserUpdated(UserUpdatedData),
///     UserDeleted(UserDeletedData),
/// }
/// ```
/// 
/// This will generate:
/// - Event trait implementation with LENGTH, variant_index(), inner_as_any(), inner_type_id()
/// - From<UserCreatedData> for AppEvent
/// - From<UserUpdatedData> for AppEvent
/// - From<UserDeletedData> for AppEvent
#[proc_macro_derive(Event)]
pub fn derive_event(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    
    let enum_name = &input.ident;
    
    // Extract enum variants
    let variants = match &input.data {
        Data::Enum(data) => &data.variants,
        _ => panic!("Event can only be derived for enums"),
    };
    
    // Validate that all variants have exactly one unnamed field (tuple variant)
    // and collect the inner types
    let mut inner_types = Vec::new();
    let mut type_set = HashSet::new();
    let mut variant_names = Vec::new();
    
    for variant in variants {
        variant_names.push(&variant.ident);
        
        match &variant.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let field = &fields.unnamed[0];
                let ty = &field.ty;
                
                // Extract the type string for uniqueness check
                let type_str = quote!(#ty).to_string().replace(" ", "");
                
                if !type_set.insert(type_str.clone()) {
                    panic!(
                        "Event variant {} has duplicate inner type {}. Each variant must have a unique inner type.",
                        variant.ident, type_str
                    );
                }
                
                inner_types.push(ty);
            }
            _ => panic!(
                "Event variant {} must have exactly one unnamed field, e.g., {}(MyData)",
                variant.ident, variant.ident
            ),
        }
    }
    
    let variant_count = variants.len();
    
    // Generate variant_index match arms
    let variant_index_arms = variant_names.iter().enumerate().map(|(i, name)| {
        quote! {
            #enum_name::#name(_) => #i
        }
    });
    
    // Generate inner_as_any match arms
    let inner_as_any_arms = variant_names.iter().map(|name| {
        quote! {
            #enum_name::#name(data) => data
        }
    });
    
    // Generate inner_type_id match arms
    let inner_type_id_arms = variant_names.iter().zip(&inner_types).map(|(name, ty)| {
        quote! {
            #enum_name::#name(_) => std::any::TypeId::of::<#ty>()
        }
    });
    
    // Generate From implementations for each inner type
    let from_impls = variant_names.iter().zip(&inner_types).map(|(name, ty)| {
        quote! {
            impl From<#ty> for #enum_name {
                fn from(value: #ty) -> Self {
                    #enum_name::#name(value)
                }
            }
        }
    });
    
    // Generate From implementations from enum to each inner type (for EventMap optimization)
    let reverse_from_impls = variant_names.iter().zip(&inner_types).map(|(name, ty)| {
        quote! {
            impl From<#enum_name> for #ty {
                fn from(event: #enum_name) -> Self {
                    match event {
                        #enum_name::#name(data) => data,
                        _ => panic!("Invalid conversion from {} to {}", stringify!(#enum_name), stringify!(#ty)),
                    }
                }
            }
        }
    });
    
    // Generate EventVariant implementations for each inner type
    let event_variant_impls = variant_names.iter().enumerate().zip(&inner_types).map(|((i, _name), ty)| {
        quote! {
            impl syzygy::event_map::EventVariant for #ty {
                const VARIANT_INDEX: usize = #i;
            }
        }
    });
    
    // Generate call_handler_with_data match arms
    let call_handler_arms = variant_names.iter().zip(&inner_types).map(|(name, ty)| {
        quote! {
            #enum_name::#name(data) => {
                type HandlerType<M, C> = fn(#ty, &mut M) -> syzygy::dispatch::Dispatch<#enum_name, C>;
                let handler = std::mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr);
                handler(data, model)
            }
        }
    });
    
    // Generate the Event trait implementation
    let output = quote! {
        impl syzygy::event_map::Event for #enum_name {
            const LENGTH: usize = #variant_count;
            type Array<V> = [V; #variant_count];
            
            fn variant_index(&self) -> usize {
                match self {
                    #(#variant_index_arms,)*
                }
            }
            
            fn inner_as_any(&self) -> &dyn std::any::Any {
                match self {
                    #(#inner_as_any_arms,)*
                }
            }
            
            fn inner_type_id(&self) -> std::any::TypeId {
                match self {
                    #(#inner_type_id_arms,)*
                }
            }
            
            unsafe fn call_handler_with_data<M, C>(
                self,
                handler_ptr: *const (),
                model: &mut M,
            ) -> syzygy::dispatch::Dispatch<Self, C> {
                match self {
                    #(#call_handler_arms,)*
                }
            }
        }
        
        #(#from_impls)*
        #(#reverse_from_impls)*
        #(#event_variant_impls)*
    };
    
    output.into()
}
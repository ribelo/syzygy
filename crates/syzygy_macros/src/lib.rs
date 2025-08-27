use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Error, Fields};

/// Derive macro for generating magic handler variants
///
/// Generates struct variants and From implementations for magic handlers.
/// This is the clean, simple approach without the complexity of EventHandlerMap.
///
/// # Example
///
/// ```rust
/// #[derive(MagicVariants)]
/// enum MyEvent {
///     Click { x: i32, y: i32 },
///     KeyPress { key: String },
///     UserLogin { username: String },
/// }
/// ```
///
/// This generates:
/// - `struct Click { x: i32, y: i32 }`
/// - `struct KeyPress { key: String }`
/// - `struct UserLogin { username: String }`
/// - `impl TryFrom<MyEvent> for Click { ... }`
/// - `impl TryFrom<MyEvent> for KeyPress { ... }`
/// - `impl TryFrom<MyEvent> for UserLogin { ... }`
#[proc_macro_derive(MagicVariants)]
pub fn derive_magic_variants(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let enum_name = &input.ident;

    // Extract enum variants
    let variants = match &input.data {
        Data::Enum(data) => &data.variants,
        _ => panic!("MagicVariants can only be derived for enums"),
    };

    let mut variant_structs = Vec::new();
    let mut from_impls = Vec::new();

    for variant in variants {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Named(_) => {
                // Named variants are not supported - return compile error
                return Error::new_spanned(
                    variant,
                    format!("MagicVariants does not support named variants like `{} {{ ... }}`. Use tuple variants like `{}(YourStruct)` where YourStruct is already defined.", variant_name, variant_name)
                ).to_compile_error().into();
            }
            Fields::Unit => {
                // Unit variants
                variant_structs.push(quote! {
                    struct #variant_name;
                });

                // Generate From implementation: struct -> enum (for Command::effect)
                from_impls.push(quote! {
                    impl From<#variant_name> for #enum_name {
                        fn from(_variant: #variant_name) -> Self {
                            #enum_name::#variant_name
                        }
                    }
                });

                // Generate TryFrom implementation: enum -> struct (for magic handlers)
                from_impls.push(quote! {
                    impl TryFrom<#enum_name> for #variant_name {
                        type Error = &'static str;

                        fn try_from(event: #enum_name) -> Result<Self, Self::Error> {
                            match event {
                                #enum_name::#variant_name => Ok(Self),
                                _ => Err(concat!("Expected ", stringify!(#variant_name))),
                            }
                        }
                    }
                });
            }
            Fields::Unnamed(fields) => {
                if fields.unnamed.len() == 1 {
                    // Single tuple variant - assume the struct is already defined
                    // Just generate the From implementation
                    let inner_type = &fields.unnamed[0].ty;

                    // Generate From implementation: struct -> enum (for Command::effect)
                    from_impls.push(quote! {
                        impl From<#inner_type> for #enum_name {
                            fn from(variant: #inner_type) -> Self {
                                #enum_name::#variant_name(variant)
                            }
                        }
                    });

                    // Generate TryFrom implementation: enum -> struct (for magic handlers)
                    from_impls.push(quote! {
                        impl TryFrom<#enum_name> for #inner_type {
                            type Error = &'static str;

                            fn try_from(event: #enum_name) -> Result<Self, Self::Error> {
                                match event {
                                    #enum_name::#variant_name(inner) => Ok(inner),
                                    _ => Err(concat!("Expected ", stringify!(#variant_name))),
                                }
                            }
                        }
                    });
                } else {
                    panic!(
                        "MagicVariants only supports single-field tuple variants: {}(Type)",
                        variant_name
                    );
                }
            }
        }
    }

    let output = quote! {
        #(#variant_structs)*
        #(#from_impls)*
    };

    output.into()
}

use std::collections::HashSet;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use quote::ToTokens;
use syn::parse_macro_input;
use syn::spanned::Spanned;
use syn::Attribute;
use syn::Data;
use syn::DeriveInput;
use syn::Fields;
use syn::Generics;
use syn::Ident;

#[proc_macro_derive(Model, attributes(model, extract))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_model(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

fn expand_model(input: DeriveInput) -> syn::Result<TokenStream2> {
    let DeriveInput {
        ident: model_ident,
        generics,
        data,
        attrs,
        ..
    } = input;

    ensure_not_packed(&attrs)?;
    let fields = named_fields(&data)?;

    let mut wrappers = Vec::with_capacity(fields.len());
    let mut extract_impls = Vec::new();
    let mut tracked_field_count = 0usize;
    let mut part_types = HashSet::new();
    let mut wrapper_names = HashSet::new();

    for field in fields {
        let field_ident = field.ident.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(field, "Model derive requires named struct fields")
        })?;
        let field_ty = &field.ty;
        let field_name = field_ident.to_string();
        let field_mode = parse_field_mode(field)?;

        let (struct_generics, impl_generics, ty_generics, where_clause) = split_generics(&generics);

        let Some(field_mode) = field_mode else {
            continue;
        };

        tracked_field_count += 1;
        if tracked_field_count > 256 {
            return Err(syn::Error::new_spanned(
                model_ident.clone(),
                "Model derive supports up to 256 #[model(...)] fields for runtime borrow checks",
            ));
        }
        let field_index = (tracked_field_count - 1) as u32;

        match field_mode {
            FieldMode::Part => {
                let part_type_key = quote!(#field_ty).to_string();
                if !part_types.insert(part_type_key) {
                    return Err(syn::Error::new_spanned(
                        field_ty,
                        "duplicate #[model(part)] field type in this model; each part type must be unique",
                    ));
                }

                extract_impls.push(quote! {
                    impl #impl_generics syzygy::extract::Part<#model_ident #ty_generics> for #field_ty #where_clause {
                        #[track_caller]
                        fn extract(ctx: &syzygy::extract::EventContext<#model_ident #ty_generics>) -> &Self {
                            // SAFETY: EventContext stores a valid pointer to the active model during dispatch.
                            let ptr = unsafe { ::core::ptr::addr_of!((*ctx.model_ptr()).#field_ident) };
                            ctx.track_field_immut(#field_index, #field_name);
                            // SAFETY: `ptr` points to the extracted field for the lifetime of this dispatch step.
                            unsafe { &*ptr }
                        }
                    }

                    #[allow(unexpected_cfgs)]
                    #[cfg(feature = "shell")]
                    impl #impl_generics syzygy::extract::SubscriptionPart<#model_ident #ty_generics> for #field_ty #where_clause {
                        #[track_caller]
                        fn extract(ctx: &syzygy::extract::SubscriptionContext<#model_ident #ty_generics>) -> &Self {
                            // SAFETY: SubscriptionContext stores a valid pointer to the active model during reconciliation.
                            let ptr = unsafe { ::core::ptr::addr_of!((*ctx.model_ptr()).#field_ident) };
                            // SAFETY: `ptr` points to the extracted field for the lifetime of this reconciliation step.
                            unsafe { &*ptr }
                        }
                    }

                    #[allow(clippy::mut_from_ref)]
                    impl #impl_generics syzygy::extract::PartMut<#model_ident #ty_generics> for #field_ty #where_clause {
                        #[track_caller]
                        fn extract_mut(ctx: &syzygy::extract::EventContext<#model_ident #ty_generics>) -> &mut Self {
                            ctx.track_field_borrow(#field_index, #field_name);
                            // SAFETY: EventContext stores a valid mutable pointer for the active handler call,
                            // and borrow tracking ensures this field is extracted at most once per handler.
                            let ptr = unsafe { ::core::ptr::addr_of_mut!((*ctx.model_ptr()).#field_ident) };
                            // SAFETY: `ptr` points to the extracted field and runtime tracking enforces exclusivity.
                            unsafe { &mut *ptr }
                        }
                    }
                });
            }
            FieldMode::Wrapper(wrapper_ident) => {
                let wrapper_name = wrapper_ident.to_string();
                if !wrapper_names.insert(wrapper_name) {
                    return Err(syn::Error::new_spanned(
                        wrapper_ident,
                        "duplicate #[model(wrapper = ...)] name in this model",
                    ));
                }

                let option_helpers = option_inner_type(field_ty).map_or_else(TokenStream2::new, |inner_ty| {
                    quote! {
                        impl #impl_generics #wrapper_ident #ty_generics #where_clause {
                            #[must_use]
                            pub fn is_some(&self) -> bool {
                                self.0.is_some()
                            }

                            #[must_use]
                            pub fn is_none(&self) -> bool {
                                self.0.is_none()
                            }

                            #[must_use]
                            pub fn as_ref(&self) -> ::core::option::Option<&#inner_ty> {
                                self.0.as_ref()
                            }

                            pub fn as_mut(&mut self) -> ::core::option::Option<&mut #inner_ty> {
                                self.0.as_mut()
                            }

                            pub fn insert(&mut self, value: #inner_ty) -> &mut #inner_ty {
                                self.0.insert(value)
                            }

                            pub fn get_or_insert(&mut self, value: #inner_ty) -> &mut #inner_ty {
                                self.0.get_or_insert(value)
                            }

                            pub fn take(&mut self) -> ::core::option::Option<#inner_ty> {
                                self.0.take()
                            }

                            pub fn clear(&mut self) {
                                self.0 = ::core::option::Option::None;
                            }
                        }
                    }
                });

                wrappers.push(quote! {
                    #[repr(transparent)]
                    pub struct #wrapper_ident #struct_generics (#field_ty) #where_clause;

                    impl #impl_generics #wrapper_ident #ty_generics #where_clause {
                        #[allow(clippy::ref_option)]
                        #[must_use]
                        pub fn get(&self) -> &#field_ty {
                            &self.0
                        }

                        #[allow(clippy::ref_option)]
                        pub fn get_mut(&mut self) -> &mut #field_ty {
                            &mut self.0
                        }

                        pub fn set(&mut self, value: #field_ty) {
                            self.0 = value;
                        }

                        #[must_use]
                        pub fn replace(&mut self, value: #field_ty) -> #field_ty {
                            ::core::mem::replace(&mut self.0, value)
                        }
                    }

                    impl #impl_generics ::core::ops::Deref for #wrapper_ident #ty_generics #where_clause {
                        type Target = #field_ty;

                        fn deref(&self) -> &Self::Target {
                            &self.0
                        }
                    }

                    impl #impl_generics ::core::ops::DerefMut for #wrapper_ident #ty_generics #where_clause {
                        fn deref_mut(&mut self) -> &mut Self::Target {
                            &mut self.0
                        }
                    }

                    impl #impl_generics syzygy::extract::Part<#model_ident #ty_generics> for #wrapper_ident #ty_generics #where_clause {
                        #[track_caller]
                        fn extract(ctx: &syzygy::extract::EventContext<#model_ident #ty_generics>) -> &Self {
                            // SAFETY: `repr(transparent)` guarantees Wrapper has the same layout as the field type.
                            let ptr = unsafe {
                                ::core::ptr::addr_of!((*ctx.model_ptr()).#field_ident).cast::<Self>()
                            };
                            ctx.track_field_immut(#field_index, #field_name);
                            // SAFETY: `ptr` points to the wrapped field for the current dispatch lifetime.
                            unsafe { &*ptr }
                        }
                    }

                    #[allow(unexpected_cfgs)]
                    #[cfg(feature = "shell")]
                    impl #impl_generics syzygy::extract::SubscriptionPart<#model_ident #ty_generics> for #wrapper_ident #ty_generics #where_clause {
                        #[track_caller]
                        fn extract(ctx: &syzygy::extract::SubscriptionContext<#model_ident #ty_generics>) -> &Self {
                            // SAFETY: `repr(transparent)` guarantees Wrapper has the same layout as the field type.
                            let ptr = unsafe {
                                ::core::ptr::addr_of!((*ctx.model_ptr()).#field_ident).cast::<Self>()
                            };
                            // SAFETY: `ptr` points to the wrapped field for the current reconciliation lifetime.
                            unsafe { &*ptr }
                        }
                    }

                    #[allow(clippy::mut_from_ref)]
                    impl #impl_generics syzygy::extract::PartMut<#model_ident #ty_generics> for #wrapper_ident #ty_generics #where_clause {
                        #[track_caller]
                        fn extract_mut(ctx: &syzygy::extract::EventContext<#model_ident #ty_generics>) -> &mut Self {
                            ctx.track_field_borrow(#field_index, #field_name);
                            // SAFETY: `repr(transparent)` guarantees Wrapper has the same layout as the field type.
                            let ptr = unsafe {
                                ::core::ptr::addr_of_mut!((*ctx.model_ptr()).#field_ident).cast::<Self>()
                            };
                            // SAFETY: `track_field_borrow` ensures unique mutable access for this field in the handler.
                            unsafe { &mut *ptr }
                        }
                    }

                    #option_helpers
                });
            }
        }
    }

    Ok(quote! {
        #(#wrappers)*
        #(#extract_impls)*
    })
}

fn ensure_not_packed(attrs: &[Attribute]) -> syn::Result<()> {
    for attr in attrs {
        if !attr.path().is_ident("repr") {
            continue;
        }

        let tokens = attr.meta.to_token_stream().to_string();
        if tokens.contains("packed") {
            return Err(syn::Error::new_spanned(
                attr,
                "Syzygy #[derive(Model)] prohibits #[repr(packed)] because creating references to packed fields can trigger unaligned-reference UB",
            ));
        }
    }

    Ok(())
}

#[derive(Debug)]
enum FieldMode {
    Part,
    Wrapper(Ident),
}

fn parse_field_mode(field: &syn::Field) -> syn::Result<Option<FieldMode>> {
    let mut mode = None;

    for attr in &field.attrs {
        if attr.path().is_ident("extract") {
            return Err(syn::Error::new_spanned(
                attr,
                "legacy #[extract] is removed; use #[model(part)] instead",
            ));
        }

        if !attr.path().is_ident("model") {
            continue;
        }

        let syn::Meta::List(_) = &attr.meta else {
            return Err(syn::Error::new_spanned(
                attr,
                "#[model(...)] requires arguments: use #[model(part)] or #[model(wrapper = Name)]",
            ));
        };

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("part") {
                if !meta.input.is_empty() {
                    return Err(meta.error(
                        "#[model(part)] does not accept a value; use #[model(wrapper = Name)] for named wrappers",
                    ));
                }

                set_field_mode(&mut mode, FieldMode::Part, meta.path.span())
            } else if meta.path.is_ident("wrapper") {
                let value = meta.value()?;
                let wrapper_ident: Ident = value.parse()?;
                set_field_mode(&mut mode, FieldMode::Wrapper(wrapper_ident), meta.path.span())
            } else {
                Err(meta.error("expected `part` or `wrapper = Name`"))
            }
        })?;
    }

    Ok(mode)
}

fn named_fields(
    data: &Data,
) -> syn::Result<&syn::punctuated::Punctuated<syn::Field, syn::token::Comma>> {
    match data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => Ok(&fields.named),
            _ => Err(syn::Error::new_spanned(
                &data.fields,
                "derive only supports structs with named fields",
            )),
        },
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "derive only supports structs",
        )),
    }
}

fn set_field_mode(
    slot: &mut Option<FieldMode>,
    new_mode: FieldMode,
    span: proc_macro2::Span,
) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new(
            span,
            "field extraction mode already specified; use a single #[model(...)] mode per field",
        ));
    }

    *slot = Some(new_mode);
    Ok(())
}

fn split_generics(
    generics: &Generics,
) -> (
    TokenStream2,
    syn::ImplGenerics<'_>,
    syn::TypeGenerics<'_>,
    Option<&syn::WhereClause>,
) {
    let struct_generics = if generics.params.is_empty() {
        quote! {}
    } else {
        let params = &generics.params;
        quote! { <#params> }
    };
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    (struct_generics, impl_generics, ty_generics, where_clause)
}

fn option_inner_type(field_ty: &syn::Type) -> Option<syn::Type> {
    let syn::Type::Path(type_path) = field_ty else {
        return None;
    };
    if type_path.qself.is_some() {
        return None;
    }

    let segment = type_path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }

    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let first = arguments.args.first()?;
    let syn::GenericArgument::Type(inner_ty) = first else {
        return None;
    };
    Some(inner_ty.clone())
}

use std::collections::HashSet;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::format_ident;
use quote::quote;
use syn::parse_macro_input;
use syn::Data;
use syn::DeriveInput;
use syn::Fields;
use syn::Generics;
use syn::Ident;

#[proc_macro_derive(Model, attributes(extract))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_model(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

fn expand_model(input: DeriveInput) -> syn::Result<TokenStream2> {
    let model_ident = input.ident;
    let generics = input.generics;
    let fields = named_fields(&input.data)?;

    let mut wrappers = Vec::with_capacity(fields.len());
    let mut extract_impls = Vec::new();
    let mut extract_types = HashSet::new();

    for field in fields {
        let field_ident = field.ident.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(field, "Model derive requires named struct fields")
        })?;
        let field_ty = &field.ty;
        let is_extract = has_extract_attr(field)?;

        let (struct_generics, impl_generics, ty_generics, where_clause) = split_generics(&generics);

        if is_extract {
            let field_name = field_ident.to_string();
            let extract_type_key = quote!(#field_ty).to_string();
            if !extract_types.insert(extract_type_key) {
                return Err(syn::Error::new_spanned(
                    field_ty,
                    "duplicate #[extract] field type in this model; each extracted type must be unique",
                ));
            }

            extract_impls.push(quote! {
                #[allow(clippy::mut_from_ref)]
                impl #impl_generics ::syzygy::extract::ExtractMutFrom<#model_ident #ty_generics> for #field_ty #where_clause {
                    fn extract_mut(ctx: &::syzygy::extract::EventContext<#model_ident #ty_generics>) -> &mut Self {
                        // SAFETY: EventContext stores a valid mutable pointer for the active handler call.
                        let ptr = unsafe { ::core::ptr::addr_of_mut!((*ctx.model_ptr()).#field_ident) };
                        ctx.track_borrow_range(
                            ptr.cast::<u8>(),
                            ::core::mem::size_of::<#field_ty>(),
                            #field_name,
                        );
                        // SAFETY: Borrow ranges are checked in debug builds and handler execution is single-threaded per event.
                        unsafe { &mut *ptr }
                    }
                }
            });

            continue;
        }

        let wrapper_ident = field_wrapper_ident(field_ident)?;

        wrappers.push(quote! {
            pub struct #wrapper_ident #struct_generics (*mut #field_ty) #where_clause;

            impl #impl_generics ::std::ops::Deref for #wrapper_ident #ty_generics #where_clause {
                type Target = #field_ty;

                fn deref(&self) -> &Self::Target {
                    // SAFETY: Wrapper instances are constructed from valid pointers to model fields.
                    unsafe { &*self.0 }
                }
            }

            impl #impl_generics ::std::ops::DerefMut for #wrapper_ident #ty_generics #where_clause {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    // SAFETY: Wrapper points to a valid mutable model field for this dispatch.
                    unsafe { &mut *self.0 }
                }
            }

            impl #impl_generics ::syzygy::extract::FromEventContext<#model_ident #ty_generics> for #wrapper_ident #ty_generics #where_clause {
                fn from_context(ctx: &::syzygy::extract::EventContext<#model_ident #ty_generics>) -> Self {
                    // SAFETY: EventContext points to the active model for this dispatch.
                    let ptr = unsafe { &mut (*ctx.model_ptr()).#field_ident };
                    Self(ptr)
                }
            }
        });
    }

    Ok(quote! {
        #(#wrappers)*
        #(#extract_impls)*
    })
}

fn has_extract_attr(field: &syn::Field) -> syn::Result<bool> {
    let mut is_extract = false;

    for attr in &field.attrs {
        if !attr.path().is_ident("extract") {
            continue;
        }

        if !matches!(&attr.meta, syn::Meta::Path(_)) {
            return Err(syn::Error::new_spanned(
                attr,
                "`#[extract]` does not accept arguments",
            ));
        }

        is_extract = true;
    }

    Ok(is_extract)
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

fn field_wrapper_ident(field_ident: &Ident) -> syn::Result<Ident> {
    let field_name = field_ident.to_string();
    let mut name = String::new();

    for segment in field_name.split('_') {
        if segment.is_empty() {
            continue;
        }

        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            name.extend(first.to_uppercase());
            name.push_str(chars.as_str());
        }
    }

    if name.is_empty() {
        return Err(syn::Error::new_spanned(
            field_ident,
            "field name must contain at least one alphanumeric character",
        ));
    }

    Ok(format_ident!("{}", name, span = field_ident.span()))
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

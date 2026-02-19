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

#[proc_macro_derive(Model)]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_model(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

#[proc_macro_derive(Resources)]
pub fn derive_resources(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_resources(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

fn expand_model(input: DeriveInput) -> syn::Result<TokenStream2> {
    let model_ident = input.ident;
    let generics = input.generics;
    let fields = named_fields(&input.data)?;

    if fields.len() > 64 {
        return Err(syn::Error::new_spanned(
            model_ident,
            "Model derive supports up to 64 fields for runtime borrow tracking",
        ));
    }

    let mut wrappers = Vec::with_capacity(fields.len());

    for (index, field) in fields.iter().enumerate() {
        let field_ident = field.ident.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(field, "Model derive requires named struct fields")
        })?;
        let wrapper_ident = field_wrapper_ident(field_ident)?;
        let field_ty = &field.ty;
        let field_name = field_ident.to_string();
        let field_index = index as u32;

        let (struct_generics, impl_generics, ty_generics, where_clause) = split_generics(&generics);

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
                    // SAFETY: Runtime borrow tracking ensures unique mutable access for this field.
                    unsafe { &mut *self.0 }
                }
            }

            impl #impl_generics ::syzygy::extract::FromEventContext<#model_ident #ty_generics> for #wrapper_ident #ty_generics #where_clause {
                fn from_context(ctx: &::syzygy::extract::EventContext<#model_ident #ty_generics>) -> Self {
                    ctx.track_borrow(#field_index, #field_name);
                    // SAFETY: `track_borrow` guarantees exclusive mutable access for this field.
                    let ptr = unsafe { &mut (*ctx.model_ptr()).#field_ident };
                    Self(ptr)
                }
            }
        });
    }

    Ok(quote! {
        #(#wrappers)*
    })
}

fn expand_resources(input: DeriveInput) -> syn::Result<TokenStream2> {
    let resources_ident = input.ident;
    let generics = input.generics;
    let fields = named_fields(&input.data)?;

    let mut wrappers = Vec::with_capacity(fields.len());

    for field in fields {
        let field_ident = field.ident.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(field, "Resources derive requires named struct fields")
        })?;
        let wrapper_ident = field_wrapper_ident(field_ident)?;
        let field_ty = &field.ty;

        let (struct_generics, impl_generics, ty_generics, where_clause) = split_generics(&generics);

        wrappers.push(quote! {
            pub struct #wrapper_ident #struct_generics (pub #field_ty) #where_clause;

            impl #impl_generics ::syzygy::extract::FromEffectContext<#resources_ident #ty_generics> for #wrapper_ident #ty_generics #where_clause {
                fn from_context(ctx: &::syzygy::extract::EffectContext<#resources_ident #ty_generics>) -> Self {
                    Self(ctx.resources().#field_ident.clone())
                }
            }
        });
    }

    Ok(quote! {
        #(#wrappers)*
    })
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

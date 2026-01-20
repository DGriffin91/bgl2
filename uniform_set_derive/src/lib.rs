use quote::quote;
use syn::{
    Data, DeriveInput, Fields, GenericArgument, PathArguments, Type, TypePath, parse_macro_input,
    spanned::Spanned,
};

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::Span;
use syn::Ident;

fn bevy_opengl_path() -> proc_macro2::TokenStream {
    match crate_name("bevy_opengl") {
        Ok(FoundCrate::Name(name)) => {
            let ident = Ident::new(&name, Span::call_site());
            quote!(::#ident)
        }
        Ok(FoundCrate::Itself) | Err(_) => quote!(::bevy_opengl),
    }
}

#[proc_macro_derive(UniformSet)]
pub fn derive_uniform_set(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let ident = &input.ident;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn::Error::new(
                    input.span(),
                    "UniformSet derive only supports structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new(input.span(), "UniformSet derive only supports structs")
                .to_compile_error()
                .into();
        }
    };

    let mut name_entries = Vec::with_capacity(fields.len());

    let mut load_arms = Vec::with_capacity(fields.len());

    let crate_path = bevy_opengl_path();

    for (i, field) in fields.iter().enumerate() {
        let Some(field_ident) = &field.ident else {
            continue;
        };
        let field_name = field_ident.to_string();

        let is_tex = is_glow_texture(&field.ty)
            | is_handle_image(&field.ty)
            | is_option_handle_image(&field.ty);
        name_entries.push(quote! { (#field_name, #is_tex) });

        let idx = i as u32;

        if is_tex {
            load_arms.push(quote! {
                #idx => {
                    #crate_path::load_tex_if_new(&self.#field_ident.clone().into(), gl, gpu_images, slot);
                }
            });
        } else {
            load_arms.push(quote! {
                #idx => #crate_path::load_if_new(&self.#field_ident, gl, slot, temp)
            });
        }
    }

    let expanded = quote! {
        impl #crate_path::UniformSet for #ident {
            fn names() -> &'static [(&'static str, bool)] {
                &[
                    #(#name_entries,)*
                ]
            }

            fn load(
                &self,
                gl: &glow::Context,
                gpu_images: &#crate_path::prepare_image::GpuImages,
                index: u32,
                slot: &mut #crate_path::SlotData,
                temp: &mut #crate_path::faststack::StackStack<u32, 16>,
            ) {
                match index {
                    #(#load_arms,)*
                    _ => unreachable!(),
                }
            }
        }
    };

    expanded.into()
}

fn as_type_path(ty: &Type) -> Option<&TypePath> {
    match ty {
        Type::Path(tp) => Some(tp),
        _ => None,
    }
}

fn is_glow_texture(ty: &Type) -> bool {
    let Some(tp) = as_type_path(ty) else {
        return false;
    };
    let Some(last) = &tp.path.segments.last() else {
        return false;
    };
    last.ident == "Texture"
}

fn is_handle_image(ty: &Type) -> bool {
    let Some(tp) = as_type_path(ty) else {
        return false;
    };
    let Some(last) = &tp.path.segments.last() else {
        return false;
    };
    if last.ident != "Handle" {
        return false;
    }
    // Handle<...>
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return false;
    };
    // Look for a type argument whose last path segment ident is "Image"
    args.args.iter().any(|arg| match arg {
        GenericArgument::Type(Type::Path(inner_tp)) => inner_tp
            .path
            .segments
            .last()
            .map(|s| s.ident == "Image")
            .unwrap_or(false),
        _ => false,
    })
}

fn is_option_handle_image(ty: &Type) -> bool {
    let Some(tp) = as_type_path(ty) else {
        return false;
    };
    let Some(last) = &tp.path.segments.last() else {
        return false;
    };
    if last.ident != "Option" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return false;
    };
    // Option<T> where T is Handle<Image>
    args.args.iter().any(|arg| match arg {
        GenericArgument::Type(inner_ty) => is_handle_image(inner_ty),
        _ => false,
    })
}

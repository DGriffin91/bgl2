use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Field, Fields, GenericArgument, LitStr, PathArguments, Type,
    TypePath, parse_macro_input, spanned::Spanned,
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

#[proc_macro_derive(UniformSet, attributes(array_max, base_type, exclude))]
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
    let mut glsl_bindings = Vec::with_capacity(fields.len());

    let mut load_arms = Vec::with_capacity(fields.len());

    let crate_path = bevy_opengl_path();

    for (i, field) in fields.iter().enumerate() {
        let Some(field_ident) = &field.ident else {
            continue;
        };
        if has_attr(&field.attrs, "exclude") {
            continue;
        }
        let field_name = field_ident.to_string();

        let is_tex = is_glow_texture(&field.ty)
            | is_texture_ref(&field.ty)
            | is_handle_image(&field.ty)
            | is_option_handle_image(&field.ty);
        name_entries.push(quote! { (#field_name, #is_tex) });

        let binding = get_glsl_binding(&field, &field_name, is_tex);
        glsl_bindings.push(quote! { #binding });

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

            fn bindings() -> &'static [&'static str] {
                &[
                    #(#glsl_bindings,)*
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

fn is_texture_ref(ty: &Type) -> bool {
    let Some(tp) = as_type_path(ty) else {
        return false;
    };
    let Some(last) = &tp.path.segments.last() else {
        return false;
    };
    last.ident == "TextureRef"
}

fn get_glsl_binding(field: &Field, field_name: &str, texture: bool) -> String {
    let ty = &field.ty;
    let Some(tp) = as_type_path(ty) else {
        panic!("unrecognized type {ty:?}")
    };
    let Some(last) = &tp.path.segments.last() else {
        panic!("unrecognized type {ty:?}")
    };

    let ty_str = last.ident.to_string();

    let array_type = vec_of(ty);
    let explicit_type = parse_attr_str(&field.attrs, "base_type").map(|v| v.value());
    let gl_ty = if let Some(explicit_type) = &explicit_type {
        explicit_type.as_str()
    } else {
        let base_ty = if let Some(array_type) = &array_type {
            array_type.as_str()
        } else {
            ty_str.as_str()
        };
        if texture {
            "sampler2D"
        } else {
            match base_ty {
                "f32" => "float",
                "Vec2" => "vec2",
                "Vec3" => "vec3",
                "Vec4" => "vec4",
                "i32" => "int",
                "IVec2" => "ivec2",
                "IVec3" => "ivec3",
                "IVec4" => "ivec4",
                "Mat2" => "mat2",
                "Mat3" => "mat3",
                "Mat4" => "mat4",
                "bool" => "bool",
                _ => panic!("unrecognized type {base_ty}"),
            }
        }
    };

    let arr_max = if array_type.is_some() {
        let arr_max = parse_attr_str(&field.attrs, "array_max")
            .expect(&format!("Vec field {field_name:?} is missing array_max()"))
            .value();
        format!("[{arr_max}]")
    } else {
        String::from("")
    };

    format!("uniform {gl_ty} {field_name}{arr_max};")
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

fn vec_of(ty: &Type) -> Option<String> {
    let Some(tp) = as_type_path(ty) else {
        return None;
    };
    let Some(last) = &tp.path.segments.last() else {
        return None;
    };
    if last.ident != "Vec" {
        return None;
    }
    // Handle<...>
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    // Look for a type argument whose last path segment ident is "Image"
    let Some(arg) = args.args.iter().next() else {
        return None;
    };

    let Some(seg) = (match arg {
        GenericArgument::Type(Type::Path(inner_tp)) => inner_tp.path.segments.last(),
        _ => return None,
    }) else {
        return None;
    };

    Some(seg.ident.to_string())
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

fn parse_attr_str(attrs: &[Attribute], ident: &str) -> Option<LitStr> {
    for attr in attrs {
        if attr.path().is_ident(ident) {
            let lit: LitStr = attr.parse_args().expect("{ident} expects a string literal");
            return Some(lit);
        }
    }
    None
}

fn has_attr(attrs: &[Attribute], ident: &str) -> bool {
    for attr in attrs {
        if attr.path().is_ident(ident) {
            return true;
        }
    }
    return false;
}

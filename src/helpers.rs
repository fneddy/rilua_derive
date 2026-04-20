use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, Ident, Lit, MetaNameValue, Type};

pub fn is_method_static(method: &syn::ImplItemFn) -> bool {
    !method
        .sig
        .inputs
        .iter()
        .any(|arg| matches!(arg, FnArg::Receiver(_)))
}

pub fn is_method_mut(method: &syn::ImplItemFn) -> bool {
    method.sig.inputs.iter().any(|arg| {
        if let FnArg::Receiver(r) = arg {
            r.mutability.is_some()
        } else {
            false
        }
    })
}

pub fn get_param_types(method: &syn::ImplItemFn) -> Vec<&Type> {
    method
        .sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pat_type) = arg {
                Some(&*pat_type.ty)
            } else {
                None
            }
        })
        .collect()
}

pub fn generate_param_extracts(param_types: &[&Type], is_static: bool) -> Vec<TokenStream> {
    param_types
        .iter()
        .enumerate()
        .map(|(i, ty)| {
            let idx = if is_static { i } else { i + 1 };
            let arg_name = Ident::new(&format!("arg{}", i), proc_macro2::Span::call_site());
            quote! {
                let #arg_name: #ty = {
                    let val = state.stack_get(state.base + #idx);
                    <#ty as rilua::conversion::FromLua>::from_lua(val, &*state)?
                };
            }
        })
        .collect()
}

pub fn generate_static_wrapper(method_name: &Ident, arg_names: &[Ident]) -> TokenStream {
    quote! {
        let instance = Self::#method_name(#(#arg_names),*);
        let type_name = Self::__lua_type_name();
        let ud = state.create_typed_userdata(instance, type_name)?;
        let lua_val = ud.into_lua(state)?;
        state.push(lua_val);
        Ok(1)
    }
}

pub fn generate_userdata_borrow_error() -> TokenStream {
    quote! {
        return Err(rilua::error::LuaError::Runtime(rilua::error::RuntimeError {
            message: "type mismatch".to_string(),
            level: 0,
            traceback: vec![],
        }))
    }
}

pub fn generate_instance_wrapper(
    method_name: &Ident,
    arg_names: &[Ident],
    is_mut: bool,
    has_return: bool,
) -> TokenStream {
    let borrow_method = if is_mut {
        quote! { borrow_mut }
    } else {
        quote! { borrow }
    };

    let mutability = if is_mut {
        quote! { mut }
    } else {
        quote! {}
    };

    let error_handler = generate_userdata_borrow_error();

    let call_and_return = if has_return {
        quote! {
            let result = {
                let #mutability data = match ud.#borrow_method::<Self>(state) {
                    Some(d) => d,
                    None => #error_handler
                };
                data.#method_name(#(#arg_names),*)
            };
            let lua_val = result.into_lua(state)?;
            state.push(lua_val);
            Ok(1)
        }
    } else {
        quote! {
            {
                let #mutability data = match ud.#borrow_method::<Self>(state) {
                    Some(d) => d,
                    None => #error_handler
                };
                data.#method_name(#(#arg_names),*);
            }
            Ok(0)
        }
    };

    quote! {
        let val = state.stack_get(state.base);
        let ud = <rilua::handles::AnyUserData as rilua::conversion::FromLua>::from_lua(val, &*state)?;
        #call_and_return
    }
}

pub fn generate_lua_function_wrapper(method_name: &Ident, is_mut: bool) -> TokenStream {
    let error_handler = generate_userdata_borrow_error();

    if is_mut {
        quote! {
            let val = state.stack_get(state.base);
            let ud = <rilua::handles::AnyUserData as rilua::conversion::FromLua>::from_lua(val, &*state)?;
            let data_ptr = {
                let data = match ud.borrow_mut::<Self>(state) {
                    Some(d) => d,
                    None => #error_handler
                };
                data as *mut Self
            };
            unsafe { (*data_ptr).#method_name(state) }
        }
    } else {
        quote! {
            let val = state.stack_get(state.base);
            let ud = <rilua::handles::AnyUserData as rilua::conversion::FromLua>::from_lua(val, &*state)?;
            let data_ptr = {
                let data = match ud.borrow::<Self>(state) {
                    Some(d) => d,
                    None => #error_handler
                };
                data as *const Self
            };
            unsafe { (*data_ptr).#method_name(state) }
        }
    }
}

pub fn get_lua_name(attrs: &[syn::Attribute], default: &Ident) -> String {
    for attr in attrs {
        if (attr.path().is_ident("lua_callable") || attr.path().is_ident("lua_function"))
            && let Ok(meta_list) = attr.meta.require_list()
            && let Ok(lit) = syn::parse2::<syn::LitStr>(meta_list.tokens.clone())
        {
            return lit.value();
        }
    }
    default.to_string()
}

pub fn get_field_lua_name(attrs: &[syn::Attribute], default: &Ident) -> String {
    for attr in attrs {
        if !attr.path().is_ident("lua") {
            continue;
        }

        if let Ok(meta_list) = attr.meta.require_list()
            && let Ok(nv) = syn::parse2::<MetaNameValue>(meta_list.tokens.clone())
            && nv.path.is_ident("rename")
            && let syn::Expr::Lit(expr_lit) = &nv.value
            && let Lit::Str(lit_str) = &expr_lit.lit
        {
            return lit_str.value();
        }
    }
    default.to_string()
}

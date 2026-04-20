//! Procedural macros for automatic Rust-Lua struct bindings.
//!
//! This crate provides derive macros and attributes that generate boilerplate code
//! for exposing Rust structs to Lua with type-safe method calls. It follows the KISS
//! principle to keep the implementation simple and maintainable.
//!
//! # Examples
//!
//! ```
//! use rilua_derive::{LuaUserData, lua_register, lua_callable, lua_function};
//!
//! #[derive(LuaUserData, Clone)]
//! struct Counter {
//!     count: i32,
//! }
//!
//! #[lua_register]
//! impl Counter {
//!     #[lua_callable]
//!     fn new(initial: i32) -> Self {
//!         Self { count: initial }
//!     }
//!
//!     #[lua_callable("value")]
//!     fn get(&self) -> i32 {
//!         self.count
//!     }
//!
//!     #[lua_function]
//!     fn inc(&mut self, _state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<u32> {
//!         self.count += 1;
//!         Ok(0)
//!     }
//! }
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, FnArg, ImplItem, ItemImpl, ReturnType, parse_macro_input};

/// Derives the `LuaUserData` trait for a struct, enabling it to be used with Lua.
///
/// This macro generates:
/// - A `__lua_type_name()` method that returns the struct's name as a string literal
/// - An `IntoLua` implementation that creates userdata
/// - A `FromLua` implementation that extracts userdata
///
/// The struct must implement `Clone` to support extraction from userdata.
///
/// # Examples
///
/// ```
/// # use rilua_derive::LuaUserData;
/// #[derive(LuaUserData, Clone)]
/// struct MyStruct {
///     value: i32,
/// }
/// ```
#[proc_macro_derive(LuaUserData)]
pub fn derive_lua_userdata(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            pub fn __lua_type_name() -> &'static str {
                stringify!(#name)
            }
        }

        impl #impl_generics rilua::conversion::IntoLua for #name #ty_generics
        #where_clause
        {
            fn into_lua<L: rilua::api::LuaApiMut>(self, lua: &mut L) -> rilua::error::LuaResult<rilua::vm::value::Val> {
                let type_name = Self::__lua_type_name();
                let ud = lua.create_typed_userdata(self, type_name)?;
                Ok(rilua::vm::value::Val::Userdata(ud.gc_ref()))
            }
        }

        impl #impl_generics rilua::conversion::FromLua for #name #ty_generics
        where
            Self: Clone,
            #where_clause
        {
            fn from_lua<L: rilua::api::LuaApi>(val: rilua::vm::value::Val, lua: &L) -> rilua::error::LuaResult<Self> {
                use rilua::conversion::FromLua;

                match val {
                    rilua::vm::value::Val::Userdata(_) => {
                        let ud = rilua::handles::AnyUserData::from_lua(val, lua)?;
                        let state: &rilua::vm::state::LuaState = lua.state();
                        ud.borrow::<Self>(state)
                            .ok_or_else(|| {
                                rilua::error::LuaError::Runtime(rilua::error::RuntimeError {
                                    message: format!("{} userdata expected, got wrong userdata type", Self::__lua_type_name()),
                                    level: 0,
                                    traceback: vec![],
                                })
                            })
                            .map(|data| data.clone())
                    }
                    _ => Err(rilua::error::LuaError::Runtime(rilua::error::RuntimeError {
                        message: format!("{} userdata expected, got {}", Self::__lua_type_name(), val.type_name()),
                        level: 0,
                        traceback: vec![],
                    }))
                }
            }
        }
    };

    TokenStream::from(expanded)
}

/// Marks an impl block for Lua registration code generation.
///
/// This attribute generates a `register()` function that registers all methods marked
/// with `#[lua_callable]` or `#[lua_function]` with the Lua state. Static methods
/// become constructors accessible via the type table (e.g., `Counter.new()`), while
/// instance methods are added to the metatable (e.g., `counter:value()`).
///
/// # Examples
///
/// ```
/// # use rilua_derive::LuaUserData;
/// # use rilua_derive::lua_register;
/// # use rilua_derive::lua_callable;
/// # #[derive(LuaUserData, Clone)]
/// # struct Counter { count: i32}
/// #[lua_register]
/// impl Counter {
///     #[lua_callable]
///     fn new(initial: i32) -> Self {
///         Self { count: initial }
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn lua_register(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);
    let self_ty = &input.self_ty;

    // get type name simple way
    let type_name = quote!(#self_ty).to_string();

    // find methods with lua attributes
    let mut constructor_regs = vec![];
    let mut method_regs = vec![];

    for item in &input.items {
        if let ImplItem::Fn(method) = item {
            let method_name = &method.sig.ident;

            // check for lua_callable
            let has_callable = method
                .attrs
                .iter()
                .any(|a| a.path().is_ident("lua_callable"));
            // check for lua_function
            let has_function = method
                .attrs
                .iter()
                .any(|a| a.path().is_ident("lua_function"));

            if !has_callable && !has_function {
                continue;
            }

            // get lua name from attribute or use method name
            let lua_name = get_lua_name(&method.attrs, method_name);

            // check if static (constructor)
            let is_static = !method
                .sig
                .inputs
                .iter()
                .any(|arg| matches!(arg, FnArg::Receiver(_)));

            let wrapper_name = if has_callable {
                syn::Ident::new(
                    &format!("__lua_{}", method_name),
                    proc_macro2::Span::call_site(),
                )
            } else {
                syn::Ident::new(
                    &format!("__lua_fn_{}", method_name),
                    proc_macro2::Span::call_site(),
                )
            };

            if is_static {
                constructor_regs.push(quote! {
                    state.table_set_function(&type_table, #lua_name, Self::#wrapper_name)?;
                });
            } else {
                method_regs.push(quote! {
                    state.table_set_function(&mt, #lua_name, Self::#wrapper_name)?;
                });
            }
        }
    }

    let expanded = quote! {
        #input

        impl #self_ty {
            pub fn register(state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<()> {
                use rilua::api::LuaApiMut;
                use rilua::vm::value::Val;

                let mt = state.create_userdata_metatable(#type_name)?;
                let type_table = state.create_table();

                #(#constructor_regs)*
                #(#method_regs)*

                let index_key = state.create_string(b"__index");
                state.table_raw_set(&mt, index_key, Val::Table(mt.gc_ref()))?;

                state.set_global(#type_name, Val::Table(type_table.gc_ref()))?;

                Ok(())
            }
        }
    };

    TokenStream::from(expanded)
}

/// Marks a method for automatic Lua wrapper generation with automatic state handling.
///
/// This attribute generates a wrapper function that handles all Lua state management,
/// including parameter extraction, type conversion, and result pushing. The original
/// method signature is preserved.
///
/// For static methods (no `self` parameter), the wrapper creates a constructor that
/// returns a new userdata instance. For instance methods, it extracts the userdata
/// from the Lua stack and calls the method on it.
///
/// # Optional Custom Name
///
/// You can specify a custom Lua name by passing a string literal:
/// `#[lua_callable("custom_name")]`
///
/// # Examples
///
/// ```ignore
///
/// #[lua_callable]
/// fn new(initial: i32) -> Self {
///     Self { count: initial }
/// }
///
/// #[lua_callable("value")]
/// fn get(&self) -> i32 {
///     self.count
/// }
/// ```
#[proc_macro_attribute]
pub fn lua_callable(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let method = parse_macro_input!(item as syn::ImplItemFn);
    let method_name = &method.sig.ident;
    let wrapper_name = syn::Ident::new(
        &format!("__lua_{}", method_name),
        proc_macro2::Span::call_site(),
    );

    // check if static
    let is_static = !method
        .sig
        .inputs
        .iter()
        .any(|arg| matches!(arg, FnArg::Receiver(_)));

    // count params (excluding self)
    let param_count: usize = method
        .sig
        .inputs
        .iter()
        .filter(|arg| !matches!(arg, FnArg::Receiver(_)))
        .count();

    // check if has return
    let has_return = !matches!(method.sig.output, ReturnType::Default);

    // get param types
    let param_types: Vec<_> = method
        .sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pat_type) = arg {
                Some(&pat_type.ty)
            } else {
                None
            }
        })
        .collect();

    // generate param extraction
    let param_extracts: Vec<_> = param_types
        .iter()
        .enumerate()
        .map(|(i, ty)| {
            let idx = if is_static { i } else { i + 1 };
            let arg_name = syn::Ident::new(&format!("arg{}", i), proc_macro2::Span::call_site());
            quote! {
                let #arg_name: #ty = {
                    let val = state.stack_get(state.base + #idx);
                    <#ty as rilua::conversion::FromLua>::from_lua(val, &*state)?
                };
            }
        })
        .collect();

    let arg_names: Vec<_> = (0..param_count)
        .map(|i| syn::Ident::new(&format!("arg{}", i), proc_macro2::Span::call_site()))
        .collect();

    let wrapper_body = if is_static {
        // constructor
        quote! {
            let instance = Self::#method_name(#(#arg_names),*);
            let type_name = Self::__lua_type_name();
            let ud = state.create_typed_userdata(instance, type_name)?;
            let lua_val = ud.into_lua(state)?;
            state.push(lua_val);
            Ok(1)
        }
    } else {
        // check mutability
        let is_mut = method.sig.inputs.iter().any(|arg| {
            if let FnArg::Receiver(r) = arg {
                r.mutability.is_some()
            } else {
                false
            }
        });

        if is_mut {
            if has_return {
                quote! {
                    let val = state.stack_get(state.base);
                    let ud = <rilua::handles::AnyUserData as rilua::conversion::FromLua>::from_lua(val, &*state)?;
                    let result = {
                        let mut data = match ud.borrow_mut::<Self>(state) {
                            Some(d) => d,
                            None => return Err(rilua::error::LuaError::Runtime(rilua::error::RuntimeError {
                                message: "type mismatch".to_string(),
                                level: 0,
                                traceback: vec![],
                            }))
                        };
                        data.#method_name(#(#arg_names),*)
                    };
                    let lua_val = result.into_lua(state)?;
                    state.push(lua_val);
                    Ok(1)
                }
            } else {
                quote! {
                    let val = state.stack_get(state.base);
                    let ud = <rilua::handles::AnyUserData as rilua::conversion::FromLua>::from_lua(val, &*state)?;
                    {
                        let mut data = match ud.borrow_mut::<Self>(state) {
                            Some(d) => d,
                            None => return Err(rilua::error::LuaError::Runtime(rilua::error::RuntimeError {
                                message: "type mismatch".to_string(),
                                level: 0,
                                traceback: vec![],
                            }))
                        };
                        data.#method_name(#(#arg_names),*);
                    }
                    Ok(0)
                }
            }
        } else {
            if has_return {
                quote! {
                    let val = state.stack_get(state.base);
                    let ud = <rilua::handles::AnyUserData as rilua::conversion::FromLua>::from_lua(val, &*state)?;
                    let result = {
                        let data = match ud.borrow::<Self>(state) {
                            Some(d) => d,
                            None => return Err(rilua::error::LuaError::Runtime(rilua::error::RuntimeError {
                                message: "type mismatch".to_string(),
                                level: 0,
                                traceback: vec![],
                            }))
                        };
                        data.#method_name(#(#arg_names),*)
                    };
                    let lua_val = result.into_lua(state)?;
                    state.push(lua_val);
                    Ok(1)
                }
            } else {
                quote! {
                    let val = state.stack_get(state.base);
                    let ud = <rilua::handles::AnyUserData as rilua::conversion::FromLua>::from_lua(val, &*state)?;
                    {
                        let data = match ud.borrow::<Self>(state) {
                            Some(d) => d,
                            None => return Err(rilua::error::LuaError::Runtime(rilua::error::RuntimeError {
                                message: "type mismatch".to_string(),
                                level: 0,
                                traceback: vec![],
                            }))
                        };
                        data.#method_name(#(#arg_names),*);
                    }
                    Ok(0)
                }
            }
        }
    };

    let expanded = quote! {
        #[allow(non_snake_case)]
        fn #wrapper_name(state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<u32> {
            use rilua::conversion::{FromLua, IntoLua};
            use rilua::api::LuaApiMut;

            #(#param_extracts)*
            #wrapper_body
        }

        #method
    };

    TokenStream::from(expanded)
}

/// Marks a method for Lua registration where you handle the Lua state manually.
///
/// Unlike `#[lua_callable]`, this attribute does not generate automatic parameter
/// extraction or result handling. The method must have the signature:
/// - `fn(state: &mut LuaState) -> LuaResult<u32>` for static methods
/// - `fn(&self, &mut LuaState) -> LuaResult<u32>` for immutable methods
/// - `fn(&mut self, &mut LuaState) -> LuaResult<u32>` for mutable methods
///
/// This is useful when you need fine-grained control over Lua stack operations
/// or want to implement custom behavior that doesn't fit the automatic wrapper pattern.
///
/// # Optional Custom Name
///
/// You can specify a custom Lua name by passing a string literal:
/// `#[lua_function("custom_name")]`
///
/// # Examples
///
/// ```ignore
/// #[lua_function("step")]
/// fn inc(&mut self, state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<u32> {
///     self.count += 1;
///     Ok(0)
/// }
///
/// #[lua_function]
/// fn helper(state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<u32> {
///     // static function - no self
///     Ok(0)
/// }
/// ```
#[proc_macro_attribute]
pub fn lua_function(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let method = parse_macro_input!(item as syn::ImplItemFn);
    let method_name = &method.sig.ident;
    let wrapper_name = syn::Ident::new(
        &format!("__lua_fn_{}", method_name),
        proc_macro2::Span::call_site(),
    );

    // check if static (no self)
    let has_self = method
        .sig
        .inputs
        .iter()
        .any(|arg| matches!(arg, FnArg::Receiver(_)));

    let wrapper_body = if !has_self {
        // static function - just call directly
        quote! {
            Self::#method_name(state)
        }
    } else {
        // instance method - check mutability
        let is_mut = method.sig.inputs.iter().any(|arg| {
            if let FnArg::Receiver(r) = arg {
                r.mutability.is_some()
            } else {
                false
            }
        });

        if is_mut {
            quote! {
                let val = state.stack_get(state.base);
                let ud = <rilua::handles::AnyUserData as rilua::conversion::FromLua>::from_lua(val, &*state)?;
                let data_ptr = {
                    let data = match ud.borrow_mut::<Self>(state) {
                        Some(d) => d,
                        None => return Err(rilua::error::LuaError::Runtime(rilua::error::RuntimeError {
                            message: "type mismatch".to_string(),
                            level: 0,
                            traceback: vec![],
                        }))
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
                        None => return Err(rilua::error::LuaError::Runtime(rilua::error::RuntimeError {
                            message: "type mismatch".to_string(),
                            level: 0,
                            traceback: vec![],
                        }))
                    };
                    data as *const Self
                };
                unsafe { (*data_ptr).#method_name(state) }
            }
        }
    };

    let expanded = quote! {
        #[allow(non_snake_case)]
        fn #wrapper_name(state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<u32> {
            use rilua::conversion::FromLua;

            #wrapper_body
        }

        #method
    };

    TokenStream::from(expanded)
}

/// Extracts the Lua name from method attributes.
///
/// Checks for `#[lua_callable("name")]` or `#[lua_function("name")]` attributes
/// and returns the custom name if present, otherwise returns the method's identifier.
fn get_lua_name(attrs: &[syn::Attribute], default: &syn::Ident) -> String {
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

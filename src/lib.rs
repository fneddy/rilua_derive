//! Procedural macros for automatic Rust-Lua struct bindings.
//!
//! This crate provides derive macros that generate the boilerplate code needed
//! to expose Rust structs to Lua with type-safe method calls.
//!
//! # Examples
//!
//! ```
//! use rilua::{Lua, LuaApiMut};
//! use rilua_derive::{LuaUserData, lua_methods};
//!
//! #[derive(LuaUserData)]
//! struct Counter {
//!     count: i32,
//! }
//!
//! #[lua_methods]
//! impl Counter {
//!     #[lua(constructor)]
//!     fn new(initial: i32) -> Self {
//!         Self { count: initial }
//!     }
//!     
//!     #[lua]
//!     fn get(&self) -> i32 {
//!         self.count
//!     }
//!     
//!     #[lua]
//!     fn increment(&mut self) {
//!         self.count += 1;
//!     }
//! }
//!
//! let mut lua = Lua::new().unwrap();
//! Counter::register(lua.state_mut()).unwrap();
//!
//! lua.exec(r#"
//!     local c = Counter(0)
//!     c:increment()
//!     print(c:get())  -- prints: 1
//! "#).unwrap();
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, FnArg, ImplItem, ItemImpl, ReturnType};

/// Derives basic userdata support for a struct.
///
/// This macro generates helper functions required by `#[lua_methods]`.
/// Must be used in conjunction with `#[lua_methods]` on the impl block.
///
/// # Example
///
/// ```
/// use rilua_derive::LuaUserData;
///
/// #[derive(LuaUserData)]
/// struct MyStruct {
///     value: i32,
/// }
/// ```
#[proc_macro_derive(LuaUserData, attributes(lua))]
pub fn derive_lua_userdata(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let expanded = quote! {
        impl #name {
            pub fn __lua_type_name() -> &'static str {
                stringify!(#name)
            }
        }
    };

    TokenStream::from(expanded)
}

/// Generates Lua method wrappers and registration code for an impl block.
///
/// This attribute macro processes methods marked with `#[lua]` or `#[lua(constructor)]`
/// and generates:
/// - Wrapper functions that handle argument extraction and type conversion
/// - A `register(state)` function that sets up the metatable and registers methods
///
/// # Attributes
///
/// - `#[lua]` - Exposes a method to Lua
/// - `#[lua(constructor)]` - Marks a method as a constructor (registered as global function)
///
/// # Generated Code
///
/// The macro generates a `register()` function that must be called before using
/// the type in Lua:
///
/// ```ignore
/// Counter::register(lua.state_mut()).unwrap();
/// ```
///
/// # Example
///
/// ```
/// use rilua_derive::{LuaUserData, lua_methods};
///
/// #[derive(LuaUserData)]
/// struct Point {
///     x: f64,
///     y: f64,
/// }
///
/// #[lua_methods]
/// impl Point {
///     #[lua(constructor)]
///     fn new(x: f64, y: f64) -> Self {
///         Self { x, y }
///     }
///     
///     #[lua]
///     fn distance(&self) -> f64 {
///         (self.x * self.x + self.y * self.y).sqrt()
///     }
///     
///     #[lua]
///     fn move_to(&mut self, x: f64, y: f64) {
///         self.x = x;
///         self.y = y;
///     }
/// }
/// ```
///
/// In Lua:
///
/// ```lua
/// local p = Point(3.0, 4.0)
/// print(p:distance())  -- 5.0
/// p:move_to(5.0, 12.0)
/// print(p:distance())  -- 13.0
/// ```
#[proc_macro_attribute]
pub fn lua_methods(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);
    let self_ty = input.self_ty.clone();

    let mut wrappers = Vec::new();
    let mut method_registrations = Vec::new();
    let mut constructor_registration = None;
    let mut modified_items = input.items.clone();

    for (idx, item) in input.items.iter().enumerate() {
        if let ImplItem::Fn(method) = item {
            let has_lua_attr = method.attrs.iter().any(|attr| attr.path().is_ident("lua"));

            if !has_lua_attr {
                continue;
            }

            if let ImplItem::Fn(ref mut modified_method) = modified_items[idx] {
                modified_method
                    .attrs
                    .retain(|attr| !attr.path().is_ident("lua"));
            }

            let method_name = &method.sig.ident;
            let wrapper_name =
                syn::Ident::new(&format!("__lua_{}", method_name), method_name.span());

            let is_constructor = method.attrs.iter().any(|attr| {
                if attr.path().is_ident("lua") {
                    if let Ok(meta) = attr.parse_args::<syn::Ident>() {
                        return meta == "constructor";
                    }
                }
                false
            });

            let is_mut = method
                .sig
                .inputs
                .iter()
                .any(|arg| matches!(arg, FnArg::Receiver(r) if r.mutability.is_some()));

            let args: Vec<_> = method
                .sig
                .inputs
                .iter()
                .filter_map(|arg| {
                    if let FnArg::Typed(pat_type) = arg {
                        Some(pat_type.ty.as_ref())
                    } else {
                        None
                    }
                })
                .collect();

            let arg_extracts: Vec<_> = args
                .iter()
                .enumerate()
                .map(|(i, ty)| {
                    let idx = if is_constructor { i } else { i + 1 };
                    let arg_name = syn::Ident::new(&format!("arg{}", i), method_name.span());
                    quote! {
                        let #arg_name: #ty = {
                            let val = state.stack_get(state.base + #idx);
                            <#ty as rilua::conversion::FromLua>::from_lua(val, &*state)?
                        };
                    }
                })
                .collect();

            let arg_names: Vec<_> = (0..args.len())
                .map(|i| syn::Ident::new(&format!("arg{}", i), method_name.span()))
                .collect();

            let has_return = !matches!(method.sig.output, ReturnType::Default);

            let borrow_call = if is_mut {
                quote! { borrow_mut::<#self_ty> }
            } else {
                quote! { borrow::<#self_ty> }
            };

            let wrapper = if is_constructor {
                quote! {
                    fn #wrapper_name(state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<u32> {
                        use rilua::api::LuaApiMut;
                        use rilua::conversion::IntoLua;
                        #(#arg_extracts)*
                        let instance = #self_ty::#method_name(#(#arg_names),*);
                        let ud = state.create_typed_userdata(instance, stringify!(#self_ty))?;
                        let lua_val = ud.into_lua(state)?;
                        state.push(lua_val);
                        Ok(1)
                    }
                }
            } else if has_return {
                quote! {
                    fn #wrapper_name(state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<u32> {
                        use rilua::conversion::{FromLua, IntoLua};
                        let val = state.stack_get(state.base);
                        let ud = <rilua::handles::AnyUserData as FromLua>::from_lua(val, &*state)?;
                        #(#arg_extracts)*
                        let result = {
                            let data = match ud.#borrow_call(state) {
                                Some(d) => d,
                                None => return Err(rilua::error::LuaError::Runtime(rilua::error::RuntimeError {
                                    message: concat!(stringify!(#self_ty), " expected").to_string(),
                                    level: 0,
                                    traceback: vec![],
                                })),
                            };
                            data.#method_name(#(#arg_names),*)
                        };
                        let lua_val = result.into_lua(state)?;
                        state.push(lua_val);
                        Ok(1)
                    }
                }
            } else {
                quote! {
                    fn #wrapper_name(state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<u32> {
                        use rilua::conversion::{FromLua, IntoLua};
                        let val = state.stack_get(state.base);
                        let ud = <rilua::handles::AnyUserData as FromLua>::from_lua(val, &*state)?;
                        #(#arg_extracts)*
                        {
                            let data = match ud.#borrow_call(state) {
                                Some(d) => d,
                                None => return Err(rilua::error::LuaError::Runtime(rilua::error::RuntimeError {
                                    message: concat!(stringify!(#self_ty), " expected").to_string(),
                                    level: 0,
                                    traceback: vec![],
                                })),
                            };
                            data.#method_name(#(#arg_names),*);
                        }
                        Ok(0)
                    }
                }
            };

            wrappers.push(wrapper);

            if is_constructor {
                constructor_registration = Some(quote! {
                    state.register_function(stringify!(#self_ty), Self::#wrapper_name)?;
                });
            } else {
                let method_name_str = method_name.to_string();
                method_registrations.push(quote! {
                    state.table_set_function(&mt, #method_name_str, Self::#wrapper_name)?;
                });
            }
        }
    }

    let register_fn = quote! {
        pub fn register(state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<()> {
            use rilua::api::LuaApiMut;
            use rilua::vm::value::Val;

            let mt = state.create_userdata_metatable(stringify!(#self_ty))?;

            #(#method_registrations)*

            let index_key = state.create_string(b"__index");
            state.table_raw_set(&mt, index_key, Val::Table(mt.gc_ref()))?;

            #constructor_registration

            Ok(())
        }
    };

    let modified_input = ItemImpl {
        items: modified_items,
        ..input
    };

    let expanded = quote! {
        #modified_input

        impl #self_ty {
            #(#wrappers)*

            #register_fn
        }
    };

    TokenStream::from(expanded)
}

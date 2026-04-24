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

mod helpers;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, ImplItem, ItemImpl, ReturnType, parse_macro_input};

use helpers::*;

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
/// use rilua::{Lua, LuaApiMut};
/// use rilua::conversion::{IntoLua, FromLua};
/// use rilua_derive::LuaUserData;
///
/// #[derive(LuaUserData, Clone)]
/// struct Point { x: f64, y: f64 }
///
/// let mut lua = Lua::new().unwrap();
/// let p = Point { x: 1.0, y: 2.0 };
/// let val = p.into_lua(lua.state_mut()).unwrap();
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

fn generate_newindex_error_handler() -> proc_macro2::TokenStream {
    quote! {
        let newindex_closure = rilua::vm::closure::Closure::Rust(
            rilua::vm::closure::RustClosure::new(
                |state: &mut rilua::vm::state::LuaState| -> rilua::error::LuaResult<u32> {
                    use rilua::conversion::FromLua;
                    
                    let key_val = state.stack_get(state.base + 1);
                    let key = String::from_lua(key_val, state).unwrap_or_else(|_| "<unknown>".to_string());
                    
                    Err(rilua::error::RuntimeError::new(
                        format!("Cannot use property syntax. Use getter :{}() and setter :set_{}() methods instead", key, key)
                    ).into())
                },
                "__newindex_error"
            )
        );
        let newindex_ref = state.gc.alloc_closure(newindex_closure);
        
        let newindex_key = state.create_string(b"__newindex");
        state.table_raw_set(&mt, newindex_key, rilua::vm::value::Val::Function(newindex_ref))?;
    }
}

struct MethodRegistrations {
    constructors: Vec<proc_macro2::TokenStream>,
    methods: Vec<proc_macro2::TokenStream>,
    setters: Vec<proc_macro2::TokenStream>,
}

fn process_impl_methods(input: &ItemImpl) -> MethodRegistrations {
    let mut constructor_regs = vec![];
    let mut method_regs = vec![];
    let mut setter_regs = vec![];

    for item in &input.items {
        if let ImplItem::Fn(method) = item {
            let method_name = &method.sig.ident;

            let has_callable = method.attrs.iter().any(|a| a.path().is_ident("lua_callable"));
            let has_function = method.attrs.iter().any(|a| a.path().is_ident("lua_function"));

            if !has_callable && !has_function {
                continue;
            }

            let lua_name = get_lua_name(&method.attrs, method_name);
            let is_static = is_method_static(method);
            let wrapper_name = if has_callable {
                syn::Ident::new(&format!("__lua_{}", method_name), proc_macro2::Span::call_site())
            } else {
                syn::Ident::new(&format!("__lua_fn_{}", method_name), proc_macro2::Span::call_site())
            };

            let method_name_str = method_name.to_string();
            let is_setter = method_name_str.starts_with("set_") && has_callable && !is_static;

            if is_static {
                constructor_regs.push(quote! {
                    state.table_set_function(&type_table, #lua_name, Self::#wrapper_name)?;
                });
            } else if is_setter {
                let property_name = &method_name_str[4..];
                setter_regs.push(quote! {
                    state.table_set_function(&newindex_table, #property_name, Self::#wrapper_name)?;
                });
                method_regs.push(quote! {
                    state.table_set_function(&index_table, #lua_name, Self::#wrapper_name)?;
                });
            } else {
                method_regs.push(quote! {
                    state.table_set_function(&index_table, #lua_name, Self::#wrapper_name)?;
                });
            }
        }
    }

    MethodRegistrations {
        constructors: constructor_regs,
        methods: method_regs,
        setters: setter_regs,
    }
}

/// Marks an impl block for Lua registration code generation.
///
/// This attribute generates a `register()` function that registers all methods marked
/// with `#[lua_callable]` or `#[lua_function]` with the Lua state. Static methods
/// become constructors accessible via the type table (e.g., `Counter.new()`), while
/// instance methods are added to the metatable (e.g., `counter:value()`).
///
/// Methods starting with `set_*` are detected as setters and will trigger helpful
/// error messages if property-style assignment is attempted.
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
    let type_name = quote!(#self_ty).to_string();

    let regs = process_impl_methods(&input);

    let newindex_setup = if regs.setters.is_empty() {
        quote! {}
    } else {
        generate_newindex_error_handler()
    };

    let constructor_regs = &regs.constructors;
    let method_regs = &regs.methods;

    let expanded = quote! {
        #input

        impl #self_ty {
            pub fn register(state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<()> {
                use rilua::api::LuaApiMut;
                use rilua::vm::value::Val;

                let mt = state.create_userdata_metatable(#type_name)?;
                let type_table = state.create_table();

                #(#constructor_regs)*
                
                let index_table = state.create_table();
                #(#method_regs)*

                let index_key = state.create_string(b"__index");
                state.table_raw_set(&mt, index_key, Val::Table(index_table.gc_ref()))?;

                #newindex_setup

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
/// ```
/// use rilua_derive::{LuaUserData, lua_register, lua_callable};
///
/// #[derive(LuaUserData, Clone)]
/// struct Counter { count: i32 }
///
/// #[lua_register]
/// impl Counter {
///     #[lua_callable]
///     fn new(val: i32) -> Self { Self { count: val } }
///
///     #[lua_callable("value")]
///     fn get(&self) -> i32 { self.count }
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

    let is_static = is_method_static(&method);
    let param_types = get_param_types(&method);
    let param_extracts = generate_param_extracts(&param_types, is_static);
    let arg_names: Vec<_> = (0..param_types.len())
        .map(|i| syn::Ident::new(&format!("arg{}", i), proc_macro2::Span::call_site()))
        .collect();

    let wrapper_body = if is_static {
        generate_static_wrapper(method_name, &arg_names)
    } else {
        let is_mut = is_method_mut(&method);
        let has_return = !matches!(method.sig.output, ReturnType::Default);
        generate_instance_wrapper(method_name, &arg_names, is_mut, has_return)
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
/// ```
/// use rilua_derive::{LuaUserData, lua_register, lua_function};
/// use rilua::vm::state::LuaState;
/// use rilua::error::LuaResult;
///
/// #[derive(LuaUserData, Clone)]
/// struct Counter { count: i32 }
///
/// #[lua_register]
/// impl Counter {
///     #[lua_function("step")]
///     fn inc(&mut self, _state: &mut LuaState) -> LuaResult<u32> {
///         self.count += 1;
///         Ok(0)
///     }
///
///     #[lua_function]
///     fn helper(_state: &mut LuaState) -> LuaResult<u32> {
///         Ok(0)
///     }
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

    let is_static = is_method_static(&method);
    let wrapper_body = if is_static {
        quote! { Self::#method_name(state) }
    } else {
        let is_mut = is_method_mut(&method);
        generate_lua_function_wrapper(method_name, is_mut)
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

/// Derives the `IntoLua` trait for a struct, converting it to a Lua table.
///
/// This macro generates an `IntoLua` implementation that creates a Lua table
/// with each field as a table entry. Field names are used as Lua keys.
///
/// # Field Attributes
///
/// - `#[lua(rename = "name")]` - Use custom Lua key name
///
/// # Examples
///
/// ```
/// use rilua::{Lua, LuaApiMut};
/// use rilua::conversion::IntoLua;
/// use rilua_derive::IntoLua;
///
/// #[derive(IntoLua)]
/// struct Position { x: f64, y: f64 }
///
/// let mut lua = Lua::new().unwrap();
/// let pos = Position { x: 1.0, y: 2.0 };
/// let val = pos.into_lua(lua.state_mut()).unwrap();
/// ```
#[proc_macro_derive(IntoLua, attributes(lua))]
pub fn derive_into_lua(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let Data::Struct(data_struct) = &input.data else {
        return syn::Error::new_spanned(&input, "IntoLua only supports structs")
            .to_compile_error()
            .into();
    };

    let Fields::Named(fields) = &data_struct.fields else {
        return syn::Error::new_spanned(&input, "IntoLua only supports named fields")
            .to_compile_error()
            .into();
    };

    let field_conversions: Vec<_> = fields
        .named
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            let lua_key = get_field_lua_name(&field.attrs, field_name);

            quote! {
                {
                    let key_str = #lua_key;
                    let key_val = key_str.into_lua(lua)?;
                    let value = self.#field_name.into_lua(lua)?;
                    lua.table_raw_set(&table, key_val, value)?;
                }
            }
        })
        .collect();

    let expanded = quote! {
        impl #impl_generics rilua::conversion::IntoLua for #name #ty_generics
        #where_clause
        {
            fn into_lua<L: rilua::api::LuaApiMut>(self, lua: &mut L) -> rilua::error::LuaResult<rilua::vm::value::Val> {
                use rilua::api::LuaApiMut;
                use rilua::conversion::IntoLua;

                let table = lua.create_table();
                #(#field_conversions)*
                Ok(rilua::vm::value::Val::Table(table.gc_ref()))
            }
        }
    };

    TokenStream::from(expanded)
}

/// Derives the `FromLua` trait for a struct, converting from a Lua table.
///
/// This macro generates a `FromLua` implementation that extracts field values
/// from a Lua table. Field names are used as Lua keys.
///
/// # Field Attributes
///
/// - `#[lua(rename = "name")]` - Use custom Lua key name
///
/// # Examples
///
/// ```
/// use rilua::{Lua, LuaApi, LuaApiMut};
/// use rilua::conversion::FromLua;
/// use rilua::vm::value::Val;
/// use rilua_derive::FromLua;
///
/// #[derive(FromLua)]
/// struct Position { x: f64, y: f64 }
///
/// let mut lua = Lua::new().unwrap();
/// lua.exec("pos = { x = 3.0, y = 4.0 }").unwrap();
/// let val = lua.global::<Val>("pos").unwrap();
/// let pos = Position::from_lua(val, lua.state()).unwrap();
/// ```
#[proc_macro_derive(FromLua, attributes(lua))]
pub fn derive_from_lua(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let Data::Struct(data_struct) = &input.data else {
        return syn::Error::new_spanned(&input, "FromLua only supports structs")
            .to_compile_error()
            .into();
    };

    let Fields::Named(fields) = &data_struct.fields else {
        return syn::Error::new_spanned(&input, "FromLua only supports named fields")
            .to_compile_error()
            .into();
    };

    let field_extractions: Vec<_> = fields
        .named
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            let field_type = &field.ty;
            let lua_key = get_field_lua_name(&field.attrs, field_name);

            quote! {
                let #field_name: #field_type = {
                    let key_bytes = #lua_key.as_bytes();
                    let key_ref = unsafe {
                        let state_ptr = lua.state() as *const rilua::vm::state::LuaState as *mut rilua::vm::state::LuaState;
                        (*state_ptr).gc.intern_string(key_bytes)
                    };
                    let key_val = rilua::vm::value::Val::Str(key_ref);
                    let val = table.raw_get(lua.state(), key_val)?;
                    <#field_type as rilua::conversion::FromLua>::from_lua(val, lua)?
                };
            }
        })
        .collect();

    let field_names: Vec<_> = fields
        .named
        .iter()
        .map(|f| f.ident.as_ref().unwrap())
        .collect();

    let expanded = quote! {
        impl #impl_generics rilua::conversion::FromLua for #name #ty_generics
        #where_clause
        {
            fn from_lua<L: rilua::api::LuaApi>(val: rilua::vm::value::Val, lua: &L) -> rilua::error::LuaResult<Self> {
                use rilua::api::{LuaApi, LuaApiMut};
                use rilua::conversion::FromLua;

                match val {
                    rilua::vm::value::Val::Table(_) => {
                        let table = rilua::handles::Table::from_lua(val, lua)?;
                        #(#field_extractions)*
                        Ok(Self {
                            #(#field_names),*
                        })
                    }
                    _ => Err(rilua::error::LuaError::Runtime(rilua::error::RuntimeError {
                        message: format!("{} table expected, got {}", stringify!(#name), val.type_name()),
                        level: 0,
                        traceback: vec![],
                    }))
                }
            }
        }
    };

    TokenStream::from(expanded)
}

# rilua_derive

Procedural macros for automatic Rust-Lua struct bindings with minimal complexity.

## Overview

This crate provides derive macros and attributes that generate boilerplate code for exposing Rust structs to Lua with type-safe method calls. It's a simplified reimplementation of `rilua_derive2` with reduced complexity and improved maintainability.

## Usage

```rust
use rilua_derive::{LuaUserData, lua_callable, lua_function, lua_register};

#[derive(LuaUserData)]
struct Counter {
    count: i32,
}

#[lua_register]
impl Counter {
    // Constructor - static method
    #[lua_callable]
    fn new(initial: i32) -> Self {
        Self { count: initial }
    }

    // Constructor with custom Lua name
    #[lua_callable("zero")]
    fn default() -> Self {
        Self { count: 0 }
    }

    // Mutable method
    #[lua_callable]
    fn modify(&mut self, new_value: i32) {
        self.count = new_value;
    }

    // Immutable method with custom name
    #[lua_callable("value")]
    fn get(&self) -> i32 {
        self.count
    }

    // Raw Lua function - manual state handling
    #[lua_function("step")]
    fn inc(&mut self, _state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<u32> {
        self.count += 1;
        Ok(0)
    }
}

// Register with Lua
let mut lua = Lua::new().unwrap();
Counter::register(lua.state_mut()).unwrap();

// Use in Lua
lua.exec(r#"
    local c = Counter.new(5)
    print(c:value())  -- 5
    c:modify(10)
    print(c:value())  -- 10
    c:step()
    print(c:value())  -- 11
"#).unwrap();
```

## Attributes

### `#[derive(LuaUserData)]`
Enables a struct to be used with Lua. Required on the struct definition.

### `#[lua_register]`
Generates a `register()` function for the impl block. Call this to register the type with Lua.

### `#[lua_callable]` or `#[lua_callable("custom_name")]`
Generates a wrapper function for the method, handling all Lua state management automatically.
- Static methods become constructors (e.g., `Counter.new()`)
- Instance methods become methods (e.g., `c:value()`)
- Optional custom Lua name can be specified

### `#[lua_function]` or `#[lua_function("custom_name")]`
Marks a method for registration where you handle the Lua state manually.
Method signature must be: `fn(&self, &mut LuaState) -> LuaResult<u32>` or `fn(&mut self, &mut LuaState) -> LuaResult<u32>`

## Design Philosophy

This crate follows the KISS principle: Keep It Simple, Stupid. By avoiding unnecessary abstractions and keeping the code direct and straightforward, it's easier to understand, maintain, and debug while providing the same functionality as more complex implementations.
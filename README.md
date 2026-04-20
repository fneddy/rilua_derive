# rilua_derive

Procedural macros for automatic Rust-Lua bindings. Simple and practical.

## Features

Two approaches for Rust-Lua integration:

1. **Userdata** - Rust objects with methods callable from Lua
2. **Table conversion** - Serialize Rust structs to/from Lua tables

## Userdata (Objects with Methods)

### Minimal Example

```rust
use rilua::{Lua, LuaApiMut};
use rilua_derive::{LuaUserData, lua_register, lua_callable};

#[derive(LuaUserData, Clone)]
struct Counter { count: i32 }

#[lua_register]
impl Counter {
    #[lua_callable]
    fn new(val: i32) -> Self { Self { count: val } }

    #[lua_callable]
    fn get(&self) -> i32 { self.count }

    #[lua_callable]
    fn increment(&mut self) { self.count += 1; }
}

let mut lua = Lua::new().unwrap();
Counter::register(lua.state_mut()).unwrap();
lua.exec("c = Counter.new(5); c:increment(); print(c:get())").unwrap();
```

### Custom Lua Names

```rust
#[lua_callable("value")]
fn get(&self) -> i32 { self.count }
```

Lua: `c:value()` instead of `c:get()`

### Manual State Control

```rust
use rilua_derive::lua_function;
use rilua::vm::state::LuaState;

#[lua_function]
fn raw(&mut self, state: &mut LuaState) -> rilua::error::LuaResult<u32> {
    // manual stack manipulation
    Ok(0)
}
```

### Static Functions

```rust
#[lua_function]
fn helper(state: &mut LuaState) -> rilua::error::LuaResult<u32> {
    // no self - static function
    Ok(0)
}
```

Lua: `Counter.helper()`

## Table Conversion (Plain Data)

### Minimal Example

```rust
use rilua::{Lua, LuaApi, LuaApiMut};
use rilua::conversion::{IntoLua, FromLua};
use rilua_derive::{IntoLua, FromLua};

#[derive(IntoLua, FromLua)]
struct Position { x: f64, y: f64 }

let mut lua = Lua::new().unwrap();

// Rust to Lua table
let pos = Position { x: 1.0, y: 2.0 };
let val = pos.into_lua(lua.state_mut()).unwrap();
lua.set_global("pos", val).unwrap();

// Lua to Rust
lua.exec("pos2 = { x = 3.0, y = 4.0 }").unwrap();
let val = lua.global("pos2").unwrap();
let pos2 = Position::from_lua(val, lua.state()).unwrap();
```

### Nested Structs

```rust
#[derive(IntoLua, FromLua)]
struct Entity {
    name: String,
    pos: Position,  // nested
}
```

### Optional Fields

```rust
#[derive(IntoLua, FromLua)]
struct Config {
    width: u32,
    debug: Option<bool>,  // nil in Lua if None
}
```

### Custom Field Names

```rust
#[derive(IntoLua, FromLua)]
struct Player {
    #[lua(rename = "maxHP")]
    max_health: i32,
}
```

Lua: `player.maxHP` instead of `player.max_health`

## Userdata vs Tables

**Cannot mix both on same struct!** Pick one:

```rust
// Option 1: Userdata (object with methods)
#[derive(LuaUserData, Clone)]
struct Player { health: i32 }

// Option 2: Table (plain data)
#[derive(IntoLua, FromLua)]
struct SaveData { level: u32 }

// ❌ WRONG - conflict!
// #[derive(LuaUserData, IntoLua, FromLua)]
```

## All Macros

| Macro | Purpose |
|-------|---------|
| `#[derive(LuaUserData)]` | Enable struct as userdata (requires `Clone`) |
| `#[lua_register]` | Generate `register()` function for impl block |
| `#[lua_callable]` | Auto-wrap method with Lua state handling |
| `#[lua_function]` | Register method with manual state control |
| `#[derive(IntoLua)]` | Convert struct to Lua table |
| `#[derive(FromLua)]` | Convert Lua table to struct |

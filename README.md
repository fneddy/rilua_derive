# rilua_derive

Procedural macros for automatic Rust-Lua struct bindings in [rilua](https://github.com/wowemulation-dev/rilua).

## Overview

`rilua_derive` provides `#[derive(LuaUserData)]` and `#[lua_methods]` macros that automatically generate the boilerplate code needed to expose Rust structs to Lua with type-safe method calls.

## Features

- **Zero-boilerplate struct exposure**: Derive macro generates all wrapper code
- **Type-safe method calls**: Automatic conversion using `FromLua` / `IntoLua`
- **Bidirectional access**: Rust and Lua can both access the same struct
- **GC integration**: Structs are managed by Lua's garbage collector
- **Method types supported**:
  - Immutable methods (`&self`)
  - Mutable methods (`&mut self`)
  - Constructors (static methods)
  - Methods with arguments and return values

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
rilua = "0.1"
rilua_derive = "0.1"
```

### Basic Example

```rust
use rilua::{Lua, LuaApiMut};
use rilua_derive::{LuaUserData, lua_methods};

// Derive LuaUserData for your struct
#[derive(LuaUserData)]
struct Counter {
    count: i32,
}

// Mark methods to expose to Lua
#[lua_methods]
impl Counter {
    // Constructor - called as Counter(initial) in Lua
    #[lua(constructor)]
    fn new(initial: i32) -> Self {
        Self { count: initial }
    }
    
    // Immutable method
    #[lua]
    fn get(&self) -> i32 {
        self.count
    }
    
    // Mutable method
    #[lua]
    fn increment(&mut self) {
        self.count += 1;
    }
    
    // Method with arguments
    #[lua]
    fn add(&mut self, amount: i32) {
        self.count += amount;
    }
}

fn main() {
    let mut lua = Lua::new().unwrap();
    
    // Register the type with Lua
    Counter::register(lua.state_mut()).unwrap();
    
    // Create instances from Lua
    lua.exec(r#"
        local counter = Counter(0)
        counter:increment()
        counter:increment()
        counter:add(10)
        print(counter:get())  -- prints: 12
    "#).unwrap();
}
```

### Bidirectional Access

After creating a userdata, both Rust and Lua can access the same struct:

```rust
use rilua::{Lua, LuaApi, LuaApiMut};
use rilua_derive::{LuaUserData, lua_methods};

#[derive(LuaUserData)]
struct Counter {
    count: i32,
}

#[lua_methods]
impl Counter {
    #[lua]
    fn get(&self) -> i32 {
        self.count
    }
    
    #[lua]
    fn increment(&mut self) {
        self.count += 1;
    }
}

fn main() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();
    
    // Create counter and keep the handle
    let counter = Counter { count: 0 };
    let ud = lua.create_typed_userdata(counter, "Counter").unwrap();
    lua.set_global("counter", ud).unwrap();
    
    // Lua modifies it
    lua.exec("counter:increment()").unwrap();
    lua.exec("counter:increment()").unwrap();
    
    // Rust can read the value
    let value = ud.borrow::<Counter>(lua.state()).unwrap().get();
    assert_eq!(value, 2);
    
    // Rust can modify it
    ud.borrow_mut::<Counter>(lua.state_mut()).unwrap().increment();
    
    // Lua sees Rust's changes
    lua.exec("result = counter:get()").unwrap();
    let result: f64 = lua.global("result").unwrap();
    assert_eq!(result, 3.0);
    
    // Rust can access fields directly
    if let Some(c) = ud.borrow_mut::<Counter>(lua.state_mut()) {
        c.count = 100;
    }
    
    // Lua sees the change
    lua.exec("print(counter:get())").unwrap();  // prints: 100
}
```

## Attribute Reference

### `#[derive(LuaUserData)]`

Derives the basic userdata support for a struct. Generates:
- `__lua_type_name()` helper function
- Required by `#[lua_methods]` macro

### `#[lua_methods]`

Applied to an `impl` block to generate Lua method wrappers. Generates:
- Wrapper functions for each `#[lua]` method
- `register(state)` function to set up the metatable
- Automatic argument extraction and type conversion

### Method Attributes

#### `#[lua]`

Exposes a method to Lua. The macro automatically detects:
- Immutable methods (`&self`) - uses `borrow()`
- Mutable methods (`&mut self`) - uses `borrow_mut()`
- Arguments and return values - converted via `FromLua` / `IntoLua`

```rust
#[lua_methods]
impl MyStruct {
    #[lua]
    fn read_only(&self) -> i32 {
        self.value
    }
    
    #[lua]
    fn modify(&mut self, new_value: i32) {
        self.value = new_value;
    }
}
```

#### `#[lua(constructor)]`

Marks a static method as a constructor. Registered as a global function in Lua:

```rust
#[lua_methods]
impl MyStruct {
    #[lua(constructor)]
    fn new(value: i32) -> Self {
        Self { value }
    }
}

// In Lua:
// local obj = MyStruct(42)
```

## Type Conversions

The macros use `FromLua` and `IntoLua` traits for type conversion. Supported types include:

**Primitives:**
- `bool`
- `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`
- `f32`, `f64`
- `String`, `&str`, `Vec<u8>`

**Lua types:**
- `Table`
- `Function`
- `Thread`
- `AnyUserData`

**Containers:**
- `Option<T>`
- `(T1, T2, ...)` tuples (up to 8 elements)

## Generated Code

For this struct:

```rust
#[derive(LuaUserData)]
struct Counter {
    count: i32,
}

#[lua_methods]
impl Counter {
    #[lua]
    fn get(&self) -> i32 {
        self.count
    }
}
```

The macro generates approximately:

```rust
impl Counter {
    pub fn __lua_type_name() -> &'static str {
        "Counter"
    }
}

impl Counter {
    fn get(&self) -> i32 {
        self.count
    }
}

impl Counter {
    fn __lua_get(state: &mut LuaState) -> LuaResult<u32> {
        use rilua::conversion::{FromLua, IntoLua};
        let val = state.stack_get(state.base);
        let ud = AnyUserData::from_lua(val, &*state)?;
        let result = {
            let data = match ud.borrow::<Counter>(state) {
                Some(d) => d,
                None => return Err(LuaError::Runtime(...)),
            };
            data.get()
        };
        let lua_val = result.into_lua(state)?;
        state.push(lua_val);
        Ok(1)
    }
    
    pub fn register(state: &mut LuaState) -> LuaResult<()> {
        use rilua::api::LuaApiMut;
        use rilua::vm::value::Val;
        
        let mt = state.create_userdata_metatable("Counter")?;
        state.table_set_function(&mt, "get", Self::__lua_get)?;
        
        let index_key = state.create_string(b"__index");
        state.table_raw_set(&mt, index_key, Val::Table(mt.gc_ref()))?;
        
        Ok(())
    }
}
```

## Limitations

1. **State required for access**: Rust must pass a `LuaState` reference to `borrow()` or `borrow_mut()` for every access
2. **Runtime type checking**: Type mismatches return `None` rather than compile-time errors
3. **No cross-thread access**: Structs cannot be shared across threads (use `Arc<Mutex<T>>` if needed)
4. **Lifetime tied to GC**: Structs can be garbage collected; `borrow()` returns `None` if collected

## Advanced Patterns

### Custom Error Handling

```rust
#[lua_methods]
impl MyStruct {
    #[lua]
    fn divide(&self, divisor: i32) -> i32 {
        if divisor == 0 {
            panic!("division by zero");  // Propagates to Lua as error
        }
        self.value / divisor
    }
}
```

### Optional Returns

```rust
#[lua_methods]
impl MyStruct {
    #[lua]
    fn try_get(&self) -> Option<i32> {
        if self.value > 0 {
            Some(self.value)
        } else {
            None  // Returns nil to Lua
        }
    }
}
```

### Multiple Arguments

```rust
#[lua_methods]
impl Vector {
    #[lua]
    fn set(&mut self, x: f64, y: f64, z: f64) {
        self.x = x;
        self.y = y;
        self.z = z;
    }
}
```

## Examples

See the `tests/integration_test.rs` file for complete working examples including:
- Basic struct exposure
- Bidirectional access patterns
- Constructor usage
- Mutable and immutable methods
- Type conversion

## License

MIT OR Apache-2.0

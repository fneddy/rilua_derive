use rilua::{Lua, LuaApi, LuaApiMut};
use rilua_derive::{lua_methods, LuaUserData};

/// A simple counter that can be used from both Rust and Lua
#[derive(LuaUserData)]
struct Counter {
    count: i32,
}

#[lua_methods]
impl Counter {
    /// Constructor - called as Counter(initial) in Lua
    #[lua(constructor)]
    fn new(initial: i32) -> Self {
        Self { count: initial }
    }

    /// Get the current count (immutable)
    #[lua]
    fn get(&self) -> i32 {
        self.count
    }

    /// Increment by 1 (mutable)
    #[lua]
    fn increment(&mut self) {
        self.count += 1;
    }

    /// Add a specific amount (mutable with argument)
    #[lua]
    fn add(&mut self, amount: i32) {
        self.count += amount;
    }

    /// Reset to zero
    #[lua]
    fn reset(&mut self) {
        self.count = 0;
    }
}

fn main() -> rilua::error::LuaResult<()> {
    let mut lua = Lua::new()?;

    // Register the Counter type
    Counter::register(lua.state_mut())?;

    // Example 1: Create and use from Lua
    println!("Example 1: Lua usage");
    lua.exec(
        r#"
        local counter = Counter(0)
        counter:increment()
        counter:increment()
        counter:add(10)
        print("Count from Lua: " .. counter:get())
    "#,
    )?;

    // Example 2: Create in Rust, use from Lua
    println!("\nExample 2: Created in Rust, used from Lua");
    let counter = Counter { count: 100 };
    let ud = lua.create_typed_userdata(counter, "Counter")?;
    lua.set_global("rust_counter", ud)?;

    lua.exec(
        r#"
        rust_counter:add(23)
        print("Count from Rust object: " .. rust_counter:get())
    "#,
    )?;

    // Example 3: Bidirectional access
    println!("\nExample 3: Bidirectional access");
    let counter = Counter { count: 0 };
    let ud = lua.create_typed_userdata(counter, "Counter")?;
    lua.set_global("shared", ud)?;

    // Lua increments it
    lua.exec("shared:increment(); shared:increment()")?;
    println!(
        "After Lua incremented twice: {}",
        ud.borrow::<Counter>(lua.state()).unwrap().get()
    );

    // Rust adds to it
    ud.borrow_mut::<Counter>(lua.state_mut()).unwrap().add(10);

    // Lua sees Rust's changes
    lua.exec(
        r#"
        print("After Rust added 10: " .. shared:get())
    "#,
    )?;

    Ok(())
}

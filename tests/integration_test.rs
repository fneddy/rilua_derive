use rilua::{Lua, LuaApi, LuaApiMut};
use rilua_derive::{lua_methods, LuaUserData};

#[derive(LuaUserData)]
struct Counter {
    count: i32,
}

#[lua_methods]
impl Counter {
    #[lua(constructor)]
    fn new(initial: i32) -> Self {
        Self { count: initial }
    }

    #[lua]
    fn get(&self) -> i32 {
        self.count
    }

    #[lua]
    fn increment(&mut self) {
        self.count += 1;
    }

    #[lua]
    fn add(&mut self, amount: i32) {
        self.count += amount;
    }
}

#[test]
fn test_userdata_immutable() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    let counter = Counter { count: 42 };
    let ud = lua.create_typed_userdata(counter, "Counter").unwrap();
    lua.set_global("counter", ud).unwrap();

    lua.exec("result = counter:get()").unwrap();

    let result: f64 = lua.global("result").unwrap();
    assert_eq!(result, 42.0);
}

#[test]
fn test_userdata_mutable() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    let counter = Counter { count: 0 };
    let ud = lua.create_typed_userdata(counter, "Counter").unwrap();
    lua.set_global("counter", ud).unwrap();

    lua.exec("counter:increment()").unwrap();
    lua.exec("counter:increment()").unwrap();
    lua.exec("result = counter:get()").unwrap();

    let result: f64 = lua.global("result").unwrap();
    assert_eq!(result, 2.0);
}

#[test]
fn test_userdata_with_args() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    let counter = Counter { count: 10 };
    let ud = lua.create_typed_userdata(counter, "Counter").unwrap();
    lua.set_global("counter", ud).unwrap();

    lua.exec("counter:add(5)").unwrap();
    lua.exec("result = counter:get()").unwrap();

    let result: f64 = lua.global("result").unwrap();
    assert_eq!(result, 15.0);
}

#[test]
fn test_constructor() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    lua.exec(
        r#"
        local c = Counter(100)
        result = c:get()
    "#,
    )
    .unwrap();

    let result: f64 = lua.global("result").unwrap();
    assert_eq!(result, 100.0);
}

// Approach 1: Bidirectional access using AnyUserData.borrow()
#[test]
fn test_approach1_bidirectional_access() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    // Create counter and get handle
    let counter = Counter { count: 0 };
    let ud = lua.create_typed_userdata(counter, "Counter").unwrap();
    lua.set_global("counter", ud).unwrap();

    // Lua increments it twice
    lua.exec("counter:increment()").unwrap();
    lua.exec("counter:increment()").unwrap();

    // Rust can read the value using borrow()
    let value = ud.borrow::<Counter>(lua.state()).unwrap().get();
    assert_eq!(value, 2);

    // Rust can modify using borrow_mut()
    ud.borrow_mut::<Counter>(lua.state_mut()).unwrap().add(10);

    // Lua can see Rust's changes
    lua.exec("result = counter:get()").unwrap();
    let result: f64 = lua.global("result").unwrap();
    assert_eq!(result, 12.0);

    // Rust can directly access fields
    if let Some(c) = ud.borrow::<Counter>(lua.state()) {
        assert_eq!(c.count, 12);
    }

    // Rust can modify fields directly
    if let Some(c) = ud.borrow_mut::<Counter>(lua.state_mut()) {
        c.count = 100;
    }

    // Lua sees the direct field change
    lua.exec("final = counter:get()").unwrap();
    let final_result: f64 = lua.global("final").unwrap();
    assert_eq!(final_result, 100.0);
}

// Test that demonstrates the limitations of Approach 1
#[test]
fn test_approach1_requires_state() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    let counter = Counter { count: 5 };
    let ud = lua.create_typed_userdata(counter, "Counter").unwrap();

    // Cannot access without state - this is the limitation
    // ud.get() // Would not compile - no such method

    // Must use borrow with state reference
    let value = ud.borrow::<Counter>(lua.state()).unwrap().count;
    assert_eq!(value, 5);

    // Multiple borrows are allowed if immutable
    let v1 = ud.borrow::<Counter>(lua.state()).unwrap().count;
    let v2 = ud.borrow::<Counter>(lua.state()).unwrap().count;
    assert_eq!(v1, v2);
}

// Test that shows borrow checking at runtime
#[test]
fn test_approach1_runtime_checking() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    let counter = Counter { count: 0 };
    let ud = lua.create_typed_userdata(counter, "Counter").unwrap();

    // Returns Some when userdata is valid
    assert!(ud.borrow::<Counter>(lua.state()).is_some());

    // Returns None if wrong type requested
    assert!(ud.borrow::<String>(lua.state()).is_none());

    // Can get mutable borrow
    assert!(ud.borrow_mut::<Counter>(lua.state_mut()).is_some());
}

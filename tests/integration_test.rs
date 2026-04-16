use rilua::{Lua, LuaApiMut};
use rilua_derive::{LuaUserData, lua_callable, lua_function, lua_register};

#[derive(LuaUserData)]
struct Counter {
    count: i32,
}

#[lua_register]
impl Counter {
    #[lua_callable]
    fn new(initial: i32) -> Self {
        Self { count: initial }
    }

    #[lua_callable("zero")]
    fn default() -> Self {
        Self { count: 0 }
    }

    #[lua_callable]
    fn modify(&mut self, new_value: i32) {
        self.count = new_value;
    }

    #[lua_callable("value")]
    fn get(&self) -> i32 {
        self.count
    }

    #[lua_function("step")]
    fn inc(&mut self, _state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<u32> {
        self.count += 1;
        Ok(0)
    }

    #[lua_function]
    fn dec(&mut self, _state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<u32> {
        self.count -= 1;
        Ok(0)
    }
}

#[test]
fn test_counter_new() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    lua.exec(
        r#"
        local c = Counter.new(5)
        assert(c:value() == 5, "Counter should start at 5")
    "#,
    )
    .unwrap();
}

#[test]
fn test_counter_zero() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    lua.exec(
        r#"
        local c = Counter.zero()
        assert(c:value() == 0, "Counter should start at 0")
    "#,
    )
    .unwrap();
}

#[test]
fn test_counter_modify() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    lua.exec(
        r#"
        local c = Counter.new(10)
        c:modify(42)
        assert(c:value() == 42, "Counter should be 42 after modify")
    "#,
    )
    .unwrap();
}

#[test]
fn test_counter_inc() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    lua.exec(
        r#"
        local c = Counter.new(0)
        c:step()
        assert(c:value() == 1, "Counter should be 1 after inc")
        c:step()
        assert(c:value() == 2, "Counter should be 2 after second inc")
    "#,
    )
    .unwrap();
}

#[test]
fn test_counter_dec() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    lua.exec(
        r#"
        local c = Counter.new(5)
        c:dec()
        assert(c:value() == 4, "Counter should be 4 after dec")
    "#,
    )
    .unwrap();
}

#[test]
fn test_counter_full_workflow() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    lua.exec(
        r#"
        -- Test constructor
        local c1 = Counter.new(10)
        assert(c1:value() == 10)
        
        -- Test zero constructor
        local c2 = Counter.zero()
        assert(c2:value() == 0)
        
        -- Test modify
        c1:modify(100)
        assert(c1:value() == 100)
        
        -- Test increment
        c2:step()
        c2:step()
        c2:step()
        assert(c2:value() == 3)
        
        -- Test decrement
        c1:dec()
        assert(c1:value() == 99)
        
        -- Test multiple operations
        c2:modify(50)
        c2:step()
        c2:dec()
        assert(c2:value() == 50)
    "#,
    )
    .unwrap();
}

#[test]
fn test_custom_naming() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    lua.exec(
        r#"
        local c = Counter.zero()
        assert(c.value ~= nil, "value method should exist")
        assert(c.get == nil, "get method should not exist (renamed to value)")
        
        c:step()
        assert(c:value() == 1, "step should work (renamed from inc)")
    "#,
    )
    .unwrap();
}

#[test]
fn test_namespaced_constructors() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    lua.exec(
        r#"
        -- Verify constructors are namespaced under Counter
        assert(Counter ~= nil, "Counter table should exist")
        assert(Counter.new ~= nil, "Counter.new should exist")
        assert(Counter.zero ~= nil, "Counter.zero should exist")
        
        -- Verify global namespace is not polluted
        assert(new == nil, "new should not be global")
        assert(zero == nil, "zero should not be global")
        
        -- Verify constructors work
        local c1 = Counter.new(42)
        assert(c1:value() == 42)
        
        local c2 = Counter.zero()
        assert(c2:value() == 0)
    "#,
    )
    .unwrap();
}

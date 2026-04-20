use rilua::{Lua, LuaApi, LuaApiMut};
use rilua_derive::{LuaUserData, lua_callable, lua_function, lua_register};

#[derive(LuaUserData, Clone)]
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

    #[lua_function]
    fn static_helper(state: &mut rilua::vm::state::LuaState) -> rilua::error::LuaResult<u32> {
        state.push(rilua::vm::value::Val::Num(42.0));
        Ok(1)
    }
}

#[test]
fn test_lua_callable_and_function() {
    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    lua.exec(
        r#"
        -- constructors (static lua_callable)
        local c1 = Counter.new(10)
        assert(c1:value() == 10)
        local c2 = Counter.zero()
        assert(c2:value() == 0)
        
        -- mutation (lua_callable)
        c1:modify(100)
        assert(c1:value() == 100)
        
        -- lua_function with custom name
        c2:step()
        c2:step()
        assert(c2:value() == 2)
        
        -- lua_function without custom name
        c1:dec()
        assert(c1:value() == 99)
        
        -- custom naming works
        assert(c2.value ~= nil, "renamed method exists")
        assert(c2.get == nil, "original name not exist")
        
        -- static lua_function
        local result = Counter.static_helper()
        assert(result == 42)
        
        -- namespace not polluted
        assert(new == nil)
        assert(zero == nil)
    "#,
    )
    .unwrap();
}

#[test]
fn test_userdata_conversion() {
    use rilua::conversion::{FromLua, IntoLua};
    use rilua::vm::value::Val;

    let mut lua = Lua::new().unwrap();
    Counter::register(lua.state_mut()).unwrap();

    let original = Counter { count: 123 };
    let val = original.into_lua(lua.state_mut()).unwrap();
    let restored = Counter::from_lua(val, lua.state()).unwrap();
    assert_eq!(restored.count, 123);

    lua.exec("c = Counter.new(99)").unwrap();
    let val = lua.global::<Val>("c").unwrap();
    let counter = Counter::from_lua(val, lua.state()).unwrap();
    assert_eq!(counter.count, 99);

    let result = Counter::from_lua(Val::Num(42.0), lua.state());
    assert!(result.is_err());

    lua.exec("c = Counter.new(21)").unwrap();
    let val = lua.global::<Val>("c").unwrap();
    let mut counter = Counter::from_lua(val, lua.state()).unwrap();
    counter.count *= 2;
    let result_val = counter.into_lua(lua.state_mut()).unwrap();
    lua.set_global("c2", result_val).unwrap();
    lua.exec("assert(c2:value() == 42)").unwrap();
}

use rilua::{Lua, LuaApi, LuaApiMut};
use rilua_derive::{FromLua, IntoLua};

#[derive(IntoLua, FromLua)]
struct Position {
    x: f64,
    y: f64,
}

#[derive(IntoLua, FromLua)]
struct Config {
    width: u32,
    height: u32,
    fullscreen: bool,
}

#[derive(IntoLua, FromLua)]
struct Nested {
    name: String,
    pos: Position,
}

#[derive(IntoLua, FromLua)]
struct Optional {
    required: i32,
    optional: Option<String>,
}

#[derive(IntoLua, FromLua)]
struct Renamed {
    #[lua(rename = "customName")]
    field: String,
}

#[test]
fn test_table_derives() {
    use rilua::conversion::{FromLua, IntoLua};
    use rilua::vm::value::Val;

    let mut lua = Lua::new().unwrap();

    let pos = Position { x: 1.0, y: 2.0 };
    let val = pos.into_lua(lua.state_mut()).unwrap();
    lua.set_global("pos", val).unwrap();
    lua.exec("assert(pos.x == 1.0 and pos.y == 2.0)").unwrap();

    lua.exec("pos2 = { x = 3.0, y = 4.0 }").unwrap();
    let val = lua.global::<Val>("pos2").unwrap();
    let pos2 = Position::from_lua(val, lua.state()).unwrap();
    assert_eq!(pos2.x, 3.0);
    assert_eq!(pos2.y, 4.0);

    let result = Position::from_lua(Val::Num(42.0), lua.state());
    assert!(result.is_err());

    let cfg = Config {
        width: 1920,
        height: 1080,
        fullscreen: true,
    };
    let val = cfg.into_lua(lua.state_mut()).unwrap();
    lua.set_global("cfg", val).unwrap();
    lua.exec("assert(cfg.width == 1920 and cfg.height == 1080 and cfg.fullscreen == true)")
        .unwrap();

    let nested = Nested {
        name: "test".to_string(),
        pos: Position { x: 5.0, y: 6.0 },
    };
    let val = nested.into_lua(lua.state_mut()).unwrap();
    lua.set_global("nested", val).unwrap();
    lua.exec("assert(nested.name == 'test' and nested.pos.x == 5.0 and nested.pos.y == 6.0)")
        .unwrap();

    let opt1 = Optional {
        required: 42,
        optional: Some("hello".to_string()),
    };
    let val = opt1.into_lua(lua.state_mut()).unwrap();
    lua.set_global("opt1", val).unwrap();
    lua.exec("assert(opt1.required == 42 and opt1.optional == 'hello')")
        .unwrap();

    let opt2 = Optional {
        required: 99,
        optional: None,
    };
    let val = opt2.into_lua(lua.state_mut()).unwrap();
    lua.set_global("opt2", val).unwrap();
    lua.exec("assert(opt2.required == 99)").unwrap();
    lua.exec("assert(opt2.optional == nil)").unwrap();

    let renamed = Renamed {
        field: "test".to_string(),
    };
    let val = renamed.into_lua(lua.state_mut()).unwrap();
    lua.set_global("renamed", val).unwrap();
    lua.exec("assert(renamed.customName == 'test')").unwrap();
    lua.exec("assert(renamed.field == nil)").unwrap();
}

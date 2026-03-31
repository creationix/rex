//! C FFI for LuaJIT. Exposes `rex_encode` and `rex_compile`.
//!
//! `rex_encode` takes a lua_State* and reads the value at stack position 1,
//! walking tables directly — no JSON serialization needed.
//!
//! `rex_compile` takes a Rex source string and returns REXC bytecode.
//!
//! Both return results as Lua strings pushed onto the stack.

use std::os::raw::{c_char, c_int, c_double};
mod ffi;

use rex_core::bytecode::{self, Value};

// ── Lua C API types and constants ───────────────────────────────────────

// Opaque lua_State pointer
type LuaState = *mut std::ffi::c_void;

const LUA_TNIL: c_int = 0;
const LUA_TBOOLEAN: c_int = 1;
const LUA_TNUMBER: c_int = 3;
const LUA_TSTRING: c_int = 4;
const LUA_TTABLE: c_int = 5;

unsafe extern "C" {
    fn lua_type(L: LuaState, idx: c_int) -> c_int;
    fn lua_toboolean(L: LuaState, idx: c_int) -> c_int;
    fn lua_tonumber(L: LuaState, idx: c_int) -> c_double;
    fn lua_tolstring(L: LuaState, idx: c_int, len: *mut usize) -> *const c_char;
    fn lua_objlen(L: LuaState, idx: c_int) -> usize;
    fn lua_pushnil(L: LuaState);
    fn lua_next(L: LuaState, idx: c_int) -> c_int;
    fn lua_gettop(L: LuaState) -> c_int;
    fn lua_settop(L: LuaState, idx: c_int) -> ();
    fn lua_pushlstring(L: LuaState, s: *const c_char, len: usize) -> ();
    fn lua_pushstring(L: LuaState, s: *const c_char) -> ();
    fn luaL_error(L: LuaState, fmt: *const c_char, ...) -> c_int;
    fn lua_rawgeti(L: LuaState, idx: c_int, n: c_int) -> ();
}

// ── Lua value → bytecode::Value ─────────────────────────────────────────

unsafe fn lua_to_value(l: LuaState, idx: c_int) -> Value {
    let t = unsafe { lua_type(l, idx) };
    match t {
        LUA_TNIL => Value::Ref("n".into()),
        LUA_TBOOLEAN => {
            let b = unsafe { lua_toboolean(l, idx) };
            Value::Ref(if b != 0 { "t" } else { "f" }.into())
        }
        LUA_TNUMBER => {
            let n = unsafe { lua_tonumber(l, idx) };
            if n.is_nan() {
                Value::Ref("nan".into())
            } else if n.is_infinite() {
                Value::Ref(if n > 0.0 { "inf" } else { "nif" }.into())
            } else if n.fract() == 0.0 && n.abs() < i64::MAX as f64 {
                Value::Integer(n as i64)
            } else {
                let (sig, exp) = split_number(n);
                Value::Decimal { sig, exp }
            }
        }
        LUA_TSTRING => {
            let mut len: usize = 0;
            let ptr = unsafe { lua_tolstring(l, idx, &mut len) };
            let s = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr as *const u8, len)) };
            Value::String(s.to_string())
        }
        LUA_TTABLE => unsafe { lua_table_to_value(l, idx) },
        _ => Value::Ref("no".into()), // unsupported types → none
    }
}

/// Determine if a Lua table is an array (consecutive integer keys 1..n)
/// or a map (string keys), and convert accordingly.
unsafe fn lua_table_to_value(l: LuaState, idx: c_int) -> Value {
    let len = unsafe { lua_objlen(l, idx) };

    if len > 0 {
        // Array: read indices 1..len
        let mut items = Vec::with_capacity(len);
        for i in 1..=len as c_int {
            unsafe { lua_rawgeti(l, idx, i) };
            items.push(unsafe { lua_to_value(l, -1) });
            unsafe { lua_settop(l, -2) }; // pop
        }
        Value::Array(items)
    } else {
        // Could be empty array or a map — iterate with lua_next
        let mut pairs = Vec::new();
        unsafe { lua_pushnil(l) };
        let abs_idx = if idx < 0 {
            let top = unsafe { lua_gettop(l) };
            top + idx
        } else {
            idx
        };
        while unsafe { lua_next(l, abs_idx) } != 0 {
            // key at -2, value at -1
            let key = unsafe { lua_to_value(l, -2) };
            let val = unsafe { lua_to_value(l, -1) };
            // Convert numeric keys to strings for map representation
            let key = match key {
                Value::Integer(n) => Value::String(format!("{n}")),
                Value::String(s) => Value::String(s),
                other => Value::String(format!("{other:?}")),
            };
            pairs.push((key, val));
            unsafe { lua_settop(l, -2) }; // pop value, keep key for next iteration
        }
        if pairs.is_empty() {
            Value::Array(vec![]) // empty table → empty array
        } else {
            Value::Object(pairs)
        }
    }
}

// ── Exported C functions ────────────────────────────────────────────────

/// Lua C function: encode(value) → string
///
/// Takes any Lua value at stack position 1 and returns RX bytecode as a
/// Lua string. Tables are walked directly — no JSON intermediate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rex_encode(l: LuaState) -> c_int {
    let value = unsafe { lua_to_value(l, 1) };
    let encoded = bytecode::encode_dedup(&value);
    unsafe { lua_pushlstring(l, encoded.as_ptr() as *const c_char, encoded.len()) };
    1 // one return value
}

/// Lua C function: compile(source) → string
///
/// Takes a Rex source string at stack position 1 and returns REXC bytecode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rex_compile(l: LuaState) -> c_int {
    let t = unsafe { lua_type(l, 1) };
    if t != LUA_TSTRING {
        unsafe {
            lua_pushstring(l, b"compile: expected string argument\0".as_ptr() as *const c_char);
            luaL_error(l, b"%s\0".as_ptr() as *const c_char);
        }
        return 0;
    }
    let mut len: usize = 0;
    let ptr = unsafe { lua_tolstring(l, 1, &mut len) };
    let source = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr as *const u8, len)) };

    let encoded = rex_core::compile(source);
    unsafe { lua_pushlstring(l, encoded.as_ptr() as *const c_char, encoded.len()) };
    1
}

/// Module entry point: luaopen_rex_native
///
/// Called by `require("rex_native")`. Pushes a table with `encode` and
/// `compile` functions onto the stack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn luaopen_rex_native(l: LuaState) -> c_int {
    unsafe {
        // Create a table with 2 entries
        lua_createtable(l, 0, 2);

        // Push encode function
        lua_pushcclosure(l, rex_encode, 0);
        lua_setfield(l, -2, b"encode\0".as_ptr() as *const c_char);

        // Push compile function
        lua_pushcclosure(l, rex_compile, 0);
        lua_setfield(l, -2, b"compile\0".as_ptr() as *const c_char);
    }
    1 // return the table
}

unsafe extern "C" {
    fn lua_createtable(L: LuaState, narr: c_int, nrec: c_int);
    fn lua_pushcclosure(L: LuaState, f: unsafe extern "C" fn(LuaState) -> c_int, n: c_int);
    fn lua_setfield(L: LuaState, idx: c_int, k: *const c_char);
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn split_number(val: f64) -> (i64, i64) {
    if val == 0.0 {
        return (0, 0);
    }
    let s = format!("{val}");
    if let Some(epos) = s.find('e').or_else(|| s.find('E')) {
        let mantissa = &s[..epos];
        let exp: i64 = s[epos + 1..].parse().unwrap_or(0);
        if let Some(dot) = mantissa.find('.') {
            let frac_len = mantissa.len() - dot - 1;
            let sig_str: String = mantissa.chars().filter(|c| *c != '.').collect();
            let sig: i64 = sig_str.parse().unwrap_or(0);
            (sig, exp - frac_len as i64)
        } else {
            let sig: i64 = mantissa.parse().unwrap_or(0);
            (sig, exp)
        }
    } else if let Some(dot) = s.find('.') {
        let frac_len = s.len() - dot - 1;
        let sig_str: String = s.chars().filter(|c| *c != '.' && *c != '-').collect();
        let mut sig: i64 = sig_str.parse().unwrap_or(0);
        if val < 0.0 { sig = -sig; }
        (sig, -(frac_len as i64))
    } else {
        let sig: i64 = s.parse().unwrap_or(0);
        (sig, 0)
    }
}

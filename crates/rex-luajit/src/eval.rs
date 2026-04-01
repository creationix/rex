//! FFI eval for LuaJIT. Pure C-ABI functions for evaluating REXC bytecode
//! with named RX bindings. No lua_State dependency — works with any FFI.
//!
//! Flow:
//!   1. Create a context: rex_ctx_new()
//!   2. Add bindings: rex_ctx_bind(ctx, name, rx_bytes) for each extern
//!   3. Mark mutables: rex_ctx_bind_mut(ctx, name, rx_bytes) for extern mut
//!   4. Evaluate: rex_ctx_eval(ctx, bytecode) → result handle
//!   5. Read result: rex_result_value(result) → RX bytes
//!   6. Read mutations: rex_result_mutations(result) → RX object of dirty mut bindings
//!   7. Free: rex_result_free(result), rex_ctx_free(ctx)
//!
//! The context can be reused across evals by calling rex_ctx_reset() between them.
//! Read-only bindings are parsed once and shared. Mut bindings are COW — decoded
//! on first bind, cloned into the var table on first write, and only dirty ones
//! are serialized back.

use std::collections::HashMap;
use std::os::raw::c_char;

use rex_core::bytecode::{self, Value};
use rex_core::interpret::{self, Context, RexValue};

// ── RexValue ↔ bytecode::Value conversion ──────────────────────────────

fn rexvalue_to_value(v: &RexValue) -> Value {
    match v {
        RexValue::RexNone => Value::Ref("no".into()),
        RexValue::Null => Value::Ref("n".into()),
        RexValue::Bool(true) => Value::Ref("t".into()),
        RexValue::Bool(false) => Value::Ref("f".into()),
        RexValue::Int(n) => Value::Integer(*n),
        RexValue::Float(f) => {
            if f.is_nan() { return Value::Ref("nan".into()); }
            if f.is_infinite() {
                return Value::Ref(if *f > 0.0 { "inf" } else { "nif" }.into());
            }
            // Approximate as decimal
            let (sig, exp) = split_number(*f);
            Value::Decimal { sig, exp }
        }
        RexValue::Decimal { sig, exp } => Value::Decimal { sig: *sig, exp: *exp },
        RexValue::Str(s) => Value::String(s.clone()),
        RexValue::Array(items) => {
            Value::Array(items.iter().map(rexvalue_to_value).collect())
        }
        RexValue::Object(pairs) => {
            Value::Object(pairs.iter().map(|(k, v)| {
                (Value::String(k.clone()), rexvalue_to_value(v))
            }).collect())
        }
        RexValue::Host(_) => Value::Ref("no".into()),
    }
}

fn rx_to_rexvalue(rx: &str) -> RexValue {
    let val = match bytecode::decode(rx) {
        Ok(v) => v,
        Err(_) => return RexValue::RexNone,
    };
    value_to_rexvalue(&val)
}

fn value_to_rexvalue(v: &Value) -> RexValue {
    match v {
        Value::Integer(n) => RexValue::Int(*n),
        Value::Decimal { sig, exp } => RexValue::Decimal { sig: *sig, exp: *exp },
        Value::String(s) => RexValue::Str(s.clone()),
        Value::Ref(name) => match name.as_str() {
            "t" => RexValue::Bool(true),
            "f" => RexValue::Bool(false),
            "n" => RexValue::Null,
            "no" => RexValue::RexNone,
            "nan" => RexValue::Float(f64::NAN),
            "inf" => RexValue::Float(f64::INFINITY),
            "nif" => RexValue::Float(f64::NEG_INFINITY),
            _ => RexValue::RexNone,
        },
        Value::Array(items) => {
            RexValue::Array(items.iter().map(value_to_rexvalue).collect())
        }
        Value::Object(pairs) => {
            RexValue::Object(pairs.iter().map(|(k, v)| {
                let key = match k {
                    Value::String(s) => s.clone(),
                    _ => format!("{k:?}"),
                };
                (key, value_to_rexvalue(v))
            }).collect())
        }
        _ => RexValue::RexNone,
    }
}

fn rexvalue_to_rx(v: &RexValue) -> String {
    let val = rexvalue_to_value(v);
    bytecode::encode_dedup(&val)
}

// ── Context ────────────────────────────────────────────────────────────

pub struct EvalContext {
    /// Read-only bindings: name → RexValue (decoded once, reused)
    readonly: HashMap<String, RexValue>,
    /// Mutable bindings: name → RexValue (decoded once, COW into vars on eval)
    mutable: HashMap<String, RexValue>,
    /// Gas limit for eval
    gas_limit: u64,
}

pub struct EvalResult {
    /// Return value as RX bytes
    value_rx: String,
    /// Mutations: RX object containing only dirty extern mut bindings
    mutations_rx: String,
    /// Gas used
    gas: u64,
}

// ── Lifecycle ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn rex_ctx_new() -> *mut EvalContext {
    Box::into_raw(Box::new(EvalContext {
        readonly: HashMap::new(),
        mutable: HashMap::new(),
        gas_limit: 0,
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_ctx_free(ctx: *mut EvalContext) {
    if !ctx.is_null() {
        unsafe { drop(Box::from_raw(ctx)) };
    }
}

/// Set gas limit. 0 = unlimited.
#[unsafe(no_mangle)]
pub extern "C" fn rex_ctx_gas(ctx: *mut EvalContext, limit: u64) {
    let ctx = unsafe { &mut *ctx };
    ctx.gas_limit = limit;
}

/// Add a read-only binding. The RX bytes are decoded once and cached.
#[unsafe(no_mangle)]
pub extern "C" fn rex_ctx_bind(
    ctx: *mut EvalContext,
    name: *const c_char, name_len: usize,
    rx: *const c_char, rx_len: usize,
) {
    let ctx = unsafe { &mut *ctx };
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name as *const u8, name_len)) };
    let rx = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(rx as *const u8, rx_len)) };
    let val = rx_to_rexvalue(rx);
    ctx.readonly.insert(name.to_string(), val);
}

/// Add a mutable binding. Decoded once. COW: cloned into var table on first write.
#[unsafe(no_mangle)]
pub extern "C" fn rex_ctx_bind_mut(
    ctx: *mut EvalContext,
    name: *const c_char, name_len: usize,
    rx: *const c_char, rx_len: usize,
) {
    let ctx = unsafe { &mut *ctx };
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name as *const u8, name_len)) };
    let rx = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(rx as *const u8, rx_len)) };
    let val = rx_to_rexvalue(rx);
    ctx.mutable.insert(name.to_string(), val);
}

/// Clear all bindings (keeps allocation).
#[unsafe(no_mangle)]
pub extern "C" fn rex_ctx_reset(ctx: *mut EvalContext) {
    let ctx = unsafe { &mut *ctx };
    ctx.readonly.clear();
    ctx.mutable.clear();
}

// ── Eval ───────────────────────────────────────────────────────────────

/// Evaluate REXC bytecode with the current bindings.
/// Returns an opaque result handle. The context's mutable bindings are
/// NOT modified — mutations are captured in the result.
#[unsafe(no_mangle)]
pub extern "C" fn rex_ctx_eval(
    ctx: *mut EvalContext,
    bytecode: *const c_char, bc_len: usize,
) -> *mut EvalResult {
    let ctx = unsafe { &*ctx };
    let bc = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(bytecode as *const u8, bc_len)) };

    // Build var table: all bindings (readonly + mutable) start in vars.
    // After eval, we diff the mutable ones to find dirty bindings.
    let mut vars = HashMap::new();
    for (name, val) in &ctx.readonly {
        vars.insert(name.clone(), val.clone());
    }
    for (name, val) in &ctx.mutable {
        vars.insert(name.clone(), val.clone());
    }

    let mut interp_ctx = Context::default();
    interp_ctx.vars = vars;
    interp_ctx.gas_limit = ctx.gas_limit;

    let (value, mutations_rx, gas) = match interpret::run(bc, interp_ctx) {
        Ok(result) => {
            // Find dirty mutable bindings by comparing with originals
            let mut dirty_pairs: Vec<(Value, Value)> = Vec::new();
            for (name, original) in &ctx.mutable {
                if let Some(current) = result.vars.get(name) {
                    // Compare by serialization (cheap for small values)
                    let orig_rx = rexvalue_to_rx(original);
                    let cur_rx = rexvalue_to_rx(current);
                    if orig_rx != cur_rx {
                        dirty_pairs.push((
                            Value::String(name.clone()),
                            rexvalue_to_value(current),
                        ));
                    }
                }
            }

            let mutations = if dirty_pairs.is_empty() {
                String::new()
            } else {
                bytecode::encode(&Value::Object(dirty_pairs))
            };

            (rexvalue_to_rx(&result.value), mutations, result.gas)
        }
        Err(e) => {
            // On error, return none value and empty mutations
            let err_msg = format!("{e}");
            let err_val = bytecode::encode(&Value::String(err_msg));
            (err_val, String::new(), 0)
        }
    };

    Box::into_raw(Box::new(EvalResult {
        value_rx: value,
        mutations_rx,
        gas,
    }))
}

// ── Result access ──────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn rex_result_value(result: *const EvalResult, out_len: *mut usize) -> *const c_char {
    let result = unsafe { &*result };
    unsafe { *out_len = result.value_rx.len() };
    result.value_rx.as_ptr() as *const c_char
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_result_mutations(result: *const EvalResult, out_len: *mut usize) -> *const c_char {
    let result = unsafe { &*result };
    unsafe { *out_len = result.mutations_rx.len() };
    result.mutations_rx.as_ptr() as *const c_char
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_result_gas(result: *const EvalResult) -> u64 {
    let result = unsafe { &*result };
    result.gas
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_result_free(result: *mut EvalResult) {
    if !result.is_null() {
        unsafe { drop(Box::from_raw(result)) };
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn split_number(val: f64) -> (i64, i64) {
    if val == 0.0 { return (0, 0); }
    let s = format!("{val}");
    if let Some(dot) = s.find('.') {
        let frac_len = s.len() - dot - 1;
        let sig_str: String = s.chars().filter(|c| *c != '.' && *c != '-').collect();
        let mut sig: i64 = sig_str.parse().unwrap_or(0);
        if val < 0.0 { sig = -sig; }
        (sig, -(frac_len as i64))
    } else {
        (s.parse().unwrap_or(0), 0)
    }
}

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

use rex_core::bytecode::{self, Value as BValue};
use rex_core::heap::{Value, Heap, FloatValue};
use rex_core::interpret::Context;

// ── Value ↔ bytecode::Value conversion ──────────────────────────────

fn heap_value_to_bvalue(v: Value, heap: &Heap) -> BValue {
    if v.is_none() { return BValue::Ref("no".into()); }
    if v.is_null() { return BValue::Ref("n".into()); }
    if let Some(true) = v.as_bool() { return BValue::Ref("t".into()); }
    if let Some(false) = v.as_bool() { return BValue::Ref("f".into()); }
    if let Some(n) = v.as_i64() { return BValue::Integer(n); }
    if let Some(id) = v.float_id() {
        match &heap.floats[id as usize] {
            FloatValue::Float(f) => {
                if f.is_nan() { return BValue::Ref("nan".into()); }
                if f.is_infinite() {
                    return BValue::Ref(if *f > 0.0 { "inf" } else { "nif" }.into());
                }
                let (sig, exp) = split_number(*f);
                return BValue::Decimal { sig, exp };
            }
            FloatValue::Decimal { sig, exp } => return BValue::Decimal { sig: *sig, exp: *exp },
        }
    }
    if let Some(s) = v.as_str(heap) { return BValue::String(s.to_string()); }
    if v.is_array() {
        return BValue::Array(heap.array_items(v).iter().map(|&item| heap_value_to_bvalue(item, heap)).collect());
    }
    if v.is_object() {
        return BValue::Object(heap.object_pairs(v).iter().map(|&(k, val)| {
            (BValue::String(heap.resolve_str(k).to_string()), heap_value_to_bvalue(val, heap))
        }).collect());
    }
    BValue::Ref("no".into())
}

fn rx_to_heap_value(rx: &str, heap: &mut Heap) -> Value {
    let val = match bytecode::decode(rx) {
        Ok(v) => v,
        Err(_) => return Value::NONE,
    };
    bvalue_to_heap_value(&val, heap)
}

fn bvalue_to_heap_value(v: &BValue, heap: &mut Heap) -> Value {
    match v {
        BValue::Integer(n) => Value::int(*n),
        BValue::Decimal { sig, exp } => heap.alloc_decimal(*sig, *exp),
        BValue::String(s) => heap.intern_value(s),
        BValue::Ref(name) => match name.as_str() {
            "t" => Value::TRUE,
            "f" => Value::FALSE,
            "n" => Value::NULL,
            "no" => Value::NONE,
            "nan" => heap.alloc_float(f64::NAN),
            "inf" => heap.alloc_float(f64::INFINITY),
            "nif" => heap.alloc_float(f64::NEG_INFINITY),
            _ => Value::NONE,
        },
        BValue::Array(items) => {
            let vals: Vec<Value> = items.iter().map(|item| bvalue_to_heap_value(item, heap)).collect();
            heap.alloc_array(vals)
        }
        BValue::Object(pairs) => {
            let ps: Vec<(u32, Value)> = pairs.iter().map(|(k, v)| {
                let key = match k {
                    BValue::String(s) => heap.intern(s),
                    _ => heap.intern(&format!("{k:?}")),
                };
                (key, bvalue_to_heap_value(v, heap))
            }).collect();
            heap.alloc_object(ps)
        }
        _ => Value::NONE,
    }
}

fn heap_value_to_rx(v: Value, heap: &Heap) -> String {
    let bval = heap_value_to_bvalue(v, heap);
    bytecode::encode_dedup(&bval)
}

// ── Context ────────────────────────────────────────────────────────────

pub struct EvalContext {
    /// Read-only bindings: name → RX string (decoded into heap on eval)
    readonly: HashMap<String, String>,
    /// Mutable bindings: name → RX string (decoded into heap on eval)
    mutable: HashMap<String, String>,
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

/// Add a read-only binding. The RX bytes are stored and decoded on eval.
#[unsafe(no_mangle)]
pub extern "C" fn rex_ctx_bind(
    ctx: *mut EvalContext,
    name: *const c_char, name_len: usize,
    rx: *const c_char, rx_len: usize,
) {
    let ctx = unsafe { &mut *ctx };
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name as *const u8, name_len)) };
    let rx = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(rx as *const u8, rx_len)) };
    ctx.readonly.insert(name.to_string(), rx.to_string());
}

/// Add a mutable binding.
#[unsafe(no_mangle)]
pub extern "C" fn rex_ctx_bind_mut(
    ctx: *mut EvalContext,
    name: *const c_char, name_len: usize,
    rx: *const c_char, rx_len: usize,
) {
    let ctx = unsafe { &mut *ctx };
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name as *const u8, name_len)) };
    let rx = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(rx as *const u8, rx_len)) };
    ctx.mutable.insert(name.to_string(), rx.to_string());
}

/// Clear all bindings (keeps allocation).
#[unsafe(no_mangle)]
pub extern "C" fn rex_ctx_reset(ctx: *mut EvalContext) {
    let ctx = unsafe { &mut *ctx };
    ctx.readonly.clear();
    ctx.mutable.clear();
}

// ── Eval ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn rex_ctx_eval(
    ctx: *mut EvalContext,
    bytecode_ptr: *const c_char, bc_len: usize,
) -> *mut EvalResult {
    let ctx = unsafe { &*ctx };
    let bc = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(bytecode_ptr as *const u8, bc_len)) };

    let mut heap = Heap::new();
    let mut vars = HashMap::new();

    // Decode bindings into heap
    for (name, rx) in &ctx.readonly {
        vars.insert(name.clone(), rx_to_heap_value(rx, &mut heap));
    }
    for (name, rx) in &ctx.mutable {
        vars.insert(name.clone(), rx_to_heap_value(rx, &mut heap));
    }

    let mut interp_ctx = Context::default();
    interp_ctx.vars = vars;
    interp_ctx.gas_limit = ctx.gas_limit;
    interp_ctx.heap = heap;

    let (value, mutations_rx, gas) = match rex_core::interpret::run(bc, interp_ctx) {
        Ok(result) => {
            // Find dirty mutable bindings by comparing with originals
            let mut dirty_pairs: Vec<(BValue, BValue)> = Vec::new();
            for (name, orig_rx) in &ctx.mutable {
                if let Some(&current) = result.vars.get(name) {
                    let cur_rx = heap_value_to_rx(current, &result.heap);
                    if *orig_rx != cur_rx {
                        dirty_pairs.push((
                            BValue::String(name.clone()),
                            heap_value_to_bvalue(current, &result.heap),
                        ));
                    }
                }
            }

            let mutations = if dirty_pairs.is_empty() {
                String::new()
            } else {
                bytecode::encode(&BValue::Object(dirty_pairs))
            };

            (heap_value_to_rx(result.value, &result.heap), mutations, result.gas)
        }
        Err(e) => {
            let err_msg = format!("{e}");
            let err_val = bytecode::encode(&BValue::String(err_msg));
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

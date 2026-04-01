use napi::bindgen_prelude::*;
use napi::{sys, ValueType};
use napi_derive::napi;
use rex_core::bytecode;

/// Compile Rex source code to REXC bytecode with full optimizations.
#[napi]
pub fn compile(source: String) -> String {
    rex_core::compile(&source)
}

/// Encode a JS value (JSON types) to RX bytecode with deduplication.
#[napi]
pub fn encode(env: Env, value: Unknown) -> Result<String> {
    let val = js_to_value(&env, value.raw())?;
    Ok(bytecode::encode_dedup(&val))
}

/// Walk a JS value and convert to bytecode::Value.
fn js_to_value(env: &Env, raw: sys::napi_value) -> Result<bytecode::Value> {
    let ty = unsafe {
        let mut result = 0;
        sys::napi_typeof(env.raw(), raw, &mut result);
        ValueType::from(result)
    };

    match ty {
        ValueType::Undefined => Ok(bytecode::Value::Ref("no".into())),
        ValueType::Null => Ok(bytecode::Value::Ref("n".into())),
        ValueType::Boolean => {
            let mut val = false;
            unsafe { sys::napi_get_value_bool(env.raw(), raw, &mut val) };
            Ok(bytecode::Value::Ref(if val { "t" } else { "f" }.into()))
        }
        ValueType::Number => {
            let mut val = 0.0f64;
            unsafe { sys::napi_get_value_double(env.raw(), raw, &mut val) };
            if val.is_nan() {
                Ok(bytecode::Value::Ref("nan".into()))
            } else if val.is_infinite() {
                Ok(bytecode::Value::Ref(
                    if val > 0.0 { "inf" } else { "nif" }.into(),
                ))
            } else if val.fract() == 0.0 && val.abs() < i64::MAX as f64 {
                Ok(bytecode::Value::Integer(val as i64))
            } else {
                let (sig, exp) = split_number(val);
                Ok(bytecode::Value::Decimal { sig, exp })
            }
        }
        ValueType::String => {
            let s = napi_get_string(env, raw)?;
            Ok(bytecode::Value::String(s))
        }
        ValueType::Object => {
            let mut is_arr = false;
            unsafe { sys::napi_is_array(env.raw(), raw, &mut is_arr) };
            if is_arr {
                let mut len: u32 = 0;
                unsafe { sys::napi_get_array_length(env.raw(), raw, &mut len) };
                let mut items = Vec::with_capacity(len as usize);
                for i in 0..len {
                    let mut elem = std::ptr::null_mut();
                    unsafe { sys::napi_get_element(env.raw(), raw, i, &mut elem) };
                    items.push(js_to_value(env, elem)?);
                }
                Ok(bytecode::Value::Array(items))
            } else {
                let mut names_val = std::ptr::null_mut();
                unsafe { sys::napi_get_property_names(env.raw(), raw, &mut names_val) };
                let mut len: u32 = 0;
                unsafe { sys::napi_get_array_length(env.raw(), names_val, &mut len) };
                let mut pairs = Vec::with_capacity(len as usize);
                for i in 0..len {
                    let mut key_val = std::ptr::null_mut();
                    unsafe { sys::napi_get_element(env.raw(), names_val, i, &mut key_val) };
                    let key = napi_get_string(env, key_val)?;
                    let mut prop_val = std::ptr::null_mut();
                    unsafe { sys::napi_get_property(env.raw(), raw, key_val, &mut prop_val) };
                    let val = js_to_value(env, prop_val)?;
                    pairs.push((bytecode::Value::String(key), val));
                }
                Ok(bytecode::Value::Object(pairs))
            }
        }
        _ => Ok(bytecode::Value::Ref("no".into())),
    }
}

/// Get a Rust String from a napi string value.
fn napi_get_string(env: &Env, val: sys::napi_value) -> Result<String> {
    let mut len = 0usize;
    unsafe {
        sys::napi_get_value_string_utf8(env.raw(), val, std::ptr::null_mut(), 0, &mut len);
    }
    let mut buf = vec![0u8; len + 1];
    let mut written = 0usize;
    unsafe {
        sys::napi_get_value_string_utf8(
            env.raw(),
            val,
            buf.as_mut_ptr() as *mut _,
            len + 1,
            &mut written,
        );
    }
    buf.truncate(written);
    Ok(unsafe { String::from_utf8_unchecked(buf) })
}

/// Split a float into (significand, exponent) where value = sig * 10^exp.
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
        if val < 0.0 {
            sig = -sig;
        }
        (sig, -(frac_len as i64))
    } else {
        let sig: i64 = s.parse().unwrap_or(0);
        (sig, 0)
    }
}

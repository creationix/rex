//! FFI encoder for LuaJIT. Small C functions that LuaJIT can call via FFI.
//!
//! The encoder state is an opaque pointer. Lua pushes values in forward
//! order. Sized containers (`;` `:`) record their start position at open
//! and insert the size prefix at close.

use std::os::raw::c_char;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use rex_core::bytecode::{encode_varint, zigzag_encode, encode_varint_buf};

/// Opaque encoder state.
pub struct FfiEncoder {
    buf: Vec<u8>,
    /// Stack of body-start positions for open containers
    stack: Vec<ContainerFrame>,
    /// String dedup: hash → (offset, len)
    seen_strings: HashMap<u64, (usize, usize)>,
    /// Schema dedup: schema_hash → offset of first object with that schema
    schemas: HashMap<u64, usize>,
    /// String chain prefixes
    prefixes: HashSet<String>,
}

struct ContainerFrame {
    kind: ContainerKind,
    body_start: usize,
    /// For objects: schema hash built from keys
    schema_hasher: Option<std::collections::hash_map::DefaultHasher>,
    schema_key_count: usize,
}

#[derive(Clone, Copy)]
enum ContainerKind {
    Array, // → ;
    Object, // → :
}

const CHAIN_DELIMITER: u8 = b'/';
const CHAIN_THRESHOLD: usize = 8;

// ── Lifecycle ───────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_new() -> *mut FfiEncoder {
    let enc = Box::new(FfiEncoder {
        buf: Vec::with_capacity(64 * 1024),
        stack: Vec::with_capacity(32),
        seen_strings: HashMap::new(),
        schemas: HashMap::new(),
        prefixes: HashSet::new(),
    });
    Box::into_raw(enc)
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_free(enc: *mut FfiEncoder) {
    if !enc.is_null() {
        unsafe { drop(Box::from_raw(enc)) };
    }
}

/// Finalize and return the encoded bytes. Caller must copy the result
/// before the next call or free. Returns ptr and len via out params.
#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_finish(enc: *mut FfiEncoder, out_len: *mut usize) -> *const c_char {
    let enc = unsafe { &mut *enc };
    unsafe { *out_len = enc.buf.len() };
    enc.buf.as_ptr() as *const c_char
}

/// Reset the encoder for reuse (avoids reallocation).
#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_reset(enc: *mut FfiEncoder) {
    let enc = unsafe { &mut *enc };
    enc.buf.clear();
    enc.stack.clear();
    enc.seen_strings.clear();
    enc.schemas.clear();
    enc.prefixes.clear();
}

// ── Scalars ─────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_null(enc: *mut FfiEncoder) {
    let enc = unsafe { &mut *enc };
    enc.buf.extend_from_slice(b"n'");
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_boolean(enc: *mut FfiEncoder, val: i32) {
    let enc = unsafe { &mut *enc };
    if val != 0 { enc.buf.extend_from_slice(b"t'"); }
    else { enc.buf.extend_from_slice(b"f'"); }
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_integer(enc: *mut FfiEncoder, val: i64) {
    let enc = unsafe { &mut *enc };
    push_varint(&mut enc.buf, zigzag_encode(val));
    enc.buf.push(b'+');
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_decimal(enc: *mut FfiEncoder, sig: i64, exp: i64) {
    let enc = unsafe { &mut *enc };
    push_varint(&mut enc.buf, zigzag_encode(exp));
    enc.buf.push(b'*');
    push_varint(&mut enc.buf, zigzag_encode(sig));
    enc.buf.push(b'+');
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_number(enc: *mut FfiEncoder, val: f64) {
    let enc = unsafe { &mut *enc };
    if val.is_nan() { enc.buf.extend_from_slice(b"nan'"); return; }
    if val.is_infinite() {
        if val > 0.0 { enc.buf.extend_from_slice(b"inf'"); }
        else { enc.buf.extend_from_slice(b"nif'"); }
        return;
    }
    if val.fract() == 0.0 && val.abs() < i64::MAX as f64 {
        push_varint(&mut enc.buf, zigzag_encode(val as i64));
        enc.buf.push(b'+');
    } else {
        let (sig, exp) = split_number(val);
        push_varint(&mut enc.buf, zigzag_encode(exp));
        enc.buf.push(b'*');
        push_varint(&mut enc.buf, zigzag_encode(sig));
        enc.buf.push(b'+');
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_string(enc: *mut FfiEncoder, ptr: *const c_char, len: usize) {
    let enc = unsafe { &mut *enc };
    let s = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };

    // Dedup check
    if len >= 2 {
        let hash = hash_string(s);
        if let Some(&(target, target_len)) = enc.seen_strings.get(&hash) {
            let current = enc.buf.len();
            let delta = current - target;
            let ptr_size = varint_len(delta as u64) + 1;
            if ptr_size < target_len {
                push_varint(&mut enc.buf, delta as u64);
                enc.buf.push(b'^');
                return;
            }
        }

        let start = enc.buf.len();

        // Try chain
        if len >= CHAIN_THRESHOLD {
            let s_str = unsafe { std::str::from_utf8_unchecked(s) };
            if s[1..].contains(&CHAIN_DELIMITER) {
                let mut offset = len;
                loop {
                    offset = match s[..offset].iter().rposition(|&b| b == CHAIN_DELIMITER) {
                        Some(p) => p,
                        None => break,
                    };
                    if offset == 0 { break; }
                    if enc.prefixes.contains(&s_str[..offset]) {
                        // Chain: emit prefix, then suffix, then '.' tag with size
                        let body_start = enc.buf.len();
                        // Prefix (goes through dedup/chain recursively)
                        let prefix = s_str[..offset].to_string();
                        let suffix = &s[offset..];
                        // Write prefix
                        rex_enc_string(enc as *mut FfiEncoder, prefix.as_ptr() as *const c_char, prefix.len());
                        // Write suffix
                        push_varint(&mut enc.buf, suffix.len() as u64);
                        enc.buf.push(b',');
                        enc.buf.extend_from_slice(suffix);
                        // Chain tag
                        let body_len = enc.buf.len() - body_start;
                        insert_size_prefix(&mut enc.buf, body_start, b'.', body_len);
                        register_prefixes(&mut enc.prefixes, s_str);
                        let total_len = enc.buf.len() - start;
                        enc.seen_strings.entry(hash).or_insert((start, total_len));
                        return;
                    }
                }
                register_prefixes(&mut enc.prefixes, s_str);
            }
        }

        // Plain string
        push_varint(&mut enc.buf, len as u64);
        enc.buf.push(b',');
        enc.buf.extend_from_slice(s);
        let total_len = enc.buf.len() - start;
        enc.seen_strings.entry(hash).or_insert((start, total_len));
    } else {
        // Tiny string
        push_varint(&mut enc.buf, len as u64);
        enc.buf.push(b',');
        enc.buf.extend_from_slice(s);
    }
}

// ── Object key (string that also feeds schema hash) ─────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_key(enc: *mut FfiEncoder, ptr: *const c_char, len: usize) {
    let enc = unsafe { &mut *enc };
    // Feed key into schema hasher
    if let Some(frame) = enc.stack.last_mut() {
        if let Some(ref mut hasher) = frame.schema_hasher {
            let s = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
            s.hash(hasher);
            frame.schema_key_count += 1;
        }
    }
    // Write the key as a string
    rex_enc_string(enc as *mut FfiEncoder, ptr, len);
}

// ── Containers ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_open_array(enc: *mut FfiEncoder) {
    let enc = unsafe { &mut *enc };
    let body_start = enc.buf.len();
    enc.stack.push(ContainerFrame {
        kind: ContainerKind::Array,
        body_start,
        schema_hasher: None,
        schema_key_count: 0,
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_close_array(enc: *mut FfiEncoder) {
    let enc = unsafe { &mut *enc };
    let frame = enc.stack.pop().expect("close_array without open_array");
    let body_len = enc.buf.len() - frame.body_start;
    insert_size_prefix(&mut enc.buf, frame.body_start, b';', body_len);
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_open_object(enc: *mut FfiEncoder) {
    let enc = unsafe { &mut *enc };
    let body_start = enc.buf.len();
    enc.stack.push(ContainerFrame {
        kind: ContainerKind::Object,
        body_start,
        schema_hasher: Some(std::collections::hash_map::DefaultHasher::new()),
        schema_key_count: 0,
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_enc_close_object(enc: *mut FfiEncoder) {
    let enc = unsafe { &mut *enc };
    let frame = enc.stack.pop().expect("close_object without open_object");

    // Compute schema key
    let schema_key = if let Some(mut hasher) = frame.schema_hasher {
        frame.schema_key_count.hash(&mut hasher);
        hasher.finish()
    } else { 0 };

    // Check for schema match
    if let Some(&schema_offset) = enc.schemas.get(&schema_key) {
        // Schema match — rewrite body as pointer + values only.
        // The body currently has key1,val1,key2,val2,...
        // We need to extract just values and prepend a pointer.
        // This is complex to do in-place, so for the FFI path we skip
        // schema rewriting and just record for future matches.
        // TODO: implement schema rewriting for FFI path
    }

    let body_len = enc.buf.len() - frame.body_start;
    let obj_start = enc.buf.len() + varint_len(body_len as u64) + 1; // after size prefix
    insert_size_prefix(&mut enc.buf, frame.body_start, b':', body_len);

    // Record schema for future objects
    enc.schemas.entry(schema_key).or_insert(frame.body_start);
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn push_varint(buf: &mut Vec<u8>, n: u64) {
    let mut tmp = [0u8; 11];
    let len = encode_varint_buf(n, &mut tmp);
    buf.extend_from_slice(&tmp[..len]);
}

fn varint_len(n: u64) -> usize {
    if n == 0 { return 0; }
    let mut len = 0;
    let mut v = n;
    while v > 0 { len += 1; v /= 64; }
    len
}

fn insert_size_prefix(buf: &mut Vec<u8>, body_start: usize, tag: u8, body_len: usize) {
    let prefix = encode_varint(body_len as u64);
    let prefix_bytes = prefix.as_bytes();
    let shift = prefix_bytes.len() + 1;
    buf.reserve(shift);
    // Shift body right
    let old_len = buf.len();
    buf.resize(old_len + shift, 0);
    buf.copy_within(body_start..old_len, body_start + shift);
    // Write prefix
    buf[body_start..body_start + prefix_bytes.len()].copy_from_slice(prefix_bytes);
    buf[body_start + prefix_bytes.len()] = tag;
}

fn hash_string(s: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    0u8.hash(&mut h);
    s.hash(&mut h);
    h.finish()
}

fn register_prefixes(prefixes: &mut HashSet<String>, s: &str) {
    let bytes = s.as_bytes();
    let mut offset = 0;
    loop {
        let next = match bytes[offset + 1..].iter().position(|&b| b == CHAIN_DELIMITER) {
            Some(p) => offset + 1 + p,
            None => break,
        };
        prefixes.insert(s[..next].to_string());
        offset = next;
    }
}

fn split_number(val: f64) -> (i64, i64) {
    if val == 0.0 { return (0, 0); }
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
            (mantissa.parse().unwrap_or(0), exp)
        }
    } else if let Some(dot) = s.find('.') {
        let frac_len = s.len() - dot - 1;
        let sig_str: String = s.chars().filter(|c| *c != '.' && *c != '-').collect();
        let mut sig: i64 = sig_str.parse().unwrap_or(0);
        if val < 0.0 { sig = -sig; }
        (sig, -(frac_len as i64))
    } else {
        (s.parse().unwrap_or(0), 0)
    }
}

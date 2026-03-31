//! Rex bytecode (v2): left-to-right encoding and decoding.
//!
//! Format: `[b64 varint][tag][body]` — every value starts with optional
//! base-64 digits (the varint), followed by a non-b64 tag byte that gives
//! the varint its meaning, optionally followed by a body.

use std::fmt;

// ── b64 alphabet ────────────────────────────────────────────────────────

const B64: &[u8; 64] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";

fn b64_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'z' => Some(byte - b'a' + 10),
        b'A'..=b'Z' => Some(byte - b'A' + 36),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn is_b64(byte: u8) -> bool {
    b64_val(byte).is_some()
}

// ── Varint encoding ─────────────────────────────────────────────────────

/// Encode a u64 as big-endian base-64 digits into a stack buffer.
/// Returns the slice of the buffer that was written.
/// 0 encodes as empty (0 bytes). Max 11 digits for u64.
pub fn encode_varint_buf(mut n: u64, buf: &mut [u8; 11]) -> usize {
    if n == 0 {
        return 0;
    }
    let mut len = 0usize;
    while n > 0 {
        buf[len] = B64[(n % 64) as usize];
        len += 1;
        n /= 64;
    }
    buf[..len].reverse();
    len
}

/// Encode a u64 as big-endian base-64 digits. 0 encodes as empty string.
/// Allocating version — prefer `encode_varint_buf` in hot paths.
pub fn encode_varint(n: u64) -> String {
    let mut buf = [0u8; 11];
    let len = encode_varint_buf(n, &mut buf);
    unsafe { String::from_utf8_unchecked(buf[..len].to_vec()) }
}

/// Decode a varint, also returning the raw b64 bytes (for name-based tags).
fn decode_varint_raw<'a>(input: &'a [u8], pos: &mut usize) -> &'a [u8] {
    let start = *pos;
    while *pos < input.len() && is_b64(input[*pos]) {
        *pos += 1;
    }
    &input[start..*pos]
}

// ── Zigzag encoding ─────────────────────────────────────────────────────

pub fn zigzag_encode(n: i64) -> u64 {
    if n >= 0 {
        (n as u64) * 2
    } else {
        ((-n - 1) as u64) * 2 + 1
    }
}

fn zigzag_decode(n: u64) -> i64 {
    if n % 2 == 0 {
        (n / 2) as i64
    } else {
        -((n / 2) as i64) - 1
    }
}

// ── Value type ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    Integer(i64),
    Decimal { sig: i64, exp: i64 },
    String(String),
    Ref(String),
    Variable(String),
    Opcode(String),
    SelfRef(u32),
    BreakCont(u32),
    Pointer(u32),

    Array(Vec<Value>),
    Object(Vec<(Value, Value)>),
    Block(Vec<Value>),
    Call(Vec<Value>),

    When(Vec<Value>),
    Unless(Vec<Value>),
    Or(Vec<Value>),
    And(Vec<Value>),
    ForIn(Vec<Value>),
    ForOf(Vec<Value>),
    While(Vec<Value>),

    ListCompIn(Vec<Value>),
    ListCompOf(Vec<Value>),
    ListCompWhile(Vec<Value>),
    MapCompIn(Vec<Value>),
    MapCompOf(Vec<Value>),
    MapCompWhile(Vec<Value>),

    Chain(Vec<Value>),

    Set(Box<Value>, Box<Value>),
    Swap(Box<Value>, Box<Value>),
    Delete(Box<Value>),
    Return(Box<Value>),
}

// ── Encoder ─────────────────────────────────────────────────────────────

pub fn encode(value: &Value) -> String {
    let mut out = String::new();
    encode_into(value, &mut out);
    out
}

fn encode_into(value: &Value, out: &mut String) {
    match value {
        Value::Integer(n) => {
            out.push_str(&encode_varint(zigzag_encode(*n)));
            out.push('+');
        }
        Value::Decimal { sig, exp } => {
            out.push_str(&encode_varint(zigzag_encode(*exp)));
            out.push('*');
            out.push_str(&encode_varint(zigzag_encode(*sig)));
            out.push('+');
        }
        Value::String(s) => {
            out.push_str(&encode_varint(s.len() as u64));
            out.push(',');
            out.push_str(s);
        }
        Value::Ref(name) => {
            out.push_str(name);
            out.push('\'');
        }
        Value::Variable(name) => {
            out.push_str(name);
            out.push('$');
        }
        Value::Opcode(name) => {
            out.push_str(name);
            out.push('%');
        }
        Value::SelfRef(depth) => {
            out.push_str(&encode_varint(*depth as u64));
            out.push('@');
        }
        Value::BreakCont(v) => {
            out.push_str(&encode_varint(*v as u64));
            out.push('\\');
        }
        Value::Pointer(delta) => {
            out.push_str(&encode_varint(*delta as u64));
            out.push('^');
        }

        // Paired containers
        Value::Array(items) => encode_paired('[', ']', items, out),
        Value::Object(pairs) => {
            out.push('{');
            for (k, v) in pairs {
                encode_into(k, out);
                encode_into(v, out);
            }
            out.push('}');
        }
        Value::Block(items) => encode_paired('{', '}', items, out),
        Value::Call(items) => encode_paired('(', ')', items, out),

        // Compound containers
        Value::When(items) => encode_compound('?', '(', ')', items, out),
        Value::Unless(items) => encode_compound('!', '(', ')', items, out),
        Value::Or(items) => encode_compound('|', '(', ')', items, out),
        Value::And(items) => encode_compound('&', '(', ')', items, out),
        Value::ForIn(items) => encode_compound('>', '(', ')', items, out),
        Value::ForOf(items) => encode_compound('<', '(', ')', items, out),
        Value::While(items) => encode_compound('#', '(', ')', items, out),

        Value::ListCompIn(items) => encode_compound('>', '[', ']', items, out),
        Value::ListCompOf(items) => encode_compound('<', '[', ']', items, out),
        Value::ListCompWhile(items) => encode_compound('#', '[', ']', items, out),
        Value::MapCompIn(items) => encode_compound('>', '{', '}', items, out),
        Value::MapCompOf(items) => encode_compound('<', '{', '}', items, out),
        Value::MapCompWhile(items) => encode_compound('#', '{', '}', items, out),

        // String chain (template literals)
        Value::Chain(items) => encode_sized_body('.', items, out),

        // Mutation (fixed arity, no size)
        Value::Set(place, val) => {
            out.push('=');
            encode_into(place, out);
            encode_into(val, out);
        }
        Value::Swap(place, val) => {
            out.push('/');
            encode_into(place, out);
            encode_into(val, out);
        }
        Value::Delete(place) => {
            out.push('~');
            encode_into(place, out);
        }
        Value::Return(val) => {
            out.push(';');
            encode_into(val, out);
        }
    }
}

fn encode_sized_body(tag: char, items: &[Value], out: &mut String) {
    let mut body = String::new();
    for item in items {
        encode_into(item, &mut body);
    }
    out.push_str(&encode_varint(body.len() as u64));
    out.push(tag);
    out.push_str(&body);
}

fn encode_paired(open: char, close: char, items: &[Value], out: &mut String) {
    let mut body = String::new();
    for item in items {
        encode_into(item, &mut body);
    }
    out.push(open);
    out.push_str(&body);
    out.push(close);
}

// ── Indexed container encoding ─────────────────────────────────────────

/// Minimum b64 digits needed to represent `n`. Always at least 1.
fn b64_width(n: u64) -> usize {
    varint_len(n).max(1)
}

/// Encode `n` as exactly `width` b64 digits (zero-padded on the left).
fn encode_fixed_b64(n: u64, width: usize, out: &mut String) {
    let mut digits = [b'0'; 8];
    let mut val = n;
    for i in (0..width).rev() {
        digits[i] = B64[(val % 64) as usize];
        val /= 64;
    }
    for &d in &digits[..width] {
        out.push(d as char);
    }
}

/// Decode a fixed-width b64 number from `width` bytes at `pos`.
fn decode_fixed_b64(input: &[u8], pos: &mut usize, width: usize) -> u64 {
    let mut n: u64 = 0;
    for _ in 0..width {
        if *pos < input.len() {
            n = n * 64 + b64_val(input[*pos]).unwrap_or(0) as u64;
            *pos += 1;
        }
    }
    n
}

/// Encode an array with an index for random access.
/// Format: `[ <packed># <pointers> <elements> ]`
pub fn encode_indexed_array(items: &[Value]) -> String {
    let mut out = String::new();
    encode_indexed_array_into(items, &mut out);
    out
}

fn encode_indexed_array_into(items: &[Value], out: &mut String) {
    if items.is_empty() {
        out.push_str("[]");
        return;
    }

    // Encode all elements, tracking start offsets
    let mut body = String::new();
    let mut offsets = Vec::with_capacity(items.len());
    for item in items {
        offsets.push(body.len());
        encode_into(item, &mut body);
    }

    // Pointer width = min b64 digits for the largest offset
    let max_offset = *offsets.last().unwrap();
    let width = b64_width(max_offset as u64);

    // packed = (count << 3) | (width - 1)
    let packed = ((items.len() as u64) << 3) | ((width as u64) - 1);

    out.push('[');
    out.push_str(&encode_varint(packed));
    out.push('#');
    for &offset in &offsets {
        encode_fixed_b64(offset as u64, width, out);
    }
    out.push_str(&body);
    out.push(']');
}

/// Encode an object with an index for random access.
/// Pointers are sorted by encoded key for O(log n) binary search.
/// Format: `{ <packed># <sorted-pointers> <key0><val0>... }`
pub fn encode_indexed_object(pairs: &[(Value, Value)]) -> String {
    let mut out = String::new();
    encode_indexed_object_into(pairs, &mut out);
    out
}

fn encode_indexed_object_into(pairs: &[(Value, Value)], out: &mut String) {
    if pairs.is_empty() {
        out.push_str("{}");
        return;
    }

    // Encode all key-value pairs in original order, tracking offsets and keys
    let mut body = String::new();
    let mut offsets = Vec::with_capacity(pairs.len());
    let mut encoded_keys: Vec<String> = Vec::with_capacity(pairs.len());
    for (k, v) in pairs {
        let pair_start = body.len();
        let key_start = body.len();
        encode_into(k, &mut body);
        encoded_keys.push(body[key_start..].to_string());
        encode_into(v, &mut body);
        offsets.push(pair_start);
    }

    // Sort indices by encoded key bytes
    let mut sorted: Vec<usize> = (0..pairs.len()).collect();
    sorted.sort_by(|&a, &b| encoded_keys[a].as_bytes().cmp(encoded_keys[b].as_bytes()));

    let max_offset = *offsets.iter().max().unwrap();
    let width = b64_width(max_offset as u64);
    let packed = ((pairs.len() as u64) << 3) | ((width as u64) - 1);

    out.push('{');
    out.push_str(&encode_varint(packed));
    out.push('#');
    for &idx in &sorted {
        encode_fixed_b64(offsets[idx] as u64, width, out);
    }
    out.push_str(&body);
    out.push('}');
}

fn encode_compound(modifier: char, open: char, close: char, items: &[Value], out: &mut String) {
    out.push(modifier);
    out.push(open);
    let is_cond = is_conditional_modifier(modifier);
    for (i, item) in items.iter().enumerate() {
        encode_skippable(item, is_cond && i > 0, out);
    }
    out.push(close);
}

fn is_conditional_modifier(modifier: char) -> bool {
    matches!(modifier, '?' | '!' | '|' | '&')
}

/// Containers that lack a built-in size prefix and need one for O(1) skipping.
fn is_container(value: &Value) -> bool {
    matches!(value, Value::Block(_) | Value::Array(_) | Value::Object(_) | Value::Call(_)
        | Value::When(_) | Value::Unless(_) | Value::Or(_) | Value::And(_)
        | Value::ForIn(_) | Value::ForOf(_) | Value::While(_)
        | Value::ListCompIn(_) | Value::ListCompOf(_) | Value::ListCompWhile(_)
        | Value::MapCompIn(_) | Value::MapCompOf(_) | Value::MapCompWhile(_))
}

/// Encode a value. When `skip` is true, containers get a length prefix
/// for O(1) skipping. Return is transparent — it passes `skip` through
/// to its child (`;` itself never gets a size prefix).
fn encode_skippable(value: &Value, skip: bool, out: &mut String) {
    match value {
        Value::Return(child) if skip => {
            out.push(';');
            encode_skippable(child, true, out);
        }
        v if skip && is_container(v) => {
            let mut body = String::new();
            encode_into(v, &mut body);
            let size = match v {
                Value::Block(_) | Value::Array(_) | Value::Object(_) | Value::Call(_) => body.len() - 2,
                _ => body.len().saturating_sub(3),
            };
            out.push_str(&encode_varint(size as u64));
            out.push_str(&body);
        }
        _ => encode_into(value, out),
    }
}

// ── Deduplicating encoder ────────────────────────────────────────────────

/// Returns the number of b64 digits needed to encode `n`. 0 encodes as 0 digits.
fn varint_len(n: u64) -> usize {
    if n == 0 {
        return 0;
    }
    let mut len = 0;
    let mut v = n;
    while v > 0 {
        len += 1;
        v /= 64;
    }
    len
}

/// Encode with deduplication. Writes in reverse (DFS, children before
/// parents) so sized-container headers know their body length without
/// fixups. The buffer is reversed at the end.
pub fn encode_dedup(value: &Value) -> String {
    let mut node_counts = std::collections::HashMap::new();
    prescan_counts(value, &mut node_counts);

    let mut enc = RevEncoder {
        buf: Vec::new(),
        pos: 0,
        seen: std::collections::HashMap::new(),
        schemas: std::collections::HashMap::new(),
        prefixes: std::collections::HashSet::new(),
        node_counts,
        value_hashes: std::collections::HashMap::new(),
        scope_depth: 0,
    };
    enc.write(value);
    enc.buf.reverse();
    unsafe { String::from_utf8_unchecked(enc.buf) }
}

const DEDUP_COMPLEXITY_LIMIT: usize = 32;

/// Count nodes bottom-up. Returns the count. Only inserts containers
/// into the map (scalars always have count 1, no need to store them).
fn prescan_counts(
    value: &Value,
    counts: &mut std::collections::HashMap<*const Value, usize>,
) -> usize {
    match value {
        Value::Array(items) | Value::Block(items) | Value::Call(items)
        | Value::When(items) | Value::Unless(items) | Value::Or(items) | Value::And(items)
        | Value::ForIn(items) | Value::ForOf(items) | Value::While(items)
        | Value::ListCompIn(items) | Value::ListCompOf(items) | Value::ListCompWhile(items)
        | Value::MapCompIn(items) | Value::MapCompOf(items) | Value::MapCompWhile(items)
        | Value::Chain(items) => {
            let c: usize = 1 + items.iter().map(|i| prescan_counts(i, counts)).sum::<usize>();
            counts.insert(value as *const Value, c);
            c
        }
        Value::Object(pairs) => {
            let c: usize = 1 + pairs.iter().map(|(k, v)| prescan_counts(k, counts) + prescan_counts(v, counts)).sum::<usize>();
            counts.insert(value as *const Value, c);
            c
        }
        Value::Set(a, b) | Value::Swap(a, b) => {
            let c = 1 + prescan_counts(a, counts) + prescan_counts(b, counts);
            counts.insert(value as *const Value, c);
            c
        }
        Value::Delete(a) | Value::Return(a) => {
            let c = 1 + prescan_counts(a, counts);
            counts.insert(value as *const Value, c);
            c
        }
        _ => 1, // scalars — don't store
    }
}

/// Reverse encoder: pushes bytes right-to-left so sized containers
/// naturally know their body length. `pos` counts absolute forward
/// offset. No fixups needed.
/// Default delimiter for string chaining.
const CHAIN_DELIMITER: u8 = b'/';
/// Minimum string length to attempt chaining.
const CHAIN_THRESHOLD: usize = 8;

pub struct RevEncoder {
    buf: Vec<u8>,
    pos: usize,
    /// hash → (rev_left, encoded_len, scope_depth)
    seen: std::collections::HashMap<u64, (usize, usize, u32)>,
    /// schema hash → (rev_start, len) of the first object with that key layout
    schemas: std::collections::HashMap<u64, (usize, usize)>,
    /// known string prefixes (delimiter-split) for chain dedup
    prefixes: std::collections::HashSet<String>,
    node_counts: std::collections::HashMap<*const Value, usize>,
    value_hashes: std::collections::HashMap<*const Value, u64>,
    /// Current conditional nesting depth. Pointers may only reference
    /// targets recorded at the same or lower (ancestor) depth.
    scope_depth: u32,
}

impl RevEncoder {
    /// Create a new RevEncoder (no prescan, no container dedup).
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(1024 * 1024), // 1MB initial capacity
            pos: 0,
            seen: std::collections::HashMap::new(),
            schemas: std::collections::HashMap::new(),
            prefixes: std::collections::HashSet::new(),
            node_counts: std::collections::HashMap::new(),
            value_hashes: std::collections::HashMap::new(),
            scope_depth: 0,
        }
    }

    /// Finalize and return the encoded string.
    pub fn finish(mut self) -> String {
        self.buf.reverse();
        unsafe { String::from_utf8_unchecked(self.buf) }
    }

    /// Current forward byte position.
    pub fn pos(&self) -> usize { self.pos }

    /// Write an integer value.
    pub fn write_integer(&mut self, n: i64) {
        self.push(b'+');
        self.push_varint(zigzag_encode(n));
    }

    /// Write a decimal value (sig × 10^exp).
    pub fn write_decimal(&mut self, sig: i64, exp: i64) {
        self.push(b'+');
        self.push_varint(zigzag_encode(sig));
        self.push(b'*');
        self.push_varint(zigzag_encode(exp));
    }

    /// Write a named reference (true/false/null/undefined/nan/inf/nif).
    pub fn write_ref(&mut self, name: &str) {
        self.push(b'\'');
        self.push_str_rev(name.as_bytes());
    }

    /// Begin a sized container body. Returns the position before the body.
    /// Call `finish_sized` after writing children.
    pub fn begin_body(&self) -> usize { self.pos }

    /// Finish a sized container: emit tag + size prefix for the body
    /// that started at `before`.
    pub fn finish_sized(&mut self, tag: u8, before: usize) {
        let body_len = self.pos - before;
        self.push(tag);
        self.push_varint(body_len as u64);
    }

    /// Record a schema key → (left_pos, len) for schema sharing.
    pub fn record_schema(&mut self, schema_key: u64, left_pos: usize, len: usize) {
        self.schemas.entry(schema_key).or_insert((left_pos, len));
    }

    /// Look up a schema by key. Returns (left_pos, len).
    pub fn get_schema(&self, schema_key: u64) -> Option<(usize, usize)> {
        self.schemas.get(&schema_key).copied()
    }

    /// Emit a pointer to a previously-written value at `target_left`.
    pub fn write_pointer(&mut self, target_left: usize) {
        let delta = (self.pos - target_left) as u64;
        self.push(b'^');
        self.push_varint(delta);
    }

    pub fn push(&mut self, b: u8) { self.buf.push(b); self.pos += 1; }

    /// Push a varint directly into buf (no allocation).
    pub fn push_varint(&mut self, n: u64) {
        let mut buf = [0u8; 11];
        let len = encode_varint_buf(n, &mut buf);
        // Push reversed (for the final buf.reverse())
        for i in (0..len).rev() {
            self.buf.push(buf[i]);
        }
        self.pos += len;
    }

    pub fn push_str_rev(&mut self, s: &[u8]) {
        self.buf.extend(s.iter().rev());
        self.pos += s.len();
    }

    pub fn write(&mut self, value: &Value) {
        // Strings have their own fast path with integrated dedup + chaining
        if let Value::String(s) = value {
            self.write_string(s);
            return;
        }

        if let Some(key) = self.dedup_key(value) {
            if let Some(&(target_left, target_len, target_depth)) = self.seen.get(&key) {
                // Only deduplicate if the target is at the same or ancestor scope.
                // Cross-branch pointers cause bugs when the target branch is skipped.
                if target_depth <= self.scope_depth {
                    let delta = (self.pos - target_left) as u64;
                    let ptr_size = varint_len(delta) + 1;
                    if ptr_size < target_len {
                        self.push(b'^');
                        self.push_varint(delta);
                        return;
                    }
                }
            }
            let start = self.pos;
            self.emit(value);
            let len = self.pos - start;
            self.seen.entry(key).or_insert((self.pos, len, self.scope_depth));
        } else {
            self.emit(value);
        }
    }

    fn dedup_key(&self, value: &Value) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        // Pre-computed hash from prescan
        if let Some(&h) = self.value_hashes.get(&(value as *const Value)) {
            return Some(h);
        }
        // Small containers only (strings handled by write_string)
        match value {
            Value::Array(_) | Value::Object(_) | Value::Block(_) | Value::Call(_) | Value::Chain(_) => {
                let c = self.node_counts.get(&(value as *const Value)).copied().unwrap_or(1);
                if c >= 2 && c <= DEDUP_COMPLEXITY_LIMIT {
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    value.hash(&mut h);
                    Some(h.finish())
                } else { None }
            }
            _ => None,
        }
    }


    fn emit(&mut self, value: &Value) {
        match value {
            Value::Integer(n) => { self.push(b'+'); self.push_varint(zigzag_encode(*n)); }
            Value::Decimal { sig, exp } => {
                self.push(b'+'); self.push_varint(zigzag_encode(*sig));
                self.push(b'*'); self.push_varint(zigzag_encode(*exp));
            }
            Value::String(s) => self.write_string(s),
            Value::Ref(n) => { self.push(b'\''); self.push_str_rev(n.as_bytes()); }
            Value::Variable(n) => { self.push(b'$'); self.push_str_rev(n.as_bytes()); }
            Value::Opcode(n) => { self.push(b'%'); self.push_str_rev(n.as_bytes()); }
            Value::SelfRef(d) => { self.push(b'@'); self.push_varint(*d as u64); }
            Value::BreakCont(v) => { self.push(b'\\'); self.push_varint(*v as u64); }
            Value::Pointer(d) => { self.push(b'^'); self.push_varint(*d as u64); }

            // Paired: closer, children reversed, opener
            Value::Array(items) => { self.push(b']'); for i in items.iter().rev() { self.write(i); } self.push(b'['); }
            Value::Object(pairs) => self.emit_object(pairs),
            Value::Block(items) => { self.push(b'}'); for i in items.iter().rev() { self.write(i); } self.push(b'{'); }
            Value::Call(items) => { self.push(b')'); for i in items.iter().rev() { self.write(i); } self.push(b'('); }

            // Compound: closer, children reversed, opener, modifier
            Value::When(items) => self.emit_compound(b'?', b'(', b')', items),
            Value::Unless(items) => self.emit_compound(b'!', b'(', b')', items),
            Value::Or(items) => self.emit_compound(b'|', b'(', b')', items),
            Value::And(items) => self.emit_compound(b'&', b'(', b')', items),
            Value::ForIn(items) => self.emit_compound(b'>', b'(', b')', items),
            Value::ForOf(items) => self.emit_compound(b'<', b'(', b')', items),
            Value::While(items) => self.emit_compound(b'#', b'(', b')', items),
            Value::ListCompIn(items) => self.emit_compound(b'>', b'[', b']', items),
            Value::ListCompOf(items) => self.emit_compound(b'<', b'[', b']', items),
            Value::ListCompWhile(items) => self.emit_compound(b'#', b'[', b']', items),
            Value::MapCompIn(items) => self.emit_compound(b'>', b'{', b'}', items),
            Value::MapCompOf(items) => self.emit_compound(b'<', b'{', b'}', items),
            Value::MapCompWhile(items) => self.emit_compound(b'#', b'{', b'}', items),

            // Chain (template literal string concatenation)
            Value::Chain(items) => {
                let before = self.pos;
                for item in items.iter().rev() { self.write(item); }
                let body_len = self.pos - before;
                self.push(b'.'); self.push_varint(body_len as u64);
            }

            // Mutation: reversed order
            Value::Set(p, v) => { self.write(v); self.write(p); self.push(b'='); }
            Value::Swap(p, v) => { self.write(v); self.write(p); self.push(b'/'); }
            Value::Delete(p) => { self.write(p); self.push(b'~'); }
            Value::Return(v) => {
                self.write(v);
                self.push(b';');
            }
        }
    }

    /// Emit a string, with dedup and chaining. Accepts `&str` to avoid
    /// allocating `Value::String` temporaries for chain segments.
    pub fn write_string(&mut self, s: &str) {
        let sb = s.as_bytes();

        // Dedup check (string content hash)
        if sb.len() >= 2 {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            0u8.hash(&mut h);
            s.hash(&mut h);
            let key = h.finish();

            if let Some(&(target_left, target_len, target_depth)) = self.seen.get(&key) {
                if target_depth <= self.scope_depth {
                    let delta = (self.pos - target_left) as u64;
                    let ptr_size = varint_len(delta) + 1;
                    if ptr_size < target_len {
                        self.push(b'^');
                        self.push_varint(delta);
                        return;
                    }
                }
            }

            let start = self.pos;

            // Try chaining
            if sb.len() >= CHAIN_THRESHOLD && sb[1..].contains(&CHAIN_DELIMITER) {
                let mut offset = sb.len();
                loop {
                    offset = match sb[..offset].iter().rposition(|&b| b == CHAIN_DELIMITER) {
                        Some(p) => p,
                        None => break,
                    };
                    if offset == 0 { break; }
                    if self.prefixes.contains(&s[..offset]) {
                        let before = self.pos;
                        self.write_string(&s[offset..]);
                        self.write_string(&s[..offset]);
                        let body_len = self.pos - before;
                        self.push(b'.');
                        self.push_varint(body_len as u64);
                        self.register_chain_prefixes(s);
                        let len = self.pos - start;
                        self.seen.entry(key).or_insert((self.pos, len, self.scope_depth));
                        return;
                    }
                }
                self.register_chain_prefixes(s);
            }

            // Plain string
            self.push_str_rev(sb);
            self.push(b',');
            self.push_varint(sb.len() as u64);

            let len = self.pos - start;
            self.seen.entry(key).or_insert((self.pos, len, self.scope_depth));
        } else {
            // Tiny string — no dedup
            self.push_str_rev(sb);
            self.push(b',');
            self.push_varint(sb.len() as u64);
        }
    }

    /// Register all delimiter-delimited prefixes of `s` for future chaining.
    fn register_chain_prefixes(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let mut offset = 0;
        loop {
            let next = match bytes[offset + 1..].iter().position(|&b| b == CHAIN_DELIMITER) {
                Some(p) => offset + 1 + p,
                None => break,
            };
            self.prefixes.insert(s[..next].to_string());
            offset = next;
        }
    }

    fn emit_object(&mut self, pairs: &[(Value, Value)]) {
        if pairs.is_empty() {
            self.push(b'}');
            self.push(b'{');
            return;
        }

        let schema = Self::schema_key(pairs);

        if let Some(&(schema_left, _schema_len)) = self.schemas.get(&schema) {
            // Schema match: emit values + pointer to schema, wrapped in {}
            self.push(b'}');
            for (_k, v) in pairs.iter().rev() {
                self.write(v);
            }
            let delta = (self.pos - schema_left) as u64;
            self.push(b'^');
            self.push_varint(delta);
            self.push(b'{');
        } else {
            // First occurrence: encode all key-value pairs
            let before = self.pos;
            self.push(b'}');
            for (k, v) in pairs.iter().rev() {
                self.write(v);
                self.write(k);
            }
            self.push(b'{');
            let obj_len = self.pos - before;
            self.schemas.insert(schema, (self.pos, obj_len));
        }
    }

    /// Compute a schema key from a map's keys. Two maps with the same
    /// keys in the same order produce the same key.
    fn schema_key(pairs: &[(Value, Value)]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        pairs.len().hash(&mut h);
        for (k, _) in pairs {
            k.hash(&mut h);
        }
        h.finish()
    }

    fn emit_compound(&mut self, modifier: u8, open: u8, close: u8, items: &[Value]) {
        // Conditional branches (when/unless/or/and) may be skipped at runtime,
        // so values inside them must not be dedup targets for outer scopes.
        let is_conditional = matches!(modifier, b'?' | b'!' | b'|' | b'&');
        if is_conditional { self.scope_depth += 1; }
        self.push(close);
        for (i, item) in items.iter().enumerate().rev() {
            if i > 0 && is_conditional {
                self.write_skippable(item);
            } else {
                self.write(item);
            }
        }
        self.push(open);
        self.push(modifier);
        if is_conditional { self.scope_depth -= 1; }
    }

    /// Write a value in a skip position. Containers get a length prefix.
    /// Return is transparent — passes skip through to its child.
    fn write_skippable(&mut self, value: &Value) {
        match value {
            Value::Return(child) => {
                self.write_skippable(child);
                self.push(b';');
            }
            v if is_container(v) => {
                let before = self.pos;
                self.write(v);
                let full_len = self.pos - before;
                let size = match v {
                    Value::Block(_) | Value::Array(_) | Value::Object(_) | Value::Call(_) => full_len - 2,
                    _ => full_len.saturating_sub(3),
                };
                self.push_varint(size as u64);
            }
            _ => self.write(value),
        }
    }
}

// ── Decoder ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct DecodeError {
    pub pos: usize,
    pub message: String,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "decode error at {}: {}", self.pos, self.message)
    }
}

/// Decode bytecode to a Value tree, resolving pointers and chains.
pub fn decode(input: &str) -> Result<Value, DecodeError> {
    let bytes = input.as_bytes();
    let mut pos = 0;
    read_value(bytes, &mut pos, true)
}

/// Decode bytecode preserving pointers and chains (no resolution).
pub fn decode_raw(input: &str) -> Result<Value, DecodeError> {
    let bytes = input.as_bytes();
    let mut pos = 0;
    read_value(bytes, &mut pos, false)
}

fn read_value(input: &[u8], pos: &mut usize, resolve: bool) -> Result<Value, DecodeError> {
    let value = decode_one(input, pos, resolve)?;
    if resolve {
        if let Value::Pointer(delta) = &value {
            let target_pos = *pos + *delta as usize;
            let mut tpos = target_pos;
            return read_value(input, &mut tpos, true);
        }
    }
    Ok(value)
}

fn decode_one(input: &[u8], pos: &mut usize, resolve: bool) -> Result<Value, DecodeError> {
    if *pos >= input.len() {
        return Err(DecodeError {
            pos: *pos,
            message: "unexpected end of input".into(),
        });
    }

    // Read varint (b64 digits) — may be empty
    let varint_raw = decode_varint_raw(input, pos);

    if *pos >= input.len() {
        return Err(DecodeError {
            pos: *pos,
            message: "unexpected end of input after varint".into(),
        });
    }

    let tag = input[*pos];
    *pos += 1;

    match tag {
        // Scalars
        b'+' => {
            let n = varint_from_raw(varint_raw);
            Ok(Value::Integer(zigzag_decode(n)))
        }
        b'*' => {
            // Decimal: [exp]*[sig]+
            let exp = zigzag_decode(varint_from_raw(varint_raw));
            let sig_value = read_value(input, pos, resolve)?;
            match sig_value {
                Value::Integer(sig) => Ok(Value::Decimal { sig, exp }),
                _ => Err(DecodeError {
                    pos: *pos,
                    message: "expected integer after decimal exponent".into(),
                }),
            }
        }
        b',' => {
            // String: [len],[bytes]
            let len = varint_from_raw(varint_raw) as usize;
            if *pos + len > input.len() {
                return Err(DecodeError {
                    pos: *pos,
                    message: "string extends past end of input".into(),
                });
            }
            let s = std::str::from_utf8(&input[*pos..*pos + len])
                .map_err(|_| DecodeError {
                    pos: *pos,
                    message: "invalid utf-8 in string".into(),
                })?
                .to_owned();
            *pos += len;
            Ok(Value::String(s))
        }
        b'\'' => {
            let name = std::str::from_utf8(varint_raw)
                .map_err(|_| DecodeError {
                    pos: *pos,
                    message: "invalid utf-8 in ref name".into(),
                })?
                .to_owned();
            Ok(Value::Ref(name))
        }
        b'$' => {
            let name = std::str::from_utf8(varint_raw)
                .map_err(|_| DecodeError {
                    pos: *pos,
                    message: "invalid utf-8 in variable name".into(),
                })?
                .to_owned();
            Ok(Value::Variable(name))
        }
        b'%' => {
            let name = std::str::from_utf8(varint_raw)
                .map_err(|_| DecodeError {
                    pos: *pos,
                    message: "invalid utf-8 in opcode name".into(),
                })?
                .to_owned();
            Ok(Value::Opcode(name))
        }
        b'@' => {
            let depth = varint_from_raw(varint_raw) as u32;
            Ok(Value::SelfRef(depth))
        }
        b'\\' => {
            let v = varint_from_raw(varint_raw) as u32;
            Ok(Value::BreakCont(v))
        }
        b'^' => {
            let delta = varint_from_raw(varint_raw) as u32;
            Ok(Value::Pointer(delta))
        }

        // Paired containers
        b'(' => decode_paired_body(input, pos, b')', resolve, |items| Value::Call(items)),
        b'[' => {
            // Check for index: at least one b64 digit followed by '#'
            if peek_is_index(input, *pos) {
                decode_indexed_array(input, pos, resolve)
            } else {
                decode_paired_body(input, pos, b']', resolve, |items| Value::Array(items))
            }
        }
        b'{' => {
            // Check for indexed object
            if peek_is_index(input, *pos) {
                return decode_indexed_object(input, pos, resolve);
            }

            let mut children = Vec::new();
            while *pos < input.len() && input[*pos] != b'}' {
                children.push(read_value(input, pos, resolve)?);
            }
            if *pos < input.len() && input[*pos] == b'}' {
                *pos += 1;
            } else {
                return Err(DecodeError { pos: *pos, message: "expected closing '}'".into() });
            }

            if children.is_empty() {
                return Ok(Value::Object(vec![]));
            }

            match &children[0] {
                // First child is a string → explicit key-value object
                Value::String(_) if children.len() % 2 == 0 => {
                    let pairs = children.chunks(2)
                        .map(|chunk| (chunk[0].clone(), chunk[1].clone()))
                        .collect();
                    Ok(Value::Object(pairs))
                }
                // First child is an object (schema pointer resolved) → schema-shared object
                Value::Object(schema_pairs) => {
                    let pairs = schema_pairs.iter().zip(children[1..].iter())
                        .map(|((k, _), v)| (k.clone(), v.clone()))
                        .collect();
                    Ok(Value::Object(pairs))
                }
                Value::Array(schema_keys) => {
                    let pairs = schema_keys.iter().zip(children[1..].iter())
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    Ok(Value::Object(pairs))
                }
                // Otherwise → code block
                _ => Ok(Value::Block(children))
            }
        }

        // Compound modifiers
        b'?' | b'!' | b'|' | b'&' | b'>' | b'<' | b'#' => {
            decode_compound(input, pos, tag, resolve)
        }

        // Mutation (fixed arity)
        b'=' => {
            let place = read_value(input, pos, resolve)?;
            let val = read_value(input, pos, resolve)?;
            Ok(Value::Set(Box::new(place), Box::new(val)))
        }
        b'/' => {
            let place = read_value(input, pos, resolve)?;
            let val = read_value(input, pos, resolve)?;
            Ok(Value::Swap(Box::new(place), Box::new(val)))
        }
        b'~' => {
            let place = read_value(input, pos, resolve)?;
            Ok(Value::Delete(Box::new(place)))
        }

        // String chain: [size].[segment1][segment2]...
        b'.' => {
            let size = varint_from_raw(varint_raw) as usize;
            let end = *pos + size;
            let mut segments = Vec::new();
            while *pos < end {
                segments.push(read_value(input, pos, resolve)?);
            }
            if resolve {
                // Concatenate segments into a single string
                let mut s = String::new();
                for seg in &segments {
                    if let Value::String(part) = seg { s.push_str(part); }
                }
                Ok(Value::String(s))
            } else {
                Ok(Value::Chain(segments))
            }
        }

        // Return: ;[value]
        b';' => {
            let val = read_value(input, pos, resolve)?;
            Ok(Value::Return(Box::new(val)))
        }

        _ => Err(DecodeError {
            pos: *pos - 1,
            message: format!("unexpected tag byte: {:?}", tag as char),
        }),
    }
}

/// Peek (without consuming) for an index header: at least one b64 digit followed by '#'.
/// An empty varint + '#' is While, not an index.
fn peek_is_index(input: &[u8], pos: usize) -> bool {
    let mut i = pos;
    while i < input.len() && is_b64(input[i]) { i += 1; }
    i > pos && i < input.len() && input[i] == b'#'
}

/// Decode an indexed array: `<packed>#<pointers><elements>]`
/// The `[` has already been consumed. Reads through `]`.
fn decode_indexed_array(input: &[u8], pos: &mut usize, resolve: bool) -> Result<Value, DecodeError> {
    let raw = decode_varint_raw(input, pos);
    let packed = varint_from_raw(raw);
    if *pos >= input.len() || input[*pos] != b'#' {
        return Err(DecodeError { pos: *pos, message: "expected '#' in indexed array".into() });
    }
    *pos += 1; // consume '#'

    let count = (packed >> 3) as usize;
    let width = ((packed & 7) + 1) as usize;

    // Skip pointer table (we decode eagerly — pointers not needed)
    *pos += count * width;

    // Read elements until ']'
    let mut items = Vec::with_capacity(count);
    while *pos < input.len() && input[*pos] != b']' {
        items.push(read_value(input, pos, resolve)?);
    }
    if *pos < input.len() && input[*pos] == b']' {
        *pos += 1;
    } else {
        return Err(DecodeError { pos: *pos, message: "expected closing ']' in indexed array".into() });
    }
    Ok(Value::Array(items))
}

/// Decode an indexed object: `<packed>#<pointers><key-value-pairs>}`
/// The `{` has already been consumed. Reads through `}`.
fn decode_indexed_object(input: &[u8], pos: &mut usize, resolve: bool) -> Result<Value, DecodeError> {
    let raw = decode_varint_raw(input, pos);
    let packed = varint_from_raw(raw);
    if *pos >= input.len() || input[*pos] != b'#' {
        return Err(DecodeError { pos: *pos, message: "expected '#' in indexed object".into() });
    }
    *pos += 1; // consume '#'

    let count = (packed >> 3) as usize;
    let width = ((packed & 7) + 1) as usize;

    // Skip pointer table (eager decode — pointers not needed)
    *pos += count * width;

    // Read key-value pairs until '}'
    let mut pairs = Vec::with_capacity(count);
    while *pos < input.len() && input[*pos] != b'}' {
        let k = read_value(input, pos, resolve)?;
        let v = read_value(input, pos, resolve)?;
        pairs.push((k, v));
    }
    if *pos < input.len() && input[*pos] == b'}' {
        *pos += 1;
    } else {
        return Err(DecodeError { pos: *pos, message: "expected closing '}' in indexed object".into() });
    }
    Ok(Value::Object(pairs))
}

fn decode_paired_body(
    input: &[u8],
    pos: &mut usize,
    close: u8,
    resolve: bool,
    wrap: impl FnOnce(Vec<Value>) -> Value,
) -> Result<Value, DecodeError> {
    let mut items = Vec::new();
    while *pos < input.len() && input[*pos] != close {
        // Skip optional size prefix (varint before a nested paired container)
        items.push(read_value(input, pos, resolve)?);
    }
    if *pos < input.len() && input[*pos] == close {
        *pos += 1; // consume closer
    } else {
        return Err(DecodeError {
            pos: *pos,
            message: format!("expected closing '{}'", close as char),
        });
    }
    Ok(wrap(items))
}

fn decode_compound(
    input: &[u8],
    pos: &mut usize,
    modifier: u8,
    resolve: bool,
) -> Result<Value, DecodeError> {
    if *pos >= input.len() {
        return Err(DecodeError {
            pos: *pos,
            message: "expected opener after compound modifier".into(),
        });
    }

    let opener = input[*pos];
    *pos += 1;

    let (closer, is_list, is_map) = match opener {
        b'(' => (b')', false, false),
        b'[' => (b']', true, false),
        b'{' => (b'}', false, true),
        _ => {
            return Err(DecodeError {
                pos: *pos - 1,
                message: format!("expected '(', '[', or '{{' after compound modifier, got '{}'", opener as char),
            });
        }
    };

    let mut items = Vec::new();
    while *pos < input.len() && input[*pos] != closer {
        items.push(read_value(input, pos, resolve)?);
    }
    if *pos < input.len() && input[*pos] == closer {
        *pos += 1;
    } else {
        return Err(DecodeError {
            pos: *pos,
            message: format!("expected closing '{}'", closer as char),
        });
    }

    let value = match (modifier, is_list, is_map) {
        (b'?', false, false) => Value::When(items),
        (b'!', false, false) => Value::Unless(items),
        (b'|', false, false) => Value::Or(items),
        (b'&', false, false) => Value::And(items),
        (b'>', false, false) => Value::ForIn(items),
        (b'<', false, false) => Value::ForOf(items),
        (b'#', false, false) => Value::While(items),

        (b'>', true, false) => Value::ListCompIn(items),
        (b'<', true, false) => Value::ListCompOf(items),
        (b'#', true, false) => Value::ListCompWhile(items),

        (b'>', false, true) => Value::MapCompIn(items),
        (b'<', false, true) => Value::MapCompOf(items),
        (b'#', false, true) => Value::MapCompWhile(items),

        _ => {
            return Err(DecodeError {
                pos: *pos,
                message: format!(
                    "invalid compound: modifier='{}' opener='{}'",
                    modifier as char, opener as char
                ),
            });
        }
    };

    Ok(value)
}

/// Convert raw b64 bytes to a u64 value.
fn varint_from_raw(raw: &[u8]) -> u64 {
    let mut n: u64 = 0;
    for &b in raw {
        n = n * 64 + b64_val(b).unwrap() as u64;
    }
    n
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── b64 / varint / zigzag ───────────────────────────────────────

    #[test]
    fn varint_encoding() {
        assert_eq!(encode_varint(0), "");
        assert_eq!(encode_varint(1), "1");
        assert_eq!(encode_varint(10), "a");
        assert_eq!(encode_varint(63), "_");
        assert_eq!(encode_varint(64), "10");
        assert_eq!(encode_varint(65), "11");
    }

    #[test]
    fn varint_decoding() {
        let mut pos = 0;
        let raw = decode_varint_raw(b"1k+", &mut pos);
        assert_eq!(varint_from_raw(raw), 84);
        assert_eq!(pos, 2); // consumed "1k", stopped at "+"
    }

    #[test]
    fn zigzag_roundtrip() {
        for n in [-100, -2, -1, 0, 1, 2, 42, 100] {
            assert_eq!(zigzag_decode(zigzag_encode(n)), n);
        }
    }

    // ── Scalar round-trips ──────────────────────────────────────────

    fn roundtrip(value: &Value) -> Value {
        let encoded = encode(value);
        decode(&encoded).unwrap_or_else(|e| panic!("decode failed for {:?}: {e}", encoded))
    }

    #[test]
    fn integer_roundtrip() {
        for n in [0, 1, -1, 42, -42, 100, -100, i64::MAX / 2, i64::MIN / 2] {
            assert_eq!(roundtrip(&Value::Integer(n)), Value::Integer(n));
        }
    }

    #[test]
    fn integer_encoding() {
        assert_eq!(encode(&Value::Integer(0)), "+");
        assert_eq!(encode(&Value::Integer(1)), "2+");
        assert_eq!(encode(&Value::Integer(-1)), "1+");
        assert_eq!(encode(&Value::Integer(42)), "1k+");
    }

    #[test]
    fn decimal_roundtrip() {
        let cases = [
            (314, -2),  // 3.14
            (5, -1),    // 0.5
            (0, 0),     // 0.0
            (100, 2),   // 10000
            (-314, -2), // -3.14
        ];
        for (sig, exp) in cases {
            let v = Value::Decimal { sig, exp };
            assert_eq!(roundtrip(&v), v);
        }
    }

    #[test]
    fn decimal_encoding() {
        // 3.14 = 314 × 10^-2: exp=-2 → zigzag=3, sig=314 → zigzag=628
        let v = Value::Decimal { sig: 314, exp: -2 };
        let enc = encode(&v);
        assert_eq!(&enc[..2], "3*"); // exp zigzag(3)=3 then *
        // The rest should decode back
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn string_roundtrip() {
        for s in ["", "hello", "name", "with spaces", "unicode: 日本語"] {
            let v = Value::String(s.to_string());
            assert_eq!(roundtrip(&v), v);
        }
    }

    #[test]
    fn string_encoding() {
        assert_eq!(encode(&Value::String("hello".into())), "5,hello");
        assert_eq!(encode(&Value::String("".into())), ",");
        assert_eq!(encode(&Value::String("name".into())), "4,name");
    }

    #[test]
    fn ref_roundtrip() {
        for name in ["t", "f", "n", "no", "nan", "inf", "nif"] {
            let v = Value::Ref(name.into());
            assert_eq!(roundtrip(&v), v);
        }
    }

    #[test]
    fn ref_encoding() {
        assert_eq!(encode(&Value::Ref("t".into())), "t'");
        assert_eq!(encode(&Value::Ref("n".into())), "n'");
    }

    #[test]
    fn variable_roundtrip() {
        for name in ["x", "my-var", "max", "trace-id", "request-id"] {
            let v = Value::Variable(name.into());
            assert_eq!(roundtrip(&v), v);
        }
    }

    #[test]
    fn opcode_roundtrip() {
        for name in ["ad", "sb", "ml", "dv", "eq", "lt", "gt", "rn"] {
            let v = Value::Opcode(name.into());
            assert_eq!(roundtrip(&v), v);
        }
    }

    #[test]
    fn self_ref_roundtrip() {
        for depth in [0, 1, 2, 10] {
            let v = Value::SelfRef(depth);
            assert_eq!(roundtrip(&v), v);
        }
    }

    #[test]
    fn break_cont_roundtrip() {
        // 0 = break depth 1, 1 = continue depth 1, 2 = break depth 2
        for v in [0, 1, 2, 3] {
            let val = Value::BreakCont(v);
            assert_eq!(roundtrip(&val), val);
        }
    }

    #[test]
    fn pointer_raw_roundtrip() {
        // decode_raw preserves pointers
        for delta in [0, 1, 5, 100] {
            let v = Value::Pointer(delta);
            let encoded = encode(&v);
            let decoded = decode_raw(&encoded).unwrap();
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn pointer_resolution() {
        // Encode two identical strings — second becomes a pointer.
        // decode() should resolve the pointer back to the string.
        let long_str = Value::String("this-is-a-long-repeated-string".into());
        let v = Value::Array(vec![long_str.clone(), long_str.clone()]);
        let deduped = encode_dedup(&v);
        assert!(deduped.contains('^'), "expected pointer in deduped output");

        let decoded = decode(&deduped).unwrap();
        // Both elements should be the resolved string, not a Pointer
        if let Value::Array(items) = &decoded {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::String("this-is-a-long-repeated-string".into()));
            assert_eq!(items[1], Value::String("this-is-a-long-repeated-string".into()));
        } else {
            panic!("expected Array, got {decoded:?}");
        }
    }

    // ── Container round-trips ───────────────────────────────────────

    #[test]
    fn array_roundtrip() {
        let v = Value::Array(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn array_encoding() {
        // [1, 2, 3] → [2+4+6+]
        let v = Value::Array(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]);
        assert_eq!(encode(&v), "[2+4+6+]");
    }

    #[test]
    fn object_roundtrip() {
        let v = Value::Object(vec![
            (Value::String("name".into()), Value::String("Ada".into())),
            (Value::String("score".into()), Value::Integer(95)),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn block_roundtrip() {
        let v = Value::Block(vec![
            Value::Set(
                Box::new(Value::Variable("x".into())),
                Box::new(Value::Integer(1)),
            ),
            Value::Variable("x".into()),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn call_roundtrip() {
        // add(1, 2)
        let v = Value::Call(vec![
            Value::Opcode("ad".into()),
            Value::Integer(1),
            Value::Integer(2),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn call_encoding() {
        // add(1, 2) → (ad%2+4+)
        let v = Value::Call(vec![
            Value::Opcode("ad".into()),
            Value::Integer(1),
            Value::Integer(2),
        ]);
        assert_eq!(encode(&v), "(ad%2+4+)");
    }

    #[test]
    fn empty_containers() {
        assert_eq!(roundtrip(&Value::Array(vec![])), Value::Array(vec![]));
        assert_eq!(roundtrip(&Value::Object(vec![])), Value::Object(vec![]));
        assert_eq!(roundtrip(&Value::Call(vec![])), Value::Call(vec![]));
    }

    // ── Compound round-trips ────────────────────────────────────────

    #[test]
    fn when_roundtrip() {
        // when x do y end
        let v = Value::When(vec![
            Value::Variable("x".into()),
            Value::Variable("y".into()),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn when_else_roundtrip() {
        // when x do y else z end
        let v = Value::When(vec![
            Value::Variable("x".into()),
            Value::Variable("y".into()),
            Value::Variable("z".into()),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn unless_roundtrip() {
        let v = Value::Unless(vec![
            Value::Variable("x".into()),
            Value::Variable("y".into()),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn or_and_roundtrip() {
        let or_v = Value::Or(vec![
            Value::Variable("a".into()),
            Value::Integer(100),
        ]);
        assert_eq!(roundtrip(&or_v), or_v);

        let and_v = Value::And(vec![
            Value::Variable("a".into()),
            Value::Variable("b".into()),
        ]);
        assert_eq!(roundtrip(&and_v), and_v);
    }

    #[test]
    fn for_in_roundtrip() {
        // for x in items do add(x, 1) end
        let v = Value::ForIn(vec![
            Value::Variable("items".into()),
            Value::Variable("x".into()),
            Value::Call(vec![
                Value::Opcode("ad".into()),
                Value::Variable("x".into()),
                Value::Integer(1),
            ]),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn while_roundtrip() {
        let v = Value::While(vec![
            Value::Variable("cond".into()),
            Value::Variable("body".into()),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn list_comp_in_roundtrip() {
        // [self * self in items]
        let v = Value::ListCompIn(vec![
            Value::Variable("items".into()),
            Value::Call(vec![
                Value::Opcode("ml".into()),
                Value::SelfRef(0),
                Value::SelfRef(0),
            ]),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn map_comp_in_roundtrip() {
        let v = Value::MapCompIn(vec![
            Value::Variable("users".into()),
            Value::Variable("u".into()),
            Value::Variable("key".into()),
            Value::Variable("val".into()),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    // ── Mutation round-trips ────────────────────────────────────────

    #[test]
    fn set_roundtrip() {
        let v = Value::Set(
            Box::new(Value::Variable("x".into())),
            Box::new(Value::Integer(42)),
        );
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn set_encoding() {
        // x = 42 → =x$1k+
        let v = Value::Set(
            Box::new(Value::Variable("x".into())),
            Box::new(Value::Integer(42)),
        );
        assert_eq!(encode(&v), "=x$1k+");
    }

    #[test]
    fn swap_roundtrip() {
        let v = Value::Swap(
            Box::new(Value::Variable("x".into())),
            Box::new(Value::Integer(1)),
        );
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn delete_roundtrip() {
        let v = Value::Delete(Box::new(Value::Variable("x".into())));
        assert_eq!(roundtrip(&v), v);
    }

    // ── Nested / complex round-trips ────────────────────────────────

    #[test]
    fn nested_navigation() {
        // user.name → call(var "user", string "name")
        let v = Value::Call(vec![
            Value::Variable("user".into()),
            Value::String("name".into()),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn fibonacci_program() {
        // max = max or 100
        // fibs = []
        // ...simplified
        let v = Value::Block(vec![
            Value::Set(
                Box::new(Value::Variable("max".into())),
                Box::new(Value::Or(vec![
                    Value::Variable("max".into()),
                    Value::Integer(100),
                ])),
            ),
            Value::Set(
                Box::new(Value::Variable("fibs".into())),
                Box::new(Value::Array(vec![])),
            ),
            Value::Set(
                Box::new(Value::Variable("a".into())),
                Box::new(Value::Integer(1)),
            ),
            Value::While(vec![
                Value::Call(vec![
                    Value::Opcode("le".into()),
                    Value::Variable("a".into()),
                    Value::Variable("max".into()),
                ]),
                Value::Block(vec![
                    Value::Set(
                        Box::new(Value::Variable("c".into())),
                        Box::new(Value::Call(vec![
                            Value::Opcode("ad".into()),
                            Value::Variable("a".into()),
                            Value::Variable("b".into()),
                        ])),
                    ),
                ]),
            ]),
            Value::Variable("fibs".into()),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn when_with_nested_call() {
        // when gt(x, 10) do add(x, 1) end
        let v = Value::When(vec![
            Value::Call(vec![
                Value::Opcode("gt".into()),
                Value::Variable("x".into()),
                Value::Integer(10),
            ]),
            Value::Call(vec![
                Value::Opcode("ad".into()),
                Value::Variable("x".into()),
                Value::Integer(1),
            ]),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn json_object_encoding() {
        // {"name": "Ada", "score": 95} as object
        let v = Value::Object(vec![
            (Value::String("name".into()), Value::String("Ada".into())),
            (Value::String("score".into()), Value::Integer(95)),
        ]);
        let encoded = encode(&v);
        let decoded = roundtrip(&v);
        assert_eq!(decoded, v);
        // Verify the encoded uses paired {} delimiters
        assert!(encoded.starts_with('{') && encoded.ends_with('}'), "object should use {{}} delimiters");
    }

    // ── Dedup tests ─────────────────────────────────────────────────

    #[test]
    fn dedup_no_effect_on_unique_values() {
        let v = Value::Array(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]);
        assert_eq!(encode(&v), encode_dedup(&v));
    }

    #[test]
    fn dedup_replaces_repeated_large_strings() {
        // Two identical long strings — second should be a pointer
        let long_str = Value::String("this-is-a-long-repeated-string".into());
        let v = Value::Array(vec![long_str.clone(), long_str.clone()]);
        let normal = encode(&v);
        let deduped = encode_dedup(&v);
        assert!(
            deduped.len() < normal.len(),
            "dedup should produce smaller output: {} vs {} bytes",
            deduped.len(),
            normal.len()
        );
        assert!(deduped.contains('^'), "deduped output should contain a pointer");
    }

    #[test]
    fn dedup_skips_small_values() {
        // Small values (< 4 bytes) should not be deduped
        let v = Value::Array(vec![
            Value::Integer(1),
            Value::Integer(1),
            Value::Integer(1),
        ]);
        let normal = encode(&v);
        let deduped = encode_dedup(&v);
        assert_eq!(normal, deduped, "small values should not be deduped");
    }

    #[test]
    fn dedup_repeated_objects() {
        let obj = Value::Object(vec![
            (Value::String("name".into()), Value::String("Ada".into())),
            (Value::String("score".into()), Value::Integer(95)),
        ]);
        let v = Value::Array(vec![obj.clone(), obj.clone(), obj.clone()]);
        let normal = encode(&v);
        let deduped = encode_dedup(&v);
        assert!(
            deduped.len() < normal.len(),
            "dedup should shrink repeated objects: {} vs {} bytes",
            deduped.len(),
            normal.len()
        );
    }

    #[test]
    fn dedup_repeated_arrays() {
        let arr = Value::Array(vec![
            Value::String("alpha".into()),
            Value::String("beta".into()),
            Value::String("gamma".into()),
        ]);
        let v = Value::Array(vec![arr.clone(), arr.clone()]);
        let normal = encode(&v);
        let deduped = encode_dedup(&v);
        assert!(
            deduped.len() < normal.len(),
            "dedup should shrink repeated arrays: {} vs {} bytes",
            deduped.len(),
            normal.len()
        );
    }

    #[test]
    fn dedup_no_cross_branch_pointers() {
        // Duplicate values in different conditional branches must not be
        // deduplicated — the pointer target may be in a skipped branch.
        let shared = Value::String("shared-value".into());
        let v = Value::Block(vec![
            Value::Unless(vec![
                Value::Variable("x".into()),
                shared.clone(),
            ]),
            Value::When(vec![
                Value::Variable("x".into()),
                shared.clone(),
            ]),
        ]);
        let deduped = encode_dedup(&v);
        // Both branches should contain the full string, not a pointer
        let decoded = decode(&deduped).unwrap();
        if let Value::Block(items) = &decoded {
            if let Value::Unless(u_items) = &items[0] {
                assert_eq!(u_items[1], Value::String("shared-value".into()));
            }
            if let Value::When(w_items) = &items[1] {
                assert_eq!(w_items[1], Value::String("shared-value".into()));
            }
        } else {
            panic!("expected Block");
        }
    }

    #[test]
    fn return_roundtrip() {
        let v = Value::Return(Box::new(Value::Integer(42)));
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn return_encoding() {
        // Return has no size prefix by default: just ;child
        let v = Value::Return(Box::new(Value::Integer(42)));
        assert_eq!(encode(&v), ";1k+");
    }

    #[test]
    fn bare_return_roundtrip() {
        let v = Value::Return(Box::new(Value::Ref("no".into())));
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn varint_len_correctness() {
        assert_eq!(varint_len(0), 0);
        assert_eq!(varint_len(1), 1);
        assert_eq!(varint_len(63), 1);
        assert_eq!(varint_len(64), 2);
        assert_eq!(varint_len(4095), 2);
        assert_eq!(varint_len(4096), 3);
    }

    // ── Length prefix tests ────────────────────────────────────────────

    #[test]
    fn conditional_block_branch_is_length_prefixed() {
        // when x do {1} end → ?(x$ 2{2+})
        let v = Value::When(vec![
            Value::Variable("x".into()),
            Value::Block(vec![Value::Integer(1)]),
        ]);
        let encoded = encode(&v);
        assert_eq!(encoded, "?(x$2{2+})");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, v);
    }

    #[test]
    fn conditional_both_branches_length_prefixed() {
        // when x do {1, 2} else {3} end
        let v = Value::When(vec![
            Value::Variable("x".into()),
            Value::Block(vec![Value::Integer(1), Value::Integer(2)]),
            Value::Block(vec![Value::Integer(3)]),
        ]);
        let encoded = encode(&v);
        assert_eq!(encoded, "?(x$4{2+4+}2{6+})");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, v);
    }

    #[test]
    fn or_branch_length_prefixed() {
        let v = Value::Or(vec![
            Value::Variable("a".into()),
            Value::Block(vec![Value::Integer(1), Value::Integer(2)]),
        ]);
        let encoded = encode(&v);
        assert_eq!(encoded, "|(a$4{2+4+})");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, v);
    }

    #[test]
    fn scalar_branch_not_prefixed() {
        // Scalar branches should NOT be length-prefixed
        let v = Value::When(vec![
            Value::Variable("x".into()),
            Value::Integer(42),
            Value::Integer(99),
        ]);
        let encoded = encode(&v);
        // No length prefix before integers
        assert_eq!(encoded, "?(x$1k+36+)");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, v);
    }

    #[test]
    fn non_conditional_compound_not_prefixed() {
        // for-in is not conditional, no length prefix
        let v = Value::ForIn(vec![
            Value::Variable("items".into()),
            Value::Variable("x".into()),
            Value::Block(vec![Value::Variable("x".into())]),
        ]);
        let encoded = encode(&v);
        // No length prefix — '>' is not conditional
        assert_eq!(encoded, ">(items$x${x$})");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, v);
    }

    #[test]
    fn dedup_with_length_prefixed_branches() {
        let v = Value::When(vec![
            Value::Variable("x".into()),
            Value::Block(vec![Value::Integer(1), Value::Integer(2)]),
            Value::Block(vec![Value::Integer(3)]),
        ]);
        let deduped = encode_dedup(&v);
        let decoded = decode(&deduped).unwrap();
        assert_eq!(decoded, v);
    }

    #[test]
    fn return_scalar_in_branch_no_prefix() {
        // return 42 in a branch — scalar child, no prefix needed
        let v = Value::When(vec![
            Value::Variable("x".into()),
            Value::Return(Box::new(Value::Integer(42))),
        ]);
        let encoded = encode(&v);
        assert_eq!(encoded, "?(x$;1k+)");
        assert_eq!(decode(&encoded).unwrap(), v);
    }

    #[test]
    fn return_container_in_branch_child_gets_prefix() {
        // return [1] in a branch — container child gets length prefix
        let v = Value::When(vec![
            Value::Variable("x".into()),
            Value::Return(Box::new(Value::Array(vec![Value::Integer(1)]))),
        ]);
        let encoded = encode(&v);
        // ;2[2+] — return passes skip to child, child [2+] gets prefix 2
        assert_eq!(encoded, "?(x$;2[2+])");
        assert_eq!(decode(&encoded).unwrap(), v);
    }

    // ── Indexed container tests ────────────────────────────────────────

    #[test]
    fn indexed_array_single_element() {
        let items = vec![Value::Integer(1)];
        let encoded = encode_indexed_array(&items);
        // count=1, width=1, packed=(1<<3)|0=8 → "8"
        // ptr0=0 → "0", element=2+ → "8#02+"
        assert_eq!(encoded, "[8#02+]");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, Value::Array(items));
    }

    #[test]
    fn indexed_array_two_elements() {
        let items = vec![Value::Integer(1), Value::Integer(2)];
        let encoded = encode_indexed_array(&items);
        // count=2, width=1, packed=(2<<3)|0=16 → "g" (b64 val 16)
        // ptr0=0 → "0", ptr1=2 → "2", elements=2+4+
        assert_eq!(encoded, "[g#022+4+]");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, Value::Array(items));
    }

    #[test]
    fn indexed_array_empty() {
        let items: Vec<Value> = vec![];
        let encoded = encode_indexed_array(&items);
        assert_eq!(encoded, "[]");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, Value::Array(items));
    }

    #[test]
    fn indexed_array_with_strings() {
        let items = vec![
            Value::String("hello".into()),
            Value::String("world".into()),
        ];
        let encoded = encode_indexed_array(&items);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, Value::Array(items));
    }

    #[test]
    fn indexed_array_large_offsets() {
        // Elements with enough size to need width > 1
        let long = "a]b".repeat(30); // 90 chars → offset > 63 → width 2
        let items = vec![
            Value::String(long.clone()),
            Value::Integer(42),
        ];
        let encoded = encode_indexed_array(&items);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, Value::Array(items));
        // Verify width is 2 (offset 92 needs 2 b64 digits)
        // packed = (2<<3) | (2-1) = 17 → varint "h"
        assert!(encoded.starts_with("[h#"));
    }

    #[test]
    fn indexed_array_nested() {
        let items = vec![
            Value::Array(vec![Value::Integer(1), Value::Integer(2)]),
            Value::Array(vec![Value::Integer(3)]),
        ];
        let encoded = encode_indexed_array(&items);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, Value::Array(items));
    }

    #[test]
    fn b64_width_values() {
        assert_eq!(b64_width(0), 1);
        assert_eq!(b64_width(1), 1);
        assert_eq!(b64_width(63), 1);
        assert_eq!(b64_width(64), 2);
        assert_eq!(b64_width(4095), 2);
        assert_eq!(b64_width(4096), 3);
    }

    #[test]
    fn indexed_object_roundtrip() {
        let pairs = vec![
            (Value::String("name".into()), Value::String("Ada".into())),
            (Value::String("score".into()), Value::Integer(95)),
        ];
        let encoded = encode_indexed_object(&pairs);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, Value::Object(pairs));
    }

    #[test]
    fn indexed_object_empty() {
        let pairs: Vec<(Value, Value)> = vec![];
        let encoded = encode_indexed_object(&pairs);
        assert_eq!(encoded, "{}");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, Value::Object(pairs));
    }

    #[test]
    fn indexed_object_pointers_sorted_by_key() {
        // Keys "b" and "a" — pointers should be sorted so "a" comes first in index
        let pairs = vec![
            (Value::String("b".into()), Value::Integer(2)),
            (Value::String("a".into()), Value::Integer(1)),
        ];
        let encoded = encode_indexed_object(&pairs);
        let decoded = decode(&encoded).unwrap();
        // Decoded in original order (body order), not sorted
        assert_eq!(decoded, Value::Object(pairs));
    }

    #[test]
    fn indexed_object_single_entry() {
        let pairs = vec![
            (Value::String("key".into()), Value::Integer(42)),
        ];
        let encoded = encode_indexed_object(&pairs);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, Value::Object(pairs));
    }
}

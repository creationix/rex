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

/// Decode a varint from `input` starting at `pos`. Consumes all leading
/// b64 digits. Returns 0 for an empty varint.
fn decode_varint(input: &[u8], pos: &mut usize) -> u64 {
    let mut n: u64 = 0;
    while *pos < input.len() {
        if let Some(v) = b64_val(input[*pos]) {
            n = n * 64 + v as u64;
            *pos += 1;
        } else {
            break;
        }
    }
    n
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

    List(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Array(Vec<Value>),
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

    Set(Box<Value>, Box<Value>),
    Swap(Box<Value>, Box<Value>),
    Delete(Box<Value>),
}

// ── Encoder ─────────────────────────────────────────────────────────────

impl Value {
    fn is_scalar(&self) -> bool {
        matches!(
            self,
            Value::Integer(_)
                | Value::Decimal { .. }
                | Value::String(_)
                | Value::Ref(_)
                | Value::Variable(_)
                | Value::Opcode(_)
                | Value::SelfRef(_)
                | Value::BreakCont(_)
                | Value::Pointer(_)
        )
    }
}

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

        // Sized-body containers
        Value::List(items) => encode_sized_body(';', items, out),
        Value::Map(pairs) => {
            let mut body = String::new();
            for (k, v) in pairs {
                encode_into(k, &mut body);
                encode_into(v, &mut body);
            }
            out.push_str(&encode_varint(body.len() as u64));
            out.push(':');
            out.push_str(&body);
        }

        // Paired containers
        Value::Array(items) => encode_paired('[', ']', items, out),
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

fn encode_compound(modifier: char, open: char, close: char, items: &[Value], out: &mut String) {
    let mut body = String::new();
    for item in items {
        encode_into(item, &mut body);
    }
    out.push(modifier);
    out.push(open);
    out.push_str(&body);
    out.push(close);
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
        Value::List(items) | Value::Array(items) | Value::Block(items) | Value::Call(items)
        | Value::When(items) | Value::Unless(items) | Value::Or(items) | Value::And(items)
        | Value::ForIn(items) | Value::ForOf(items) | Value::While(items)
        | Value::ListCompIn(items) | Value::ListCompOf(items) | Value::ListCompWhile(items)
        | Value::MapCompIn(items) | Value::MapCompOf(items) | Value::MapCompWhile(items) => {
            let c: usize = 1 + items.iter().map(|i| prescan_counts(i, counts)).sum::<usize>();
            counts.insert(value as *const Value, c);
            c
        }
        Value::Map(pairs) => {
            let c: usize = 1 + pairs.iter().map(|(k, v)| prescan_counts(k, counts) + prescan_counts(v, counts)).sum::<usize>();
            counts.insert(value as *const Value, c);
            c
        }
        Value::Set(a, b) | Value::Swap(a, b) => {
            let c = 1 + prescan_counts(a, counts) + prescan_counts(b, counts);
            counts.insert(value as *const Value, c);
            c
        }
        Value::Delete(a) => {
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
    seen: std::collections::HashMap<u64, (usize, usize)>,
    /// schema hash → (rev_start, len) of the first object with that key layout
    schemas: std::collections::HashMap<u64, (usize, usize)>,
    /// known string prefixes (delimiter-split) for chain dedup
    prefixes: std::collections::HashSet<String>,
    node_counts: std::collections::HashMap<*const Value, usize>,
    value_hashes: std::collections::HashMap<*const Value, u64>,
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
            if let Some(&(target_left, target_len)) = self.seen.get(&key) {
                // In rev coords: target_left = self.pos after writing target.
                // Current self.pos = where pointer starts.
                // In fwd coords after reversal:
                //   pointer_right = total - self.pos
                //   target_left = total - target_left
                //   delta = pointer_right - target_left_fwd
                let delta = (self.pos - target_left) as u64;
                let ptr_size = varint_len(delta) + 1;
                if ptr_size < target_len {
                    self.push(b'^');
                    self.push_varint(delta);
                    return;
                }
            }
            let start = self.pos;
            self.emit(value);
            let len = self.pos - start;
            // Record left edge (self.pos after writing) and length
            self.seen.entry(key).or_insert((self.pos, len));
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
            Value::List(_) | Value::Map(_) | Value::Array(_) | Value::Block(_) | Value::Call(_) => {
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

            // Sized: body first (children in reverse), then tag+size
            Value::List(items) => {
                let before = self.pos;
                for item in items.iter().rev() { self.write(item); }
                let body_len = self.pos - before;
                self.push(b';'); self.push_varint(body_len as u64);
            }
            Value::Map(pairs) => self.emit_map(pairs),

            // Paired: closer, children reversed, opener
            Value::Array(items) => { self.push(b']'); for i in items.iter().rev() { self.write(i); } self.push(b'['); }
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

            // Mutation: reversed order
            Value::Set(p, v) => { self.write(v); self.write(p); self.push(b'='); }
            Value::Swap(p, v) => { self.write(v); self.write(p); self.push(b'/'); }
            Value::Delete(p) => { self.write(p); self.push(b'~'); }
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

            if let Some(&(target_left, target_len)) = self.seen.get(&key) {
                let delta = (self.pos - target_left) as u64;
                let ptr_size = varint_len(delta) + 1;
                if ptr_size < target_len {
                    self.push(b'^');
                    self.push_varint(delta);
                    return;
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
                        // Chain: [suffix][prefix] then `.` tag
                        let before = self.pos;
                        self.write_string(&s[offset..]);
                        self.write_string(&s[..offset]);
                        let body_len = self.pos - before;
                        self.push(b'.');
                        self.push_varint(body_len as u64);
                        self.register_chain_prefixes(s);
                        let len = self.pos - start;
                        self.seen.entry(key).or_insert((self.pos, len));
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
            self.seen.entry(key).or_insert((self.pos, len));
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

    fn emit_map(&mut self, pairs: &[(Value, Value)]) {
        if pairs.is_empty() {
            self.push(b':');
            return;
        }

        // Compute schema key from the map's keys
        let schema = Self::schema_key(pairs);

        if let Some(&(schema_left, _schema_len)) = self.schemas.get(&schema) {
            // Schema match: emit pointer to previous object, then just values.
            let before = self.pos;
            for (_k, v) in pairs.iter().rev() {
                self.write(v);
            }
            // Emit pointer to the schema object
            let delta = (self.pos - schema_left) as u64;
            self.push(b'^');
            self.push_varint(delta);

            let body_len = self.pos - before;
            self.push(b':');
            self.push_varint(body_len as u64);
        } else {
            // First time seeing this schema: encode normally and record
            let before = self.pos;
            for (k, v) in pairs.iter().rev() {
                self.write(v);
                self.write(k);
            }
            let body_len = self.pos - before;
            self.push(b':');
            self.push_varint(body_len as u64);

            // Record: left edge = self.pos (after writing the whole object)
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
        self.push(close);
        for item in items.iter().rev() { self.write(item); }
        self.push(open);
        self.push(modifier);
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

        // Sized-body containers
        b';' => {
            let size = varint_from_raw(varint_raw) as usize;
            let end = *pos + size;
            let mut items = Vec::new();
            while *pos < end {
                items.push(read_value(input, pos, resolve)?);
            }
            Ok(Value::List(items))
        }
        b':' => {
            let size = varint_from_raw(varint_raw) as usize;
            let end = *pos + size;
            if *pos >= end {
                return Ok(Value::Map(vec![]));
            }
            // Peek at first value to determine if this is key-value pairs
            // or a schema-shared object (pointer/object + values).
            let first = read_value(input, pos, resolve)?;
            match &first {
                Value::String(_) => {
                    // Normal key-value pairs: first is a key
                    let mut pairs = Vec::new();
                    let val = read_value(input, pos, resolve)?;
                    pairs.push((first, val));
                    while *pos < end {
                        let key = read_value(input, pos, resolve)?;
                        let val = read_value(input, pos, resolve)?;
                        pairs.push((key, val));
                    }
                    Ok(Value::Map(pairs))
                }
                Value::Map(schema_pairs) => {
                    // Schema-shared: first value resolved to a map.
                    // Use its keys, read remaining values from body.
                    let mut pairs = Vec::new();
                    for (schema_key, _) in schema_pairs {
                        let val = read_value(input, pos, resolve)?;
                        pairs.push((schema_key.clone(), val));
                    }
                    Ok(Value::Map(pairs))
                }
                _ => {
                    // Fallback: treat as key-value pairs
                    let mut pairs = Vec::new();
                    let val = read_value(input, pos, resolve)?;
                    pairs.push((first, val));
                    while *pos < end {
                        let key = read_value(input, pos, resolve)?;
                        let val = read_value(input, pos, resolve)?;
                        pairs.push((key, val));
                    }
                    Ok(Value::Map(pairs))
                }
            }
        }

        // Paired containers (with optional size prefix)
        b'(' => decode_paired_body(input, pos, b')', resolve, |items| Value::Call(items)),
        b'[' => decode_paired_body(input, pos, b']', resolve, |items| Value::Array(items)),
        b'{' => decode_paired_body(input, pos, b'}', resolve, |items| Value::Block(items)),

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
                // Raw mode: preserve chain structure as a Call with opcode "chain"
                let mut items = vec![Value::Opcode("chain".into())];
                items.extend(segments);
                Ok(Value::Call(items))
            }
        }

        _ => Err(DecodeError {
            pos: *pos - 1,
            message: format!("unexpected tag byte: {:?}", tag as char),
        }),
    }
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
        assert_eq!(decode_varint(b"1k+", &mut pos), 84);
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
        for name in ["t", "f", "n", "u", "nan", "inf", "nif"] {
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
        let v = Value::List(vec![long_str.clone(), long_str.clone()]);
        let deduped = encode_dedup(&v);
        assert!(deduped.contains('^'), "expected pointer in deduped output");

        let decoded = decode(&deduped).unwrap();
        // Both elements should be the resolved string, not a Pointer
        if let Value::List(items) = &decoded {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::String("this-is-a-long-repeated-string".into()));
            assert_eq!(items[1], Value::String("this-is-a-long-repeated-string".into()));
        } else {
            panic!("expected List, got {decoded:?}");
        }
    }

    // ── Container round-trips ───────────────────────────────────────

    #[test]
    fn list_roundtrip() {
        let v = Value::List(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn list_encoding() {
        // [1, 2, 3] → 6;2+4+6+
        let v = Value::List(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]);
        assert_eq!(encode(&v), "6;2+4+6+");
    }

    #[test]
    fn map_roundtrip() {
        let v = Value::Map(vec![
            (Value::String("name".into()), Value::String("Ada".into())),
            (Value::String("score".into()), Value::Integer(95)),
        ]);
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn array_roundtrip() {
        let v = Value::Array(vec![
            Value::Integer(1),
            Value::Integer(2),
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
        assert_eq!(roundtrip(&Value::List(vec![])), Value::List(vec![]));
        assert_eq!(roundtrip(&Value::Map(vec![])), Value::Map(vec![]));
        assert_eq!(roundtrip(&Value::Array(vec![])), Value::Array(vec![]));
        assert_eq!(roundtrip(&Value::Block(vec![])), Value::Block(vec![]));
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
        // {"name": "Ada", "score": 95} as lazy map
        let v = Value::Map(vec![
            (Value::String("name".into()), Value::String("Ada".into())),
            (Value::String("score".into()), Value::Integer(95)),
        ]);
        let encoded = encode(&v);
        let decoded = roundtrip(&v);
        assert_eq!(decoded, v);
        // Verify the encoded starts with size + ':'
        assert!(encoded.contains(':'), "map should contain ':' tag");
    }

    // ── Dedup tests ─────────────────────────────────────────────────

    #[test]
    fn dedup_no_effect_on_unique_values() {
        let v = Value::List(vec![
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
        let v = Value::List(vec![long_str.clone(), long_str.clone()]);
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
        let v = Value::List(vec![
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
        let obj = Value::Map(vec![
            (Value::String("name".into()), Value::String("Ada".into())),
            (Value::String("score".into()), Value::Integer(95)),
        ]);
        let v = Value::List(vec![obj.clone(), obj.clone(), obj.clone()]);
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
        let arr = Value::List(vec![
            Value::String("alpha".into()),
            Value::String("beta".into()),
            Value::String("gamma".into()),
        ]);
        let v = Value::List(vec![arr.clone(), arr.clone()]);
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
    fn varint_len_correctness() {
        assert_eq!(varint_len(0), 0);
        assert_eq!(varint_len(1), 1);
        assert_eq!(varint_len(63), 1);
        assert_eq!(varint_len(64), 2);
        assert_eq!(varint_len(4095), 2);
        assert_eq!(varint_len(4096), 3);
    }
}

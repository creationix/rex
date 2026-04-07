//! Arena-based runtime values with handle semantics.
//!
//! `Value` is a tagged u64 that either inlines small values or holds a handle
//! into the `Heap`. All compound data (strings, arrays, objects) lives on the
//! heap and is referenced by index. This gives us:
//! - Cheap copies (just a u64)
//! - Mutation through handles (all aliases see changes)
//! - Bump-style allocation (drop the whole heap when done)

use std::collections::HashMap;

// ── Value ──────────────────────────────────────────────────────────────

/// Tagged 64-bit runtime value.
///
/// Layout (low 3 bits = tag):
///   000 + discriminant  → None, Null, Bool(false), Bool(true)
///   001 + zigzag i61    → small integer
///   010 + index         → interned string
///   011 + index         → heap array
///   100 + index         → heap object
///   101 + index         → host object
///   110 + index         → heap float/decimal
///   111 + offset        → embedded bytecode value (COW)
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Value(u64);

const TAG_SPECIAL: u64 = 0b000;
const TAG_INT: u64     = 0b001;
const TAG_STRING: u64  = 0b010;
const TAG_ARRAY: u64   = 0b011;
const TAG_OBJECT: u64  = 0b100;
const TAG_HOST: u64    = 0b101;
const TAG_FLOAT: u64   = 0b110;
const TAG_EMBEDDED: u64 = 0b111;

// Special discriminants (shifted left by 3)
const SPECIAL_NONE:  u64 = 0 << 3;
const SPECIAL_NULL:  u64 = 1 << 3;
const SPECIAL_FALSE: u64 = 2 << 3;
const SPECIAL_TRUE:  u64 = 3 << 3;

impl Value {
    // ── Constructors ───────────────────────────────────────────────

    pub const NONE: Value = Value(TAG_SPECIAL | SPECIAL_NONE);
    pub const NULL: Value = Value(TAG_SPECIAL | SPECIAL_NULL);
    pub const FALSE: Value = Value(TAG_SPECIAL | SPECIAL_FALSE);
    pub const TRUE: Value = Value(TAG_SPECIAL | SPECIAL_TRUE);

    #[inline]
    pub fn bool(b: bool) -> Value {
        if b { Self::TRUE } else { Self::FALSE }
    }

    #[inline]
    pub fn int(n: i64) -> Value {
        // Zigzag encode to fit in 61 bits unsigned
        let z = if n >= 0 { (n as u64) << 1 } else { ((-n as u64) << 1) - 1 };
        Value((z << 3) | TAG_INT)
    }

    #[inline]
    pub fn string(id: u32) -> Value {
        Value(((id as u64) << 3) | TAG_STRING)
    }

    #[inline]
    pub fn array(id: u32) -> Value {
        Value(((id as u64) << 3) | TAG_ARRAY)
    }

    #[inline]
    pub fn object(id: u32) -> Value {
        Value(((id as u64) << 3) | TAG_OBJECT)
    }

    #[inline]
    pub fn host(id: u32) -> Value {
        Value(((id as u64) << 3) | TAG_HOST)
    }

    #[inline]
    pub fn float(id: u32) -> Value {
        Value(((id as u64) << 3) | TAG_FLOAT)
    }

    #[inline]
    pub fn embedded(offset: u32) -> Value {
        Value(((offset as u64) << 3) | TAG_EMBEDDED)
    }

    // ── Tag queries ────────────────────────────────────────────────

    #[inline]
    fn tag(self) -> u64 { self.0 & 0b111 }

    #[inline]
    fn payload(self) -> u64 { self.0 >> 3 }

    #[inline]
    pub fn is_none(self) -> bool { self.0 == Self::NONE.0 }

    #[inline]
    pub fn is_null(self) -> bool { self.0 == Self::NULL.0 }

    #[inline]
    pub fn is_defined(self) -> bool { !self.is_none() }

    #[inline]
    pub fn is_string(self) -> bool { self.tag() == TAG_STRING }

    #[inline]
    pub fn is_array(self) -> bool { self.tag() == TAG_ARRAY }

    #[inline]
    pub fn is_object(self) -> bool { self.tag() == TAG_OBJECT }

    #[inline]
    pub fn is_host(self) -> bool { self.tag() == TAG_HOST }

    #[inline]
    pub fn is_embedded(self) -> bool { self.tag() == TAG_EMBEDDED }

    // ── Inline extraction ──────────────────────────────────────────

    #[inline]
    pub fn as_bool(self) -> Option<bool> {
        if self.0 == Self::TRUE.0 { Some(true) }
        else if self.0 == Self::FALSE.0 { Some(false) }
        else { None }
    }

    #[inline]
    pub fn as_i64(self) -> Option<i64> {
        if self.tag() != TAG_INT { return None; }
        let z = self.payload();
        Some(if z & 1 == 0 { (z >> 1) as i64 } else { -(((z >> 1) + 1) as i64) })
    }

    #[inline]
    pub fn string_id(self) -> Option<u32> {
        if self.tag() == TAG_STRING { Some(self.payload() as u32) } else { None }
    }

    #[inline]
    pub fn array_id(self) -> Option<u32> {
        if self.tag() == TAG_ARRAY { Some(self.payload() as u32) } else { None }
    }

    #[inline]
    pub fn object_id(self) -> Option<u32> {
        if self.tag() == TAG_OBJECT { Some(self.payload() as u32) } else { None }
    }

    #[inline]
    pub fn host_id(self) -> Option<u32> {
        if self.tag() == TAG_HOST { Some(self.payload() as u32) } else { None }
    }

    #[inline]
    pub fn float_id(self) -> Option<u32> {
        if self.tag() == TAG_FLOAT { Some(self.payload() as u32) } else { None }
    }

    #[inline]
    pub fn embedded_offset(self) -> Option<u32> {
        if self.tag() == TAG_EMBEDDED { Some(self.payload() as u32) } else { None }
    }

    // ── Heap-dependent accessors ───────────────────────────────────

    pub fn as_f64(self, heap: &Heap) -> Option<f64> {
        if let Some(n) = self.as_i64() {
            Some(n as f64)
        } else if let Some(id) = self.float_id() {
            Some(heap.floats[id as usize].to_f64())
        } else {
            None
        }
    }

    pub fn to_i64(self, heap: &Heap) -> Option<i64> {
        if let Some(n) = self.as_i64() {
            Some(n)
        } else if let Some(id) = self.float_id() {
            match &heap.floats[id as usize] {
                FloatValue::Float(f) if f.fract() == 0.0 => Some(*f as i64),
                _ => None,
            }
        } else {
            None
        }
    }

    pub fn as_str<'a>(self, heap: &'a Heap) -> Option<&'a str> {
        self.string_id().map(|id| heap.strings[id as usize].as_str())
    }

    pub fn type_name(self, heap: &Heap) -> &'static str {
        match self.tag() {
            TAG_SPECIAL => {
                if self.is_none() { "none" }
                else if self.is_null() { "null" }
                else { "boolean" }
            }
            TAG_INT | TAG_FLOAT => "number",
            TAG_STRING => "string",
            TAG_ARRAY => "array",
            TAG_OBJECT | TAG_HOST => "object",
            TAG_EMBEDDED => {
                // Peek at the embedded value to determine type
                let _ = heap; // embedded type detection needs bytecode, not heap
                "object" // default for now — caller should resolve
            }
            _ => "unknown",
        }
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_none() { return write!(f, "None"); }
        if self.is_null() { return write!(f, "Null"); }
        if let Some(b) = self.as_bool() { return write!(f, "Bool({b})"); }
        if let Some(n) = self.as_i64() { return write!(f, "Int({n})"); }
        if let Some(id) = self.string_id() { return write!(f, "String({id})"); }
        if let Some(id) = self.array_id() { return write!(f, "Array({id})"); }
        if let Some(id) = self.object_id() { return write!(f, "Object({id})"); }
        if let Some(id) = self.host_id() { return write!(f, "Host({id})"); }
        if let Some(id) = self.float_id() { return write!(f, "Float({id})"); }
        if let Some(off) = self.embedded_offset() { return write!(f, "Embedded({off})"); }
        write!(f, "Value(0x{:016x})", self.0)
    }
}

// ── Float storage ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum FloatValue {
    Float(f64),
    Decimal { sig: i64, exp: i64 },
    Blob(usize), // index into Heap::blobs
}

impl FloatValue {
    pub fn to_f64(&self) -> f64 {
        match self {
            FloatValue::Float(f) => *f,
            FloatValue::Decimal { sig, exp } => *sig as f64 * 10f64.powi(*exp as i32),
            FloatValue::Blob(_) => f64::NAN,
        }
    }

    pub fn is_blob(&self) -> bool {
        matches!(self, FloatValue::Blob(_))
    }
}

// ── Heap ───────────────────────────────────────────────────────────────

pub struct Heap {
    // String interning
    pub strings: Vec<String>,
    string_index: HashMap<String, u32>,

    // Mutable heap storage
    pub arrays: Vec<Vec<Value>>,
    pub objects: Vec<Vec<(u32, Value)>>,  // key is StringId, insertion order

    // Float/decimal storage
    pub floats: Vec<FloatValue>,

    // Blob storage (opaque byte arrays)
    pub blobs: Vec<Vec<u8>>,

    // COW: bytecode offset → promoted heap Value
    pub cow: HashMap<u32, Value>,
}

impl Heap {
    pub fn new() -> Self {
        Self {
            strings: Vec::new(),
            string_index: HashMap::new(),
            arrays: Vec::new(),
            objects: Vec::new(),
            floats: Vec::new(),
            blobs: Vec::new(),
            cow: HashMap::new(),
        }
    }

    // ── String interning ───────────────────────────────────────────

    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.string_index.get(s) {
            return id;
        }
        let id = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.string_index.insert(s.to_string(), id);
        id
    }

    pub fn intern_value(&mut self, s: &str) -> Value {
        Value::string(self.intern(s))
    }

    pub fn resolve_str(&self, id: u32) -> &str {
        &self.strings[id as usize]
    }

    /// Get the StringId for a Value that is already a string, or intern its
    /// string representation. Returns None for values that can't be used as keys.
    pub fn value_to_key(&mut self, v: Value) -> u32 {
        if let Some(id) = v.string_id() {
            return id;
        }
        if let Some(n) = v.as_i64() {
            return self.intern(&n.to_string());
        }
        self.intern(&format!("{v:?}"))
    }

    // ── Array allocation ───────────────────────────────────────────

    pub fn alloc_array(&mut self, items: Vec<Value>) -> Value {
        let id = self.arrays.len() as u32;
        self.arrays.push(items);
        Value::array(id)
    }

    // ── Object allocation ──────────────────────────────────────────

    pub fn alloc_object(&mut self, pairs: Vec<(u32, Value)>) -> Value {
        let id = self.objects.len() as u32;
        self.objects.push(pairs);
        Value::object(id)
    }

    // ── Float allocation ───────────────────────────────────────────

    pub fn alloc_float(&mut self, f: f64) -> Value {
        let id = self.floats.len() as u32;
        self.floats.push(FloatValue::Float(f));
        Value::float(id)
    }

    pub fn alloc_decimal(&mut self, sig: i64, exp: i64) -> Value {
        let id = self.floats.len() as u32;
        self.floats.push(FloatValue::Decimal { sig, exp });
        Value::float(id)
    }

    // ── Blob allocation ───────────────────────────────────────────

    pub fn alloc_blob(&mut self, data: Vec<u8>) -> Value {
        let blob_id = self.blobs.len();
        self.blobs.push(data);
        let id = self.floats.len() as u32;
        self.floats.push(FloatValue::Blob(blob_id));
        Value::float(id)
    }

    pub fn blob_data(&self, v: Value) -> Option<&[u8]> {
        let id = v.float_id()?;
        match &self.floats[id as usize] {
            FloatValue::Blob(blob_id) => Some(&self.blobs[*blob_id]),
            _ => None,
        }
    }

    pub fn is_blob(&self, v: Value) -> bool {
        v.float_id()
            .is_some_and(|id| self.floats[id as usize].is_blob())
    }

    // ── Mutation ───────────────────────────────────────────────────

    pub fn array_get(&self, arr: Value, idx: usize) -> Value {
        if let Some(id) = arr.array_id() {
            self.arrays[id as usize].get(idx).copied().unwrap_or(Value::NONE)
        } else {
            Value::NONE
        }
    }

    pub fn array_len(&self, arr: Value) -> usize {
        if let Some(id) = arr.array_id() {
            self.arrays[id as usize].len()
        } else {
            0
        }
    }

    pub fn array_set(&mut self, arr: Value, idx: usize, val: Value) {
        if let Some(id) = arr.array_id() {
            let vec = &mut self.arrays[id as usize];
            if idx < vec.len() {
                vec[idx] = val;
            }
        }
    }

    pub fn array_push(&mut self, arr: Value, val: Value) {
        if let Some(id) = arr.array_id() {
            self.arrays[id as usize].push(val);
        }
    }

    pub fn array_items(&self, arr: Value) -> &[Value] {
        if let Some(id) = arr.array_id() {
            &self.arrays[id as usize]
        } else {
            &[]
        }
    }

    pub fn object_get(&self, obj: Value, key: u32) -> Value {
        if let Some(id) = obj.object_id() {
            for &(k, v) in &self.objects[id as usize] {
                if k == key { return v; }
            }
        }
        Value::NONE
    }

    pub fn object_set(&mut self, obj: Value, key: u32, val: Value) {
        if let Some(id) = obj.object_id() {
            let pairs = &mut self.objects[id as usize];
            for pair in pairs.iter_mut() {
                if pair.0 == key {
                    pair.1 = val;
                    return;
                }
            }
            pairs.push((key, val));
        }
    }

    pub fn object_delete(&mut self, obj: Value, key: u32) {
        if let Some(id) = obj.object_id() {
            self.objects[id as usize].retain(|&(k, _)| k != key);
        }
    }

    pub fn object_pairs(&self, obj: Value) -> &[(u32, Value)] {
        if let Some(id) = obj.object_id() {
            &self.objects[id as usize]
        } else {
            &[]
        }
    }

    pub fn object_len(&self, obj: Value) -> usize {
        if let Some(id) = obj.object_id() {
            self.objects[id as usize].len()
        } else {
            0
        }
    }
}

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_none_null_bool() {
        assert!(Value::NONE.is_none());
        assert!(!Value::NONE.is_defined());
        assert!(Value::NULL.is_null());
        assert!(Value::NULL.is_defined());
        assert_eq!(Value::TRUE.as_bool(), Some(true));
        assert_eq!(Value::FALSE.as_bool(), Some(false));
        assert_eq!(Value::bool(true), Value::TRUE);
        assert_eq!(Value::bool(false), Value::FALSE);
    }

    #[test]
    fn value_int_roundtrip() {
        for n in [0i64, 1, -1, 42, -42, 1000000, -1000000, i32::MAX as i64, i32::MIN as i64] {
            let v = Value::int(n);
            assert_eq!(v.as_i64(), Some(n), "failed for {n}");
        }
    }

    #[test]
    fn value_int_large() {
        // i60 fits in 61-bit zigzag
        let big = (1i64 << 59) - 1;
        assert_eq!(Value::int(big).as_i64(), Some(big));
        assert_eq!(Value::int(-big).as_i64(), Some(-big));
    }

    #[test]
    fn heap_string_interning() {
        let mut heap = Heap::new();
        let id1 = heap.intern("hello");
        let id2 = heap.intern("hello");
        let id3 = heap.intern("world");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert_eq!(heap.resolve_str(id1), "hello");
        assert_eq!(heap.resolve_str(id3), "world");
    }

    #[test]
    fn heap_array_mutation() {
        let mut heap = Heap::new();
        let arr = heap.alloc_array(vec![Value::int(1), Value::int(2), Value::int(3)]);
        assert_eq!(heap.array_len(arr), 3);
        assert_eq!(heap.array_get(arr, 0).as_i64(), Some(1));

        heap.array_set(arr, 1, Value::int(20));
        assert_eq!(heap.array_get(arr, 1).as_i64(), Some(20));

        heap.array_push(arr, Value::int(4));
        assert_eq!(heap.array_len(arr), 4);
    }

    #[test]
    fn heap_object_mutation() {
        let mut heap = Heap::new();
        let k_x = heap.intern("x");
        let k_y = heap.intern("y");
        let obj = heap.alloc_object(vec![(k_x, Value::int(1))]);

        assert_eq!(heap.object_get(obj, k_x).as_i64(), Some(1));
        assert!(heap.object_get(obj, k_y).is_none());

        // Update existing key
        heap.object_set(obj, k_x, Value::int(2));
        assert_eq!(heap.object_get(obj, k_x).as_i64(), Some(2));

        // Insert new key
        heap.object_set(obj, k_y, Value::int(3));
        assert_eq!(heap.object_get(obj, k_y).as_i64(), Some(3));
        assert_eq!(heap.object_len(obj), 2);

        // Delete
        heap.object_delete(obj, k_x);
        assert_eq!(heap.object_len(obj), 1);
        assert!(heap.object_get(obj, k_x).is_none());
    }

    #[test]
    fn heap_float() {
        let mut heap = Heap::new();
        let v = heap.alloc_float(3.14);
        assert!((v.as_f64(&heap).unwrap() - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn heap_decimal() {
        let mut heap = Heap::new();
        let v = heap.alloc_decimal(314, -2);
        assert!((v.as_f64(&heap).unwrap() - 3.14).abs() < 0.001);
    }

    #[test]
    fn value_handles_are_copy() {
        let mut heap = Heap::new();
        let arr = heap.alloc_array(vec![Value::int(1)]);
        let alias = arr; // Copy, not move
        heap.array_push(arr, Value::int(2));
        // Both see the same array
        assert_eq!(heap.array_len(alias), 2);
    }
}

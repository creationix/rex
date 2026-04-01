use rex_core::heap::{Value, Heap};
use rex_core::interpret::{HostObject, RexError};

/// Request headers with case-insensitive lookup.
pub struct HeadersObject {
    headers: Vec<(String, String)>, // lowercased keys
}

impl HeadersObject {
    pub fn new(headers: Vec<(String, String)>) -> Self {
        Self { headers }
    }
}

impl HostObject for HeadersObject {
    fn get(&self, key: &str, heap: &mut Heap) -> Option<Value> {
        let lower = key.to_lowercase();
        let values: Vec<&str> = self.headers.iter()
            .filter(|(k, _)| k == &lower)
            .map(|(_, v)| v.as_str())
            .collect();
        match values.len() {
            0 => None,
            1 => Some(heap.intern_value(values[0])),
            _ => {
                let items: Vec<Value> = values.iter().map(|v| heap.intern_value(v)).collect();
                Some(heap.alloc_array(items))
            }
        }
    }

    fn get_index(&self, _index: usize, _heap: &mut Heap) -> Option<Value> { None }
    fn set(&mut self, _key: &str, _value: Value, _heap: &Heap) -> Result<(), RexError> { Ok(()) }
    fn call(&mut self, _method: &str, _args: &[Value], _heap: &mut Heap) -> Result<Value, RexError> {
        Ok(Value::NONE)
    }

    fn iter_keys(&self, heap: &mut Heap) -> Option<Vec<Value>> {
        let mut seen = Vec::new();
        for (k, _) in &self.headers {
            if !seen.contains(k) {
                seen.push(k.clone());
            }
        }
        Some(seen.into_iter().map(|k| heap.intern_value(&k)).collect())
    }

    fn iter_pairs(&self, heap: &mut Heap) -> Option<Vec<(Value, Value)>> {
        Some(self.headers.iter()
            .map(|(k, v)| (heap.intern_value(k), heap.intern_value(v)))
            .collect())
    }
}

/// Mutable response headers.
pub struct ResponseHeadersObject {
    pub headers: Vec<(String, String)>,
}

impl ResponseHeadersObject {
    pub fn new() -> Self {
        Self { headers: Vec::new() }
    }
}

impl HostObject for ResponseHeadersObject {
    fn get(&self, key: &str, heap: &mut Heap) -> Option<Value> {
        let lower = key.to_lowercase();
        self.headers.iter()
            .find(|(k, _)| k == &lower)
            .map(|(_, v)| heap.intern_value(v))
    }

    fn get_index(&self, _index: usize, _heap: &mut Heap) -> Option<Value> { None }

    fn set(&mut self, key: &str, value: Value, heap: &Heap) -> Result<(), RexError> {
        let lower = key.to_lowercase();
        let val_str = value_to_string(value, heap);
        if let Some(entry) = self.headers.iter_mut().find(|(k, _)| k == &lower) {
            entry.1 = val_str;
        } else {
            self.headers.push((lower, val_str));
        }
        Ok(())
    }

    fn call(&mut self, _method: &str, _args: &[Value], _heap: &mut Heap) -> Result<Value, RexError> {
        Ok(Value::NONE)
    }

    fn iter_keys(&self, heap: &mut Heap) -> Option<Vec<Value>> {
        Some(self.headers.iter().map(|(k, _)| heap.intern_value(k)).collect())
    }

    fn iter_pairs(&self, heap: &mut Heap) -> Option<Vec<(Value, Value)>> {
        Some(self.headers.iter()
            .map(|(k, v)| (heap.intern_value(k), heap.intern_value(v)))
            .collect())
    }
}

/// Mutable response object. `res.status` and `res.headers` are accessed via this.
pub struct ResponseObject {
    pub status: u16,
    pub headers_host_idx: usize,
}

impl ResponseObject {
    pub fn new(headers_host_idx: usize) -> Self {
        Self { status: 200, headers_host_idx }
    }
}

impl HostObject for ResponseObject {
    fn get(&self, key: &str, _heap: &mut Heap) -> Option<Value> {
        match key {
            "status" => Some(Value::int(self.status as i64)),
            "headers" => Some(Value::host(self.headers_host_idx as u32)),
            _ => None,
        }
    }

    fn get_index(&self, _index: usize, _heap: &mut Heap) -> Option<Value> { None }

    fn set(&mut self, key: &str, value: Value, heap: &Heap) -> Result<(), RexError> {
        match key {
            "status" => {
                self.status = value.to_i64(heap).unwrap_or(200) as u16;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn call(&mut self, _method: &str, _args: &[Value], _heap: &mut Heap) -> Result<Value, RexError> {
        Ok(Value::NONE)
    }
}

/// Query parameters object.
pub struct QueryObject {
    pub params: Vec<(String, Vec<String>)>,
}

impl QueryObject {
    pub fn from_query_string(qs: &str) -> Self {
        let mut map: Vec<(String, Vec<String>)> = Vec::new();
        for pair in qs.split('&') {
            if pair.is_empty() { continue; }
            let (key, val) = match pair.split_once('=') {
                Some((k, v)) => (k.to_string(), urldecode(v)),
                None => (pair.to_string(), String::new()),
            };
            if let Some(entry) = map.iter_mut().find(|(k, _)| k == &key) {
                entry.1.push(val);
            } else {
                map.push((key, vec![val]));
            }
        }
        Self { params: map }
    }
}

impl HostObject for QueryObject {
    fn get(&self, key: &str, heap: &mut Heap) -> Option<Value> {
        self.params.iter()
            .find(|(k, _)| k == key)
            .map(|(_, vals)| {
                if vals.len() == 1 {
                    heap.intern_value(&vals[0])
                } else {
                    let items: Vec<Value> = vals.iter().map(|v| heap.intern_value(v)).collect();
                    heap.alloc_array(items)
                }
            })
    }

    fn get_index(&self, _index: usize, _heap: &mut Heap) -> Option<Value> { None }
    fn set(&mut self, _key: &str, _value: Value, _heap: &Heap) -> Result<(), RexError> { Ok(()) }
    fn call(&mut self, _method: &str, _args: &[Value], _heap: &mut Heap) -> Result<Value, RexError> {
        Ok(Value::NONE)
    }

    fn iter_keys(&self, heap: &mut Heap) -> Option<Vec<Value>> {
        Some(self.params.iter().map(|(k, _)| heap.intern_value(k)).collect())
    }

    fn iter_pairs(&self, heap: &mut Heap) -> Option<Vec<(Value, Value)>> {
        Some(self.params.iter().map(|(k, vals)| {
            let v = if vals.len() == 1 {
                heap.intern_value(&vals[0])
            } else {
                let items: Vec<Value> = vals.iter().map(|v| heap.intern_value(v)).collect();
                heap.alloc_array(items)
            };
            (heap.intern_value(k), v)
        }).collect())
    }
}

/// Cookie map object.
pub struct CookieObject {
    pub cookies: Vec<(String, String)>,
}

impl CookieObject {
    pub fn from_header(header: &str) -> Self {
        let cookies = header.split(';')
            .filter_map(|pair| {
                let pair = pair.trim();
                let (k, v) = pair.split_once('=')?;
                Some((k.trim().to_string(), v.trim().to_string()))
            })
            .collect();
        Self { cookies }
    }
}

impl HostObject for CookieObject {
    fn get(&self, key: &str, heap: &mut Heap) -> Option<Value> {
        self.cookies.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| heap.intern_value(v))
    }

    fn get_index(&self, _index: usize, _heap: &mut Heap) -> Option<Value> { None }
    fn set(&mut self, _key: &str, _value: Value, _heap: &Heap) -> Result<(), RexError> { Ok(()) }
    fn call(&mut self, _method: &str, _args: &[Value], _heap: &mut Heap) -> Result<Value, RexError> {
        Ok(Value::NONE)
    }

    fn iter_keys(&self, heap: &mut Heap) -> Option<Vec<Value>> {
        Some(self.cookies.iter().map(|(k, _)| heap.intern_value(k)).collect())
    }

    fn iter_pairs(&self, heap: &mut Heap) -> Option<Vec<(Value, Value)>> {
        Some(self.cookies.iter()
            .map(|(k, v)| (heap.intern_value(k), heap.intern_value(v)))
            .collect())
    }
}

/// Namespace object that maps method names to opcode strings.
pub struct OpcodeNamespace {
    pub methods: Vec<(&'static str, &'static str)>,
    pub tag_opcode: Option<&'static str>,
}

impl HostObject for OpcodeNamespace {
    fn get(&self, key: &str, heap: &mut Heap) -> Option<Value> {
        self.methods.iter()
            .find(|(name, _)| *name == key)
            .map(|(_, opcode)| heap.intern_value(&format!("%{opcode}")))
    }

    fn get_index(&self, _index: usize, _heap: &mut Heap) -> Option<Value> { None }
    fn set(&mut self, _key: &str, _value: Value, _heap: &Heap) -> Result<(), RexError> { Ok(()) }
    fn call(&mut self, _method: &str, args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
        if let Some(opcode) = self.tag_opcode {
            crate::opcodes::call_opcode(opcode, args, heap)
        } else {
            Ok(Value::NONE)
        }
    }
}

/// Read-only config object backed by a serde_json::Value.
#[allow(dead_code)]
pub struct JsonHostObject {
    pub value: serde_json::Value,
}

impl HostObject for JsonHostObject {
    fn get(&self, key: &str, heap: &mut Heap) -> Option<Value> {
        match &self.value {
            serde_json::Value::Object(map) => {
                map.get(key).map(|v| json_to_value(v, heap))
            }
            _ => None,
        }
    }

    fn get_index(&self, index: usize, heap: &mut Heap) -> Option<Value> {
        match &self.value {
            serde_json::Value::Array(arr) => {
                arr.get(index).map(|v| json_to_value(v, heap))
            }
            _ => None,
        }
    }

    fn set(&mut self, _key: &str, _value: Value, _heap: &Heap) -> Result<(), RexError> {
        Err(RexError::HostError("read-only".into()))
    }

    fn call(&mut self, _method: &str, _args: &[Value], _heap: &mut Heap) -> Result<Value, RexError> {
        Ok(Value::NONE)
    }

    fn as_string(&self) -> Option<&str> {
        match &self.value {
            serde_json::Value::String(s) => Some(s),
            _ => None,
        }
    }
}

pub fn json_to_value(v: &serde_json::Value, heap: &mut Heap) -> Value {
    match v {
        serde_json::Value::Null => Value::NULL,
        serde_json::Value::Bool(b) => Value::bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::int(i)
            } else {
                heap.alloc_float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => heap.intern_value(s),
        serde_json::Value::Array(arr) => {
            let items: Vec<Value> = arr.iter().map(|v| json_to_value(v, heap)).collect();
            heap.alloc_array(items)
        }
        serde_json::Value::Object(map) => {
            let pairs: Vec<(u32, Value)> = map.iter()
                .map(|(k, v)| (heap.intern(k), json_to_value(v, heap)))
                .collect();
            heap.alloc_object(pairs)
        }
    }
}

pub fn value_to_string(v: Value, heap: &Heap) -> String {
    if let Some(s) = v.as_str(heap) {
        s.to_string()
    } else if let Some(n) = v.as_i64() {
        n.to_string()
    } else if let Some(f) = v.as_f64(heap) {
        f.to_string()
    } else if let Some(b) = v.as_bool() {
        b.to_string()
    } else if v.is_null() {
        "null".into()
    } else if v.is_none() {
        String::new()
    } else {
        format!("{v:?}")
    }
}

pub fn value_to_json(v: Value, heap: &Heap) -> serde_json::Value {
    if v.is_none() || v.is_null() {
        serde_json::Value::Null
    } else if let Some(b) = v.as_bool() {
        serde_json::Value::Bool(b)
    } else if let Some(n) = v.as_i64() {
        serde_json::json!(n)
    } else if let Some(f) = v.as_f64(heap) {
        serde_json::json!(f)
    } else if let Some(s) = v.as_str(heap) {
        serde_json::Value::String(s.to_string())
    } else if v.is_array() {
        let items: Vec<serde_json::Value> = heap.array_items(v).iter()
            .map(|&item| value_to_json(item, heap))
            .collect();
        serde_json::Value::Array(items)
    } else if v.is_object() {
        let map: serde_json::Map<String, serde_json::Value> = heap.object_pairs(v).iter()
            .map(|&(k, val)| (heap.resolve_str(k).to_string(), value_to_json(val, heap)))
            .collect();
        serde_json::Value::Object(map)
    } else {
        serde_json::Value::Null
    }
}

fn urldecode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'+' {
            result.push(' ');
        } else if b == b'%' {
            let h = chars.next().unwrap_or(b'0');
            let l = chars.next().unwrap_or(b'0');
            let byte = hex_byte(h) * 16 + hex_byte(l);
            result.push(byte as char);
        } else {
            result.push(b as char);
        }
    }
    result
}

fn hex_byte(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

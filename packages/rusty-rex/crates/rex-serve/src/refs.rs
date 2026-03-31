use rex_core::interpret::{HostObject, RexError, RexValue};

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
    fn get(&self, key: &str) -> Option<RexValue> {
        let lower = key.to_lowercase();
        // Collect all values for this header
        let values: Vec<&str> = self.headers.iter()
            .filter(|(k, _)| k == &lower)
            .map(|(_, v)| v.as_str())
            .collect();
        match values.len() {
            0 => None,
            1 => Some(RexValue::Str(values[0].to_string())),
            _ => Some(RexValue::Array(values.iter().map(|v| RexValue::Str(v.to_string())).collect())),
        }
    }

    fn get_index(&self, _index: usize) -> Option<RexValue> { None }
    fn set(&mut self, _key: &str, _value: RexValue) -> Result<(), RexError> { Ok(()) }
    fn call(&mut self, _method: &str, _args: &[RexValue]) -> Result<RexValue, RexError> {
        Ok(RexValue::RexNone)
    }

    fn iter_keys(&self) -> Option<Vec<RexValue>> {
        let mut seen = Vec::new();
        for (k, _) in &self.headers {
            if !seen.contains(k) {
                seen.push(k.clone());
            }
        }
        Some(seen.into_iter().map(RexValue::Str).collect())
    }

    fn iter_pairs(&self) -> Option<Vec<(RexValue, RexValue)>> {
        Some(self.headers.iter()
            .map(|(k, v)| (RexValue::Str(k.clone()), RexValue::Str(v.clone())))
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
    fn get(&self, key: &str) -> Option<RexValue> {
        let lower = key.to_lowercase();
        self.headers.iter()
            .find(|(k, _)| k == &lower)
            .map(|(_, v)| RexValue::Str(v.clone()))
    }

    fn get_index(&self, _index: usize) -> Option<RexValue> { None }

    fn set(&mut self, key: &str, value: RexValue) -> Result<(), RexError> {
        let lower = key.to_lowercase();
        let val_str = rex_value_to_string(&value);
        // Replace existing or add new
        if let Some(entry) = self.headers.iter_mut().find(|(k, _)| k == &lower) {
            entry.1 = val_str;
        } else {
            self.headers.push((lower, val_str));
        }
        Ok(())
    }

    fn call(&mut self, _method: &str, _args: &[RexValue]) -> Result<RexValue, RexError> {
        Ok(RexValue::RexNone)
    }

    fn iter_keys(&self) -> Option<Vec<RexValue>> {
        Some(self.headers.iter().map(|(k, _)| RexValue::Str(k.clone())).collect())
    }

    fn iter_pairs(&self) -> Option<Vec<(RexValue, RexValue)>> {
        Some(self.headers.iter()
            .map(|(k, v)| (RexValue::Str(k.clone()), RexValue::Str(v.clone())))
            .collect())
    }
}

/// Mutable response object. `res.status` and `res.headers` are accessed via this.
pub struct ResponseObject {
    pub status: u16,
    /// Index into the host_objects vec for the ResponseHeadersObject
    pub headers_host_idx: usize,
}

impl ResponseObject {
    pub fn new(headers_host_idx: usize) -> Self {
        Self { status: 200, headers_host_idx }
    }
}

impl HostObject for ResponseObject {
    fn get(&self, key: &str) -> Option<RexValue> {
        match key {
            "status" => Some(RexValue::Int(self.status as i64)),
            "headers" => Some(RexValue::Host(self.headers_host_idx)),
            _ => None,
        }
    }

    fn get_index(&self, _index: usize) -> Option<RexValue> { None }

    fn set(&mut self, key: &str, value: RexValue) -> Result<(), RexError> {
        match key {
            "status" => {
                self.status = value.to_i64().unwrap_or(200) as u16;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn call(&mut self, _method: &str, _args: &[RexValue]) -> Result<RexValue, RexError> {
        Ok(RexValue::RexNone)
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
    fn get(&self, key: &str) -> Option<RexValue> {
        self.params.iter()
            .find(|(k, _)| k == key)
            .map(|(_, vals)| {
                if vals.len() == 1 {
                    RexValue::Str(vals[0].clone())
                } else {
                    RexValue::Array(vals.iter().map(|v| RexValue::Str(v.clone())).collect())
                }
            })
    }

    fn get_index(&self, _index: usize) -> Option<RexValue> { None }
    fn set(&mut self, _key: &str, _value: RexValue) -> Result<(), RexError> { Ok(()) }
    fn call(&mut self, _method: &str, _args: &[RexValue]) -> Result<RexValue, RexError> {
        Ok(RexValue::RexNone)
    }

    fn iter_keys(&self) -> Option<Vec<RexValue>> {
        Some(self.params.iter().map(|(k, _)| RexValue::Str(k.clone())).collect())
    }

    fn iter_pairs(&self) -> Option<Vec<(RexValue, RexValue)>> {
        Some(self.params.iter().map(|(k, vals)| {
            let v = if vals.len() == 1 {
                RexValue::Str(vals[0].clone())
            } else {
                RexValue::Array(vals.iter().map(|v| RexValue::Str(v.clone())).collect())
            };
            (RexValue::Str(k.clone()), v)
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
    fn get(&self, key: &str) -> Option<RexValue> {
        self.cookies.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| RexValue::Str(v.clone()))
    }

    fn get_index(&self, _index: usize) -> Option<RexValue> { None }
    fn set(&mut self, _key: &str, _value: RexValue) -> Result<(), RexError> { Ok(()) }
    fn call(&mut self, _method: &str, _args: &[RexValue]) -> Result<RexValue, RexError> {
        Ok(RexValue::RexNone)
    }

    fn iter_keys(&self) -> Option<Vec<RexValue>> {
        Some(self.cookies.iter().map(|(k, _)| RexValue::Str(k.clone())).collect())
    }

    fn iter_pairs(&self) -> Option<Vec<(RexValue, RexValue)>> {
        Some(self.cookies.iter()
            .map(|(k, v)| (RexValue::Str(k.clone()), RexValue::Str(v.clone())))
            .collect())
    }
}

/// Namespace object that maps method names to opcode strings.
/// e.g., `time.uuid()` → Host(time_idx).get("uuid") → Str("%tu")
///
/// Optionally supports a tag opcode for tagged template literals:
/// e.g., html`<p>${x}</p>` calls the namespace with (["<p>","</p>"], x)
pub struct OpcodeNamespace {
    pub methods: Vec<(&'static str, &'static str)>,
    /// Opcode to invoke when this namespace is used as a tagged template.
    pub tag_opcode: Option<&'static str>,
}

impl HostObject for OpcodeNamespace {
    fn get(&self, key: &str) -> Option<RexValue> {
        self.methods.iter()
            .find(|(name, _)| *name == key)
            .map(|(_, opcode)| RexValue::Str(format!("%{opcode}")))
    }

    fn get_index(&self, _index: usize) -> Option<RexValue> { None }
    fn set(&mut self, _key: &str, _value: RexValue) -> Result<(), RexError> { Ok(()) }
    fn call(&mut self, _method: &str, args: &[RexValue]) -> Result<RexValue, RexError> {
        // When called directly (e.g., as a tagged template), delegate to the tag opcode
        if let Some(opcode) = self.tag_opcode {
            // Look up the opcode in thread-local registry and call it
            crate::opcodes::call_opcode(opcode, args)
        } else {
            Ok(RexValue::RexNone)
        }
    }
}

/// Read-only config object backed by a serde_json::Value.
#[allow(dead_code)]
pub struct JsonHostObject {
    pub value: serde_json::Value,
}

impl HostObject for JsonHostObject {
    fn get(&self, key: &str) -> Option<RexValue> {
        match &self.value {
            serde_json::Value::Object(map) => {
                map.get(key).map(json_to_rex)
            }
            _ => None,
        }
    }

    fn get_index(&self, index: usize) -> Option<RexValue> {
        match &self.value {
            serde_json::Value::Array(arr) => {
                arr.get(index).map(json_to_rex)
            }
            _ => None,
        }
    }

    fn set(&mut self, _key: &str, _value: RexValue) -> Result<(), RexError> {
        Err(RexError::HostError("read-only".into()))
    }

    fn call(&mut self, _method: &str, _args: &[RexValue]) -> Result<RexValue, RexError> {
        Ok(RexValue::RexNone)
    }

    fn as_string(&self) -> Option<String> {
        match &self.value {
            serde_json::Value::String(s) => Some(s.clone()),
            _ => Some(self.value.to_string()),
        }
    }
}

#[allow(dead_code)]
fn json_to_rex(v: &serde_json::Value) -> RexValue {
    match v {
        serde_json::Value::Null => RexValue::Null,
        serde_json::Value::Bool(b) => RexValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                RexValue::Int(i)
            } else {
                RexValue::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => RexValue::Str(s.clone()),
        serde_json::Value::Array(arr) => {
            RexValue::Array(arr.iter().map(json_to_rex).collect())
        }
        serde_json::Value::Object(map) => {
            RexValue::Object(map.iter().map(|(k, v)| (k.clone(), json_to_rex(v))).collect())
        }
    }
}

pub fn rex_value_to_string(v: &RexValue) -> String {
    match v {
        RexValue::Str(s) => s.clone(),
        RexValue::Int(n) => n.to_string(),
        RexValue::Float(n) => n.to_string(),
        RexValue::Bool(b) => b.to_string(),
        RexValue::Null => "null".into(),
        RexValue::RexNone => String::new(),
        _ => format!("{v:?}"),
    }
}

pub fn rex_value_to_json(v: &RexValue) -> serde_json::Value {
    match v {
        RexValue::RexNone => serde_json::Value::Null,
        RexValue::Null => serde_json::Value::Null,
        RexValue::Bool(b) => serde_json::Value::Bool(*b),
        RexValue::Int(n) => serde_json::json!(n),
        RexValue::Float(n) => serde_json::json!(n),
        RexValue::Decimal { sig, exp } => {
            let f = *sig as f64 * 10f64.powi(*exp as i32);
            serde_json::json!(f)
        }
        RexValue::Str(s) => serde_json::Value::String(s.clone()),
        RexValue::Array(items) => {
            serde_json::Value::Array(items.iter().map(rex_value_to_json).collect())
        }
        RexValue::Object(pairs) => {
            let map: serde_json::Map<String, serde_json::Value> = pairs.iter()
                .map(|(k, v)| (k.clone(), rex_value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        RexValue::Host(_) => serde_json::Value::Null,
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

//! Zero-copy cursor interpreter for REXC/RX bytecode.
//!
//! Evaluates bytecode in-place without deserializing to a `Value` tree.
//! Runtime values are heap-allocated handles — mutation works through aliases.
//! Host objects provide custom read/write/call behavior via the `HostObject` trait.

use std::collections::HashMap;
use crate::heap::{Value, Heap};

// ── Errors ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum RexError {
    GasLimitExceeded,
    UnexpectedEnd,
    UnexpectedTag(u8),
    InvalidBytecode(String),
    HostError(String),
    BreakSignal(u32),
    ContinueSignal(u32),
    ReturnSignal(Value),
}

impl std::fmt::Display for RexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RexError::GasLimitExceeded => write!(f, "gas limit exceeded"),
            RexError::UnexpectedEnd => write!(f, "unexpected end of bytecode"),
            RexError::UnexpectedTag(t) => write!(f, "unexpected tag: {:?}", *t as char),
            RexError::InvalidBytecode(msg) => write!(f, "invalid bytecode: {msg}"),
            RexError::HostError(msg) => write!(f, "host error: {msg}"),
            RexError::BreakSignal(_) => write!(f, "break outside loop"),
            RexError::ContinueSignal(_) => write!(f, "continue outside loop"),
            RexError::ReturnSignal(_) => write!(f, "return signal"),
        }
    }
}

// ── Host object trait ───────────────────────────────────────────────────

/// Host-provided proxy object with custom read/write/call behavior.
pub trait HostObject {
    fn get(&self, key: &str, heap: &mut Heap) -> Option<Value>;
    fn get_index(&self, index: usize, heap: &mut Heap) -> Option<Value>;
    fn set(&mut self, key: &str, value: Value, heap: &Heap) -> Result<(), RexError>;
    fn call(&mut self, method: &str, args: &[Value], heap: &mut Heap) -> Result<Value, RexError>;
    fn delete(&mut self, key: &str) -> Result<(), RexError> { let _ = key; Ok(()) }
    fn len(&self) -> Option<usize> { None }
    fn iter_values(&self, heap: &mut Heap) -> Option<Vec<Value>> { let _ = heap; None }
    fn iter_keys(&self, heap: &mut Heap) -> Option<Vec<Value>> { let _ = heap; None }
    fn iter_pairs(&self, heap: &mut Heap) -> Option<Vec<(Value, Value)>> { let _ = heap; None }
    fn as_string(&self) -> Option<&str> { None }
    fn as_number(&self) -> Option<f64> { None }
    fn as_bool(&self) -> Option<bool> { None }
}

// ── Context ─────────────────────────────────────────────────────────────

/// Execution context provided by the host.
pub struct Context<'a> {
    pub refs: HashMap<String, Value>,
    pub vars: HashMap<String, Value>,
    pub host_objects: Vec<&'a mut dyn HostObject>,
    pub opcodes: HashMap<String, fn(&[Value], &mut Heap) -> Result<Value, RexError>>,
    pub gas_limit: u64,
    pub heap: Heap,
}

impl<'a> Default for Context<'a> {
    fn default() -> Self {
        Self {
            refs: HashMap::new(),
            vars: HashMap::new(),
            host_objects: Vec::new(),
            opcodes: HashMap::new(),
            gas_limit: 0,
            heap: Heap::new(),
        }
    }
}

/// Result of running a Rex program.
pub struct RunResult {
    pub value: Value,
    pub vars: HashMap<String, Value>,
    pub heap: Heap,
    pub gas: u64,
}

// ── b64 ─────────────────────────────────────────────────────────────────

fn b64_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'z' => Some(b - b'a' + 10),
        b'A'..=b'Z' => Some(b - b'A' + 36),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn is_b64(b: u8) -> bool {
    b64_val(b).is_some()
}

fn parse_uint(raw: &[u8]) -> u64 {
    let mut n: u64 = 0;
    for &b in raw {
        n = n * 64 + b64_val(b).unwrap_or(0) as u64;
    }
    n
}

fn zigzag_decode(n: u64) -> i64 {
    if n % 2 == 0 { (n / 2) as i64 } else { -((n / 2) as i64) - 1 }
}

// ── Entry point ─────────────────────────────────────────────────────────

/// Run REXC/RX bytecode with the given context.
pub fn run<'a>(bytecode: &'a str, mut ctx: Context<'a>) -> Result<RunResult, RexError> {
    let mut interp = Interpreter {
        code: bytecode.as_bytes(),
        pos: 0,
        heap: ctx.heap,
        vars: HashMap::new(),
        refs: HashMap::new(),
        host_objects: ctx.host_objects,
        opcodes: ctx.opcodes,
        gas: 0,
        gas_limit: ctx.gas_limit,
        last_method_target: None,
    };

    // Intern string keys from context vars/refs into heap
    for (k, v) in std::mem::take(&mut ctx.vars) {
        let kid = interp.heap.intern(&k);
        interp.vars.insert(kid, v);
    }
    for (k, v) in std::mem::take(&mut ctx.refs) {
        let kid = interp.heap.intern(&k);
        interp.refs.insert(kid, v);
    }

    let value = interp.eval_top()?;

    // De-intern var keys for result
    let mut result_vars = HashMap::new();
    for (kid, v) in interp.vars {
        let key = interp.heap.resolve_str(kid).to_string();
        result_vars.insert(key, v);
    }

    Ok(RunResult {
        value,
        vars: result_vars,
        heap: interp.heap,
        gas: interp.gas,
    })
}

// ── Interpreter ─────────────────────────────────────────────────────────

struct Interpreter<'a> {
    code: &'a [u8],
    pos: usize,
    heap: Heap,
    vars: HashMap<u32, Value>,      // interned key → value
    refs: HashMap<u32, Value>,      // interned key → value
    host_objects: Vec<&'a mut dyn HostObject>,
    opcodes: HashMap<String, fn(&[Value], &mut Heap) -> Result<Value, RexError>>,
    gas: u64,
    gas_limit: u64,
    /// When navigation resolves to a method opcode, stash the target here
    /// so the outer call can prepend it to the args.
    last_method_target: Option<Value>,
}

impl<'a> Interpreter<'a> {
    // ── Cursor helpers ──────────────────────────────────────────────

    fn at_end(&self) -> bool {
        self.pos >= self.code.len()
    }

    fn peek(&self) -> u8 {
        if self.pos < self.code.len() { self.code[self.pos] } else { 0 }
    }

    fn read_byte(&mut self) -> u8 {
        if self.pos < self.code.len() {
            let b = self.code[self.pos];
            self.pos += 1;
            b
        } else {
            0
        }
    }

    fn read_raw(&mut self) -> &'a [u8] {
        let start = self.pos;
        while self.pos < self.code.len() && is_b64(self.code[self.pos]) {
            self.pos += 1;
        }
        &self.code[start..self.pos]
    }

    fn read_utf8(&mut self, len: usize) -> &'a str {
        let end = (self.pos + len).min(self.code.len());
        let s = std::str::from_utf8(&self.code[self.pos..end]).unwrap_or("");
        self.pos = end;
        s
    }

    fn raw_to_str(raw: &[u8]) -> &str {
        std::str::from_utf8(raw).unwrap_or("")
    }

    fn tick(&mut self) -> Result<(), RexError> {
        if self.gas_limit > 0 {
            self.gas += 1;
            if self.gas > self.gas_limit {
                return Err(RexError::GasLimitExceeded);
            }
        }
        Ok(())
    }

    // ── Top-level eval ──────────────────────────────────────────────

    fn eval_top(&mut self) -> Result<Value, RexError> {
        let mut last = Value::NONE;
        while !self.at_end() {
            match self.eval() {
                Ok(val) => last = val,
                Err(RexError::ReturnSignal(val)) => return Ok(val),
                Err(e) => return Err(e),
            }
        }
        Ok(last)
    }

    // ── Main eval dispatch ──────────────────────────────────────────

    fn eval(&mut self) -> Result<Value, RexError> {
        if self.at_end() {
            return Ok(Value::NONE);
        }

        let raw = self.read_raw();
        let tag = self.read_byte();

        match tag {
            // Scalars
            b'+' => Ok(Value::int(zigzag_decode(parse_uint(raw)))),
            b'*' => {
                let exp = zigzag_decode(parse_uint(raw));
                let sig_raw = self.read_raw();
                let sig_tag = self.read_byte();
                if sig_tag != b'+' {
                    return Err(RexError::InvalidBytecode("expected + after *".into()));
                }
                let sig = zigzag_decode(parse_uint(sig_raw));
                Ok(self.heap.alloc_decimal(sig, exp))
            }
            b',' => {
                let len = parse_uint(raw) as usize;
                let s = self.read_utf8(len);
                Ok(self.heap.intern_value(s))
            }
            b'\'' => {
                let name = Self::raw_to_str(raw);
                Ok(self.resolve_ref(name))
            }
            b'$' => {
                let name = Self::raw_to_str(raw);
                let kid = self.heap.intern(name);
                Ok(self.vars.get(&kid).copied().unwrap_or(Value::NONE))
            }
            b'%' => {
                // Standalone opcode (type predicate keyword)
                let name = Self::raw_to_str(raw);
                Ok(self.heap.intern_value(&format!("%{name}")))
            }
            b'\\' => {
                let v = parse_uint(raw) as u32;
                if v % 2 == 0 {
                    Err(RexError::BreakSignal(v / 2))
                } else {
                    Err(RexError::ContinueSignal(v / 2))
                }
            }
            b';' => {
                let val = self.eval()?;
                Err(RexError::ReturnSignal(val))
            }
            b'^' => {
                let delta = parse_uint(raw) as usize;
                let target = self.pos + delta;
                let save = self.pos;
                self.pos = target;
                let val = self.eval()?;
                self.pos = save;
                Ok(val)
            }

            // String chain (template literals)
            b'.' => {
                let size = parse_uint(raw) as usize;
                let end = self.pos + size;
                let mut segments = Vec::new();
                while self.pos < end {
                    segments.push(self.eval()?);
                }

                let has_array = segments.iter().any(|v| v.is_array());
                let has_object = segments.iter().any(|v| v.is_object());

                // Spread chains for arrays: [ ...a 42 ]
                if has_array && !has_object {
                    let mut items = Vec::new();
                    for seg in segments {
                        if seg.is_array() {
                            items.extend_from_slice(self.heap.array_items(seg));
                        } else if !seg.is_none() {
                            // Non-array segments are kept as single items.
                            items.push(seg);
                        }
                    }
                    return Ok(self.heap.alloc_array(items));
                }

                // Spread chains for objects: { ...base k:v }
                if has_object && !has_array {
                    let mut pairs: Vec<(u32, Value)> = Vec::new();
                    for seg in segments {
                        if !seg.is_object() {
                            continue;
                        }
                        for &(k, v) in self.heap.object_pairs(seg) {
                            if v.is_none() {
                                pairs.retain(|(ek, _)| *ek != k);
                                continue;
                            }
                            if let Some(existing) = pairs.iter_mut().find(|(ek, _)| *ek == k) {
                                existing.1 = v;
                            } else {
                                pairs.push((k, v));
                            }
                        }
                    }
                    return Ok(self.heap.alloc_object(pairs));
                }

                // Template string chain
                let mut result = String::new();
                for seg in segments {
                    if let Some(s) = seg.as_str(&self.heap) {
                        result.push_str(s);
                    } else if let Some(n) = seg.as_i64() {
                        result.push_str(&n.to_string());
                    } else if let Some(f) = seg.as_f64(&self.heap) {
                        if f.is_infinite() {
                            result.push(if f > 0.0 { '\u{221E}' } else { '-' });
                            if f < 0.0 { result.push('\u{221E}'); }
                        } else if f.is_nan() {
                            result.push_str("NaN");
                        } else {
                            result.push_str(&f.to_string());
                        }
                    } else if let Some(b) = seg.as_bool() {
                        result.push(if b { '\u{2713}' } else { '\u{2717}' });
                    } else if seg.is_null() {
                        result.push('\u{2400}');
                    } else if seg.is_none() {
                        result.push('\u{2205}');
                    }
                    // arrays, objects — skip
                }
                Ok(self.heap.intern_value(&result))
            }

            // Paired containers
            b'(' => self.eval_call(),
            b'[' => {
                if self.peek_is_index() {
                    self.eval_indexed_array()
                } else {
                    self.eval_eager_array()
                }
            }
            b'{' => self.eval_block(),

            // Compound containers
            b'?' => self.eval_cond(),
            b'|' => self.eval_or(),
            b'&' => self.eval_and(),
            b'>' => self.eval_for_in(),
            b'<' => self.eval_for_of(),
            b'#' => self.eval_while(),
            // Mutation
            b'=' => self.eval_set(),
            b'/' => self.eval_swap(),
            b'~' => self.eval_delete(),

            0 => Ok(Value::NONE),
            _ => Err(RexError::UnexpectedTag(tag)),
        }
    }

    // ── Eager containers ────────────────────────────────────────────

    fn eval_call(&mut self) -> Result<Value, RexError> {
        let callee = self.eval()?;
        let mut args = Vec::new();
        while self.peek() != b')' && !self.at_end() {
            args.push(self.eval()?);
        }
        self.read_byte(); // consume ')'

        // Method call: if callee is "%opcode", it came from method navigation.
        // The method's target was lost during inner eval, but we stored it.
        if let Some(s) = callee.as_str(&self.heap) {
            if let Some(opname) = s.strip_prefix('%') {
                if let Some(target) = self.last_method_target.take() {
                    let opname = opname.to_string();
                    let mut method_args = vec![target];
                    method_args.extend(args);
                    return self.apply_opcode(&opname, &method_args);
                }
            }
        }
        self.dispatch_call(callee, args)
    }

    fn eval_eager_array(&mut self) -> Result<Value, RexError> {
        let mut items = Vec::new();
        while self.peek() != b']' && !self.at_end() {
            items.push(self.eval()?);
        }
        self.read_byte(); // consume ']'
        Ok(self.heap.alloc_array(items))
    }

    fn peek_is_index(&self) -> bool {
        let mut i = self.pos;
        while i < self.code.len() && is_b64(self.code[i]) { i += 1; }
        i > self.pos && i < self.code.len() && self.code[i] == b'#'
    }

    fn eval_indexed_array(&mut self) -> Result<Value, RexError> {
        let raw = self.read_raw();
        let packed = parse_uint(raw);
        self.read_byte(); // consume '#'

        let count = (packed >> 3) as usize;
        let width = ((packed & 7) + 1) as usize;
        self.pos += count * width; // skip pointer table

        let mut items = Vec::with_capacity(count);
        while self.peek() != b']' && !self.at_end() {
            items.push(self.eval()?);
        }
        self.read_byte(); // consume ']'
        Ok(self.heap.alloc_array(items))
    }

    fn eval_indexed_object(&mut self) -> Result<Value, RexError> {
        let raw = self.read_raw();
        let packed = parse_uint(raw);
        self.read_byte(); // consume '#'

        let count = (packed >> 3) as usize;
        let width = ((packed & 7) + 1) as usize;
        self.pos += count * width; // skip pointer table

        let mut pairs = Vec::with_capacity(count);
        while self.peek() != b'}' && !self.at_end() {
            let k = self.eval()?;
            let v = self.eval()?;
            let kid = self.heap.value_to_key(k);
            Self::upsert_object_pair(&mut pairs, kid, v);
        }
        self.read_byte(); // consume '}'
        Ok(self.heap.alloc_object(pairs))
    }

    fn eval_block(&mut self) -> Result<Value, RexError> {
        if self.peek() == b'}' {
            self.read_byte();
            return Ok(self.heap.alloc_object(vec![]));
        }

        // Indexed object
        if self.peek_is_index() {
            return self.eval_indexed_object();
        }

        if self.peek_is_string_literal() {
            return self.eval_object();
        }

        // Schema pointer
        if self.peek_is_pointer() {
            // Read the pointer manually to get the target position
            let raw = self.read_raw();
            let tag = self.read_byte(); // consume '^'
            debug_assert_eq!(tag, b'^');
            let delta = parse_uint(raw) as usize;
            let target = self.pos + delta;

            // Find the object opener at the target position, skipping any
            // size prefix (b64 digits before the `{`).
            let obj_start = {
                let mut i = target;
                while i < self.code.len() && is_b64(self.code[i]) { i += 1; }
                i
            };

            if obj_start < self.code.len() && self.code[obj_start] == b'{' {
                // Target is an object — scan its bytecode to extract keys
                // without evaluating values (which may contain expressions
                // that resolve to none and would be dropped).
                let keys = self.scan_object_keys(obj_start + 1)?;
                let mut pairs = Vec::new();
                for &key in &keys {
                    let v = self.eval()?;
                    Self::upsert_object_pair(&mut pairs, key, v);
                }
                self.read_byte(); // consume '}'
                return Ok(self.heap.alloc_object(pairs));
            }

            // Not an object target — evaluate normally
            let save = self.pos;
            self.pos = target;
            let first = self.eval()?;
            self.pos = save;

            if first.is_string() {
                // Pointer resolved to a string → deduped first key
                let kid = first.string_id().unwrap();
                let mut pairs = Vec::new();
                let v = self.eval()?;
                Self::upsert_object_pair(&mut pairs, kid, v);
                while self.peek() != b'}' && !self.at_end() {
                    let k = self.eval()?;
                    let v = self.eval()?;
                    let kid = self.heap.value_to_key(k);
                    Self::upsert_object_pair(&mut pairs, kid, v);
                }
                self.read_byte(); // consume '}'
                return Ok(self.heap.alloc_object(pairs));
            } else if first.is_array() {
                // Pointer resolved to an array → schema-shared object (array of keys)
                let len = self.heap.array_len(first);
                let keys: Vec<u32> = (0..len)
                    .map(|i| self.heap.value_to_key(self.heap.array_get(first, i)))
                    .collect();
                let mut pairs = Vec::new();
                for &key in &keys {
                    let v = self.eval()?;
                    Self::upsert_object_pair(&mut pairs, key, v);
                }
                self.read_byte(); // consume '}'
                return Ok(self.heap.alloc_object(pairs));
            } else {
                return Err(RexError::InvalidBytecode(
                    "{} first child must resolve to string, object, or array".into()
                ));
            }
        }

        Err(RexError::InvalidBytecode(
            "{} first child must be string literal, pointer, or index".into()
        ))
    }

    fn peek_is_string_literal(&self) -> bool {
        let mut i = self.pos;
        while i < self.code.len() && is_b64(self.code[i]) { i += 1; }
        i < self.code.len() && self.code[i] == b','
    }

    fn peek_is_pointer(&self) -> bool {
        let mut i = self.pos;
        while i < self.code.len() && is_b64(self.code[i]) { i += 1; }
        i < self.code.len() && self.code[i] == b'^'
    }

    fn eval_object(&mut self) -> Result<Value, RexError> {
        let mut pairs = Vec::new();
        while self.peek() != b'}' && !self.at_end() {
            let k = self.eval()?;
            let v = self.eval()?;
            let kid = self.heap.value_to_key(k);
            Self::upsert_object_pair(&mut pairs, kid, v);
        }
        self.read_byte(); // consume '}'
        Ok(self.heap.alloc_object(pairs))
    }

    fn upsert_object_pair(pairs: &mut Vec<(u32, Value)>, key: u32, value: Value) {
        // `none` means key deletion/omission for object construction.
        if value.is_none() {
            pairs.retain(|(k, _)| *k != key);
            return;
        }
        if let Some((_, existing)) = pairs.iter_mut().find(|(k, _)| *k == key) {
            *existing = value;
        } else {
            pairs.push((key, value));
        }
    }

    // ── Control flow ────────────────────────────────────────────────

    fn eval_cond(&mut self) -> Result<Value, RexError> {
        self.read_byte(); // consume '('
        while self.peek() != b')' && !self.at_end() {
            let cond = self.eval()?;
            if self.peek() == b')' {
                self.read_byte();
                return Ok(cond);
            }
            if cond.is_defined() {
                let result = self.eval()?;
                while self.peek() != b')' && !self.at_end() {
                    self.skip_value_fast()?;
                }
                self.read_byte();
                return Ok(result);
            } else {
                self.skip_value_fast()?;
            }
        }
        self.read_byte();
        Ok(Value::NONE)
    }

    fn eval_or(&mut self) -> Result<Value, RexError> {
        self.read_byte();
        while self.peek() != b')' && !self.at_end() {
            let val = self.eval()?;
            if val.is_defined() {
                while self.peek() != b')' && !self.at_end() {
                    self.skip_value_fast()?;
                }
                self.read_byte();
                return Ok(val);
            }
        }
        self.read_byte();
        Ok(Value::NONE)
    }

    fn eval_and(&mut self) -> Result<Value, RexError> {
        self.read_byte();
        let mut last = Value::NONE;
        while self.peek() != b')' && !self.at_end() {
            last = self.eval()?;
            if !last.is_defined() {
                while self.peek() != b')' && !self.at_end() {
                    self.skip_value_fast()?;
                }
                self.read_byte();
                return Ok(Value::NONE);
            }
        }
        self.read_byte();
        Ok(last)
    }

    fn eval_for_in(&mut self) -> Result<Value, RexError> {
        self.read_byte(); // opener
        let opener = self.code[self.pos - 1];
        let closer = match opener { b'(' => b')', b'[' => b']', b'{' => b'}', _ => b')' };

        let iterable = self.eval()?;

        // Count children to find body boundary
        let scan_start = self.pos;
        let mut child_count: usize = 0;
        while self.peek() != closer && !self.at_end() {
            self.skip_value()?;
            child_count += 1;
        }
        self.pos = scan_start;

        let body_count: usize = if opener == b'{' { 2 } else { 1 };
        let max_bindings = child_count.saturating_sub(body_count);
        let mut bindings = Vec::new();
        while bindings.len() < max_bindings && self.peek() != closer && !self.at_end() {
            let save = self.pos;
            let raw = self.read_raw();
            if self.peek() == b'$' {
                self.read_byte();
                let name = Self::raw_to_str(raw);
                bindings.push(self.heap.intern(name));
            } else {
                self.pos = save;
                break;
            }
        }

        let body_start = self.pos;
        self.skip_until(closer)?;
        let body_end = self.pos - 1;

        let items = self.materialize_iterable(iterable)?;
        let keys = if bindings.len() == 2 {
            Some(self.materialize_keys(iterable)?)
        } else {
            None
        };
        let mut results = Vec::new();
        let mut obj_pairs: Vec<(u32, Value)> = Vec::new();

        for (i, item) in items.iter().enumerate() {
            self.tick()?;

            if bindings.len() == 1 {
                self.vars.insert(bindings[0], *item);
            } else if bindings.len() == 2 {
                let key = keys.as_ref().and_then(|k| k.get(i).copied()).unwrap_or(Value::int(i as i64));
                self.vars.insert(bindings[0], key);
                self.vars.insert(bindings[1], *item);
            }

            self.pos = body_start;
            match self.eval_body(closer, opener == b'{') {
                Ok((val, key)) => {
                    if opener == b'{' {
                        // Object comprehension: key and value must both be defined
                        if let Some(k) = key {
                            if k.is_defined() && val.is_defined() {
                                let kid = self.heap.value_to_key(k);
                                obj_pairs.push((kid, val));
                            }
                        }
                    } else if opener == b'(' || val.is_defined() {
                        results.push(val);
                    }
                }
                Err(RexError::BreakSignal(0)) => { break; }
                Err(RexError::ContinueSignal(0)) => { continue; }
                Err(e) => { return Err(e); }
            }
        }

        self.pos = body_end + 1;

        if opener == b'{' {
            Ok(self.heap.alloc_object(obj_pairs))
        } else if opener == b'(' {
            Ok(results.last().copied().unwrap_or(Value::NONE))
        } else {
            Ok(self.heap.alloc_array(results))
        }
    }

    fn eval_for_of(&mut self) -> Result<Value, RexError> {
        self.read_byte(); // opener
        let opener = self.code[self.pos - 1];
        let closer = match opener { b'(' => b')', b'[' => b']', b'{' => b'}', _ => b')' };

        let iterable = self.eval()?;

        let scan_start = self.pos;
        let mut child_count: usize = 0;
        while self.peek() != closer && !self.at_end() {
            self.skip_value()?;
            child_count += 1;
        }
        self.pos = scan_start;

        let body_count: usize = if opener == b'{' { 2 } else { 1 };
        let max_bindings = child_count.saturating_sub(body_count);
        let mut bindings = Vec::new();
        while bindings.len() < max_bindings && self.peek() != closer && !self.at_end() {
            let save = self.pos;
            let raw = self.read_raw();
            if self.peek() == b'$' {
                self.read_byte();
                let name = Self::raw_to_str(raw);
                bindings.push(self.heap.intern(name));
            } else {
                self.pos = save;
                break;
            }
        }

        let body_start = self.pos;
        self.skip_until(closer)?;
        let body_end = self.pos - 1;

        let keys = self.materialize_keys(iterable)?;
        let mut results = Vec::new();
        let mut obj_pairs: Vec<(u32, Value)> = Vec::new();

        for key in &keys {
            self.tick()?;

            if !bindings.is_empty() {
                self.vars.insert(bindings[0], *key);
            }

            self.pos = body_start;
            match self.eval_body(closer, opener == b'{') {
                Ok((val, key)) => {
                    if opener == b'{' {
                        if let Some(k) = key {
                            if k.is_defined() && val.is_defined() {
                                let kid = self.heap.value_to_key(k);
                                obj_pairs.push((kid, val));
                            }
                        }
                    } else if opener == b'(' || val.is_defined() {
                        results.push(val);
                    }
                }
                Err(RexError::BreakSignal(0)) => { break; }
                Err(RexError::ContinueSignal(0)) => { continue; }
                Err(e) => { return Err(e); }
            }
        }

        self.pos = body_end + 1;
        if opener == b'{' {
            Ok(self.heap.alloc_object(obj_pairs))
        } else if opener == b'(' {
            Ok(results.last().copied().unwrap_or(Value::NONE))
        } else {
            Ok(self.heap.alloc_array(results))
        }
    }

    fn eval_while(&mut self) -> Result<Value, RexError> {
        self.read_byte(); // opener
        let opener = self.code[self.pos - 1];
        let closer = match opener { b'(' => b')', b'[' => b']', b'{' => b'}', _ => b')' };

        let cond_start = self.pos;
        let save = self.pos;
        self.skip_value()?;
        let body_start = self.pos;
        self.skip_until(closer)?;
        let body_end = self.pos - 1;
        self.pos = save;

        let mut results = Vec::new();
        let mut obj_pairs: Vec<(u32, Value)> = Vec::new();
        loop {
            self.tick()?;
            self.pos = cond_start;
            let cond = self.eval()?;
            if !cond.is_defined() {
                break;
            }

            self.pos = body_start;
            match self.eval_body(closer, opener == b'{') {
                Ok((val, key)) => {
                    if opener == b'{' {
                        if let Some(k) = key {
                            if k.is_defined() && val.is_defined() {
                                let kid = self.heap.value_to_key(k);
                                obj_pairs.push((kid, val));
                            }
                        }
                    } else if opener == b'(' || val.is_defined() {
                        results.push(val);
                    }
                }
                Err(RexError::BreakSignal(0)) => { break; }
                Err(RexError::ContinueSignal(0)) => { continue; }
                Err(e) => { return Err(e); }
            }
        }

        self.pos = body_end + 1;
        if opener == b'{' {
            Ok(self.heap.alloc_object(obj_pairs))
        } else if opener == b'(' {
            Ok(results.last().copied().unwrap_or(Value::NONE))
        } else {
            Ok(self.heap.alloc_array(results))
        }
    }

    /// Evaluate body expressions until closer. For object comprehensions
    /// (is_obj=true), returns (value, Some(key)) where the second-to-last
    /// expression is the key and the last is the value.
    fn eval_body(&mut self, closer: u8, is_obj: bool) -> Result<(Value, Option<Value>), RexError> {
        if !is_obj {
            let val = self.eval_until(closer)?;
            return Ok((val, None));
        }
        // Object: collect all values, last two are key and value
        let mut vals = Vec::new();
        while self.peek() != closer && !self.at_end() {
            vals.push(self.eval()?);
        }
        match vals.len() {
            0 => Ok((Value::NONE, Some(Value::NONE))),
            1 => Ok((vals[0], Some(Value::NONE))),
            _ => {
                let val = vals[vals.len() - 1];
                let key = vals[vals.len() - 2];
                Ok((val, Some(key)))
            }
        }
    }

    fn eval_until(&mut self, closer: u8) -> Result<Value, RexError> {
        let mut last = Value::NONE;
        while self.peek() != closer && !self.at_end() {
            last = self.eval()?;
        }
        Ok(last)
    }

    // ── Call dispatch ────────────────────────────────────────────────

    fn dispatch_call(&mut self, callee: Value, args: Vec<Value>) -> Result<Value, RexError> {
        // Opcode call: "%ad", "%lt", etc.
        if let Some(s) = callee.as_str(&self.heap) {
            if let Some(opname) = s.strip_prefix('%') {
                let opname = opname.to_string();
                return self.apply_opcode(&opname, &args);
            }
        }

        // Host object call (not navigation)
        if let Some(idx) = callee.host_id() {
            if !args.is_empty() && args[0].as_str(&self.heap).is_none() {
                return self.host_objects[idx as usize].call("", &args, &mut self.heap);
            }
        }

        // Navigation
        let mut target = callee;
        for arg in &args {
            let prop = self.read_property(target, *arg)?;
            // If navigation resolved to a method opcode, stash the target
            // for the outer eval_call to pick up
            if let Some(s) = prop.as_str(&self.heap) {
                if s.starts_with('%') {
                    self.last_method_target = Some(target);
                    return Ok(prop);
                }
            }
            target = prop;
        }
        Ok(target)
    }

    // ── Property access ─────────────────────────────────────────────

    fn read_property(&mut self, target: Value, key: Value) -> Result<Value, RexError> {
        if target.is_object() {
            let kid = self.heap.value_to_key(key);
            return Ok(self.heap.object_get(target, kid));
        }

        if target.is_array() {
            if let Some(k) = key.as_str(&self.heap) {
                if k == "size" { return Ok(Value::int(self.heap.array_len(target) as i64)); }
                // Built-in methods → return opcode string
                if let Some(op) = array_method(k) {
                    return Ok(self.heap.intern_value(op));
                }
                if let Ok(idx) = k.parse::<usize>() {
                    return Ok(self.heap.array_get(target, idx));
                }
            }
            if let Some(idx) = key.to_i64(&self.heap) {
                if idx >= 0 {
                    return Ok(self.heap.array_get(target, idx as usize));
                }
            }
            return Ok(Value::NONE);
        }

        if target.is_string() {
            if let Some(k) = key.as_str(&self.heap) {
                if let Some(op) = string_method(k) {
                    return Ok(self.heap.intern_value(op));
                }
                if k == "size" {
                    let s = target.as_str(&self.heap).unwrap();
                    return Ok(Value::int(s.chars().count() as i64));
                }
            }
            if let Some(idx) = key.to_i64(&self.heap) {
                if idx >= 0 {
                    let s = target.as_str(&self.heap).unwrap().to_string();
                    if let Some(c) = s.chars().nth(idx as usize) {
                        return Ok(self.heap.intern_value(&c.to_string()));
                    }
                }
            }
            return Ok(Value::NONE);
        }

        // Blob access
        if self.heap.is_blob(target) {
            if let Some(k) = key.as_str(&self.heap) {
                if k == "size" {
                    let data = self.heap.blob_data(target).unwrap();
                    return Ok(Value::int(data.len() as i64));
                }
                if k == "slice" {
                    return Ok(self.heap.intern_value("%bS"));
                }
            }
            return Ok(Value::NONE);
        }

        if let Some(idx) = target.host_id() {
            if key.is_string() {
                let k = key.as_str(&self.heap).unwrap().to_string();
                return Ok(self.host_objects[idx as usize].get(&k, &mut self.heap).unwrap_or(Value::NONE));
            }
            if let Some(i) = key.to_i64(&self.heap) {
                return Ok(self.host_objects[idx as usize].get_index(i as usize, &mut self.heap).unwrap_or(Value::NONE));
            }
            return Ok(Value::NONE);
        }

        Ok(Value::NONE)
    }

    // ── Mutation (set/delete) ──────────────────────────────────────

    fn eval_set(&mut self) -> Result<Value, RexError> {
        let raw = self.read_raw();
        let tag = self.peek();

        if tag == b'$' {
            // Simple variable: $name = value
            self.read_byte();
            let name = Self::raw_to_str(raw);
            let kid = self.heap.intern(name);
            let val = self.eval()?;
            self.vars.insert(kid, val);
            Ok(val)
        } else if tag == b'(' {
            // Navigation chain: (target keys...) = value
            self.read_byte();

            let mut parts = Vec::new();
            while self.peek() != b')' && !self.at_end() {
                parts.push(self.eval()?);
            }
            self.read_byte(); // consume ')'

            let val = self.eval()?;

            if parts.len() >= 2 {
                // Navigate to parent, then write the last key
                let mut target = parts[0];
                for i in 1..parts.len() - 1 {
                    target = self.read_property(target, parts[i])?;
                }
                let last_key = parts[parts.len() - 1];
                self.write_property(target, last_key, val)?;
            }

            Ok(val)
        } else if tag == b'^' {
            // Pointer to a place expression
            self.read_byte();
            let delta = parse_uint(raw) as usize;
            let target = self.pos + delta;

            let target_tag = {
                let mut i = target;
                while i < self.code.len() && is_b64(self.code[i]) { i += 1; }
                if i < self.code.len() { self.code[i] } else { 0 }
            };

            if target_tag == b'(' {
                let save = self.pos;
                self.pos = target;
                let _raw2 = self.read_raw();
                self.read_byte(); // consume '('

                let mut parts = Vec::new();
                while self.peek() != b')' && !self.at_end() {
                    parts.push(self.eval()?);
                }
                self.read_byte(); // consume ')'
                self.pos = save;

                let val = self.eval()?;

                if parts.len() >= 2 {
                    let mut nav_target = parts[0];
                    for i in 1..parts.len() - 1 {
                        nav_target = self.read_property(nav_target, parts[i])?;
                    }
                    let last_key = parts[parts.len() - 1];
                    self.write_property(nav_target, last_key, val)?;
                }
                Ok(val)
            } else {
                let save = self.pos;
                self.pos = target;
                let _place = self.eval()?;
                self.pos = save;
                let val = self.eval()?;
                Ok(val)
            }
        } else {
            let _place = self.eval()?;
            let val = self.eval()?;
            Ok(val)
        }
    }

    fn eval_swap(&mut self) -> Result<Value, RexError> {
        let raw = self.read_raw();
        let tag = self.peek();

        if tag == b'$' {
            // Simple variable: /var newval → returns old, sets new
            self.read_byte();
            let name = Self::raw_to_str(raw);
            let kid = self.heap.intern(name);
            let old = self.vars.get(&kid).copied().unwrap_or(Value::NONE);
            let val = self.eval()?;
            self.vars.insert(kid, val);
            Ok(old)
        } else if tag == b'(' {
            // Navigation: /(target keys...) newval → returns old, sets new
            self.read_byte();
            let mut parts = Vec::new();
            while self.peek() != b')' && !self.at_end() {
                parts.push(self.eval()?);
            }
            self.read_byte(); // consume ')'
            let val = self.eval()?;

            if parts.len() >= 2 {
                let mut target = parts[0];
                for i in 1..parts.len() - 1 {
                    target = self.read_property(target, parts[i])?;
                }
                let last_key = parts[parts.len() - 1];
                let old = self.read_property(target, last_key).unwrap_or(Value::NONE);
                self.write_property(target, last_key, val)?;
                Ok(old)
            } else {
                Ok(Value::NONE)
            }
        } else {
            let _place = self.eval()?;
            let val = self.eval()?;
            Ok(val)
        }
    }

    /// Write a property on a target value. Works for objects, arrays, and host objects.
    fn write_property(&mut self, target: Value, key: Value, val: Value) -> Result<(), RexError> {
        if target.is_object() {
            let kid = self.heap.value_to_key(key);
            self.heap.object_set(target, kid, val);
            return Ok(());
        }

        if target.is_array() {
            if let Some(idx) = key.to_i64(&self.heap) {
                if idx >= 0 {
                    self.heap.array_set(target, idx as usize, val);
                }
            }
            return Ok(());
        }

        if let Some(idx) = target.host_id() {
            if key.is_string() {
                let k = key.as_str(&self.heap).unwrap().to_string();
                self.host_objects[idx as usize].set(&k, val, &self.heap)?;
            }
            return Ok(());
        }

        Ok(())
    }

    fn eval_delete(&mut self) -> Result<Value, RexError> {
        let raw = self.read_raw();
        let tag = self.peek();
        if tag == b'$' {
            // Delete variable
            self.read_byte();
            let name = Self::raw_to_str(raw);
            let kid = self.heap.intern(name);
            self.vars.remove(&kid);
        } else if tag == b'(' || tag == b'^' {
            // Delete property: ~(obj key) or ~^pointer
            // For pointers, follow to the call, then delete
            let parts = if tag == b'^' {
                self.read_byte(); // consume '^'
                let delta = parse_uint(raw) as usize;
                let target = self.pos + delta;
                let save = self.pos;
                self.pos = target;
                let _raw2 = self.read_raw();
                self.read_byte(); // consume '('
                let mut p = Vec::new();
                while self.peek() != b')' && !self.at_end() {
                    p.push(self.eval()?);
                }
                self.read_byte(); // consume ')'
                self.pos = save;
                p
            } else {
                self.read_byte(); // consume '('
                let mut p = Vec::new();
                while self.peek() != b')' && !self.at_end() {
                    p.push(self.eval()?);
                }
                self.read_byte(); // consume ')'
                p
            };
            if parts.len() >= 2 {
                let mut target = parts[0];
                for i in 1..parts.len() - 1 {
                    target = self.read_property(target, parts[i])?;
                }
                let last_key = parts[parts.len() - 1];
                if target.is_object() {
                    let kid = self.heap.value_to_key(last_key);
                    self.heap.object_delete(target, kid);
                }
            }
        } else {
            self.skip_value()?;
        }
        Ok(Value::NONE)
    }

    // ── Iteration helpers ───────────────────────────────────────────

    fn materialize_iterable(&mut self, value: Value) -> Result<Vec<Value>, RexError> {
        if value.is_array() {
            return Ok(self.heap.array_items(value).to_vec());
        }
        if value.is_object() {
            return Ok(self.heap.object_pairs(value).iter().map(|&(_, v)| v).collect());
        }
        if value.is_string() {
            let s = value.as_str(&self.heap).unwrap().to_string();
            let chars: Vec<Value> = s.chars()
                .map(|c| self.heap.intern_value(&c.to_string()))
                .collect();
            return Ok(chars);
        }
        if let Some(idx) = value.host_id() {
            return Ok(self.host_objects[idx as usize].iter_values(&mut self.heap).unwrap_or_default());
        }
        Ok(vec![])
    }

    fn materialize_keys(&mut self, value: Value) -> Result<Vec<Value>, RexError> {
        if value.is_object() {
            return Ok(self.heap.object_pairs(value).iter()
                .map(|&(k, _)| Value::string(k))
                .collect());
        }
        if value.is_array() {
            let len = self.heap.array_len(value);
            return Ok((0..len).map(|i| Value::int(i as i64)).collect());
        }
        if let Some(idx) = value.host_id() {
            return Ok(self.host_objects[idx as usize].iter_keys(&mut self.heap).unwrap_or_default());
        }
        Ok(vec![])
    }

    // ── Refs ────────────────────────────────────────────────────────

    fn resolve_ref(&mut self, name: &str) -> Value {
        match name {
            "t" => Value::TRUE,
            "f" => Value::FALSE,
            "n" => Value::NULL,
            "no" => Value::NONE,
            "nan" => self.heap.alloc_float(f64::NAN),
            "inf" => self.heap.alloc_float(f64::INFINITY),
            "nif" => self.heap.alloc_float(f64::NEG_INFINITY),
            other => {
                let kid = self.heap.intern(other);
                self.refs.get(&kid).copied().unwrap_or(Value::NONE)
            }
        }
    }

    // ── Schema scanning ───────────────────────────────────────────

    /// Scan an object's bytecode starting at `start` (just after the `{`)
    /// to extract key names. Evaluates keys, skips values. Restores pos
    /// when done.
    fn scan_object_keys(&mut self, start: usize) -> Result<Vec<u32>, RexError> {
        let save = self.pos;
        self.pos = start;
        let mut keys = Vec::new();
        while self.peek() != b'}' && !self.at_end() {
            // Evaluate the key
            let k = self.eval()?;
            let kid = self.heap.value_to_key(k);
            keys.push(kid);
            // Skip the value without evaluating
            self.skip_value()?;
        }
        self.pos = save;
        Ok(keys)
    }

    // ── Skip ────────────────────────────────────────────────────────

    fn skip_value(&mut self) -> Result<(), RexError> {
        if self.at_end() { return Ok(()); }
        let raw = self.read_raw();
        let tag = self.read_byte();
        match tag {
            b'+' | b'\'' | b'$' | b'%' | b'\\' | b'^' => {}
            b'*' => { self.skip_value()?; }
            b',' | b'.' => {
                let size = parse_uint(raw) as usize;
                self.pos += size;
            }
            b'(' | b'[' | b'{' => {
                let closer = match tag { b'(' => b')', b'[' => b']', _ => b'}' };
                if !raw.is_empty() {
                    let size = parse_uint(raw) as usize;
                    self.pos += size;
                    if self.peek() == closer { self.read_byte(); }
                } else {
                    if tag != b'(' && self.peek_is_index() {
                        self.skip_index();
                    }
                    self.skip_until(closer)?;
                }
            }
            b'?' | b'|' | b'&' | b'>' | b'<' | b'#' => {
                let opener = self.read_byte();
                let closer = match opener { b'(' => b')', b'[' => b']', b'{' => b'}', _ => b')' };
                self.skip_until(closer)?;
            }
            b'=' | b'/' => { self.skip_value()?; self.skip_value()?; }
            b'~' => { self.skip_value()?; }
            b';' => { self.skip_value_fast()?; }
            _ => {}
        }
        Ok(())
    }

    fn skip_value_fast(&mut self) -> Result<(), RexError> {
        if self.at_end() { return Ok(()); }
        let save = self.pos;
        let raw = self.read_raw();
        if !raw.is_empty() && matches!(self.peek(), b'[' | b'{' | b'(') {
            let size = parse_uint(raw) as usize;
            self.read_byte(); // consume opener
            self.pos += size;
            self.read_byte(); // consume closer
            return Ok(());
        }
        self.pos = save;
        self.skip_value()
    }

    fn skip_index(&mut self) {
        let raw = self.read_raw();
        let packed = parse_uint(raw);
        self.read_byte(); // consume '#'
        let count = (packed >> 3) as usize;
        let width = ((packed & 7) + 1) as usize;
        self.pos += count * width;
    }

    fn skip_until(&mut self, closer: u8) -> Result<(), RexError> {
        while self.peek() != closer && !self.at_end() {
            self.skip_value()?;
        }
        if !self.at_end() { self.read_byte(); }
        Ok(())
    }

    // ── Opcodes ─────────────────────────────────────────────────────

    fn apply_opcode(&mut self, name: &str, args: &[Value]) -> Result<Value, RexError> {
        if let Some(f) = self.opcodes.get(name) {
            return f(args, &mut self.heap);
        }

        match name {
            // Empty opcode = block: evaluate all args (already done), return last
            "" => Ok(args.last().copied().unwrap_or(Value::NONE)),
            "ad" => self.op_add(args),
            "sb" => self.op_arith(args, |a, b| a - b),
            "ml" => self.op_arith(args, |a, b| a * b),
            "dv" => self.op_arith(args, |a, b| if b != 0.0 { a / b } else { f64::NAN }),
            "md" => self.op_arith(args, |a, b| if b != 0.0 { a % b } else { f64::NAN }),
            "ng" => {
                if args.is_empty() { return Ok(Value::NONE); }
                if let Some(n) = args[0].as_i64() {
                    Ok(Value::int(-n))
                } else if let Some(f) = args[0].as_f64(&self.heap) {
                    Ok(self.heap.alloc_float(-f))
                } else {
                    Ok(Value::NONE)
                }
            }
            "eq" => self.op_eq(args, false),
            "nq" => self.op_eq(args, true),
            "gt" => self.op_compare(args, |o| o == std::cmp::Ordering::Greater),
            "ge" => self.op_compare(args, |o| o != std::cmp::Ordering::Less),
            "lt" => self.op_compare(args, |o| o == std::cmp::Ordering::Less),
            "le" => self.op_compare(args, |o| o != std::cmp::Ordering::Greater),
            "an" => self.op_bitwise(args, |a, b| a & b),
            "or" => self.op_bitwise(args, |a, b| a | b),
            "xr" => self.op_bitwise(args, |a, b| a ^ b),
            "nt" => {
                if args.is_empty() { return Ok(Value::NONE); }
                if let Some(n) = args[0].as_i64() {
                    Ok(Value::int(!n))
                } else if let Some(b) = args[0].as_bool() {
                    Ok(Value::bool(!b))
                } else {
                    Ok(Value::NONE)
                }
            }
            "rn" => {
                if args.len() < 2 { return Ok(self.heap.alloc_array(vec![])); }
                let from = args[0].to_i64(&self.heap).unwrap_or(0);
                let to = args[1].to_i64(&self.heap).unwrap_or(0);
                let items: Vec<Value> = if from <= to {
                    (from..=to).map(Value::int).collect()
                } else {
                    (to..=from).rev().map(Value::int).collect()
                };
                Ok(self.heap.alloc_array(items))
            }
            // Type predicates
            "st" => Ok(if args.first().map_or(false, |a| a.is_string()) { args[0] } else { Value::NONE }),
            "nm" => Ok(if args.first().map_or(false, |a| a.as_i64().is_some() || a.float_id().is_some()) { args[0] } else { Value::NONE }),
            "ob" => Ok(if args.first().map_or(false, |a| a.is_object() || a.is_host()) { args[0] } else { Value::NONE }),
            "ar" => Ok(if args.first().map_or(false, |a| a.is_array()) { args[0] } else { Value::NONE }),
            "bt" => Ok(if args.first().map_or(false, |a| a.as_bool().is_some()) { args[0] } else { Value::NONE }),
            // Built-in methods (target is args[0])
            "pu" => self.op_push(args),
            "po" => self.op_pop(args),
            "jn" => self.op_join(args),
            "ix" => self.op_index_of(args),
            "cn" => self.op_contains(args),
            "sl" => self.op_slice(args),
            "sp" => self.op_split(args),
            "tm" => self.op_trim(args),
            "sw" => self.op_starts_with(args),
            "ew" => self.op_ends_with(args),
            "uc" => self.op_upper(args),
            "lc" => self.op_lower(args),
            "rp" => self.op_replace(args),
            "bS" => self.op_blob_slice(args),
            _ => Ok(Value::NONE),
        }
    }

    fn op_add(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.len() < 2 { return Ok(Value::NONE); }
        // Blob concatenation
        if self.heap.is_blob(args[0]) && self.heap.is_blob(args[1]) {
            let a = self.heap.blob_data(args[0]).unwrap().to_vec();
            let b = self.heap.blob_data(args[1]).unwrap().to_vec();
            let mut result = a;
            result.extend_from_slice(&b);
            return Ok(self.heap.alloc_blob(result));
        }
        // String concatenation
        if args[0].is_string() && args[1].is_string() {
            let a = args[0].as_str(&self.heap).unwrap().to_string();
            let b = args[1].as_str(&self.heap).unwrap();
            let result = format!("{a}{b}");
            return Ok(self.heap.intern_value(&result));
        }
        if let (Some(a), Some(b)) = (args[0].as_f64(&self.heap), args[1].as_f64(&self.heap)) {
            let r = a + b;
            if r.fract() == 0.0 && r.abs() < i64::MAX as f64 {
                return Ok(Value::int(r as i64));
            }
            return Ok(self.heap.alloc_float(r));
        }
        Ok(Value::NONE)
    }

    fn op_arith(&mut self, args: &[Value], f: fn(f64, f64) -> f64) -> Result<Value, RexError> {
        if args.len() < 2 { return Ok(Value::NONE); }
        if let (Some(a), Some(b)) = (args[0].as_f64(&self.heap), args[1].as_f64(&self.heap)) {
            let r = f(a, b);
            if r.fract() == 0.0 && r.abs() < i64::MAX as f64 && !r.is_nan() {
                return Ok(Value::int(r as i64));
            }
            return Ok(self.heap.alloc_float(r));
        }
        Ok(Value::NONE)
    }

    fn op_eq(&self, args: &[Value], negate: bool) -> Result<Value, RexError> {
        if args.len() < 2 { return Ok(Value::NONE); }
        let equal = values_deep_equal(args[0], args[1], &self.heap);
        let matches = if negate { !equal } else { equal };
        if matches { Ok(args[0]) } else { Ok(Value::NONE) }
    }

    fn op_compare(&self, args: &[Value], pred: fn(std::cmp::Ordering) -> bool) -> Result<Value, RexError> {
        if args.len() < 2 { return Ok(Value::NONE); }
        let ord = if let (Some(a), Some(b)) = (args[0].as_i64(), args[1].as_i64()) {
            a.cmp(&b)
        } else if let (Some(fa), Some(fb)) = (args[0].as_f64(&self.heap), args[1].as_f64(&self.heap)) {
            fa.partial_cmp(&fb).unwrap_or(std::cmp::Ordering::Equal)
        } else if let (Some(a), Some(b)) = (args[0].as_str(&self.heap), args[1].as_str(&self.heap)) {
            a.cmp(b)
        } else {
            return Ok(Value::NONE);
        };
        if pred(ord) { Ok(args[0]) } else { Ok(Value::NONE) }
    }

    // ── Built-in methods ─────────────────────────────────────────

    fn op_push(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.len() < 2 { return Ok(Value::NONE); }
        let arr = args[0];
        self.heap.array_push(arr, args[1]);
        Ok(arr)
    }

    fn op_pop(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.is_empty() { return Ok(Value::NONE); }
        let arr = args[0];
        if let Some(id) = arr.array_id() {
            let vec = &mut self.heap.arrays[id as usize];
            if vec.is_empty() { return Ok(Value::NONE); }
            Ok(vec.pop().unwrap())
        } else {
            Ok(Value::NONE)
        }
    }

    fn op_join(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.is_empty() { return Ok(Value::NONE); }
        let arr = args[0];
        let sep = args.get(1).and_then(|v| v.as_str(&self.heap)).unwrap_or(",");
        let sep = sep.to_string();
        let items = self.heap.array_items(arr);
        let parts: Vec<String> = items.iter().map(|&v| {
            if let Some(s) = v.as_str(&self.heap) { s.to_string() }
            else if let Some(n) = v.as_i64() { n.to_string() }
            else if let Some(f) = v.as_f64(&self.heap) { f.to_string() }
            else if v.is_null() { "null".into() }
            else if v.is_none() { String::new() }
            else if let Some(b) = v.as_bool() { b.to_string() }
            else { String::new() }
        }).collect();
        Ok(self.heap.intern_value(&parts.join(&sep)))
    }

    fn op_index_of(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.len() < 2 { return Ok(Value::NONE); }
        let target = args[0];
        let needle = args[1];
        if target.is_array() {
            let items = self.heap.array_items(target);
            for (i, &item) in items.iter().enumerate() {
                if values_equal(item, needle, &self.heap) {
                    return Ok(Value::int(i as i64));
                }
            }
            Ok(Value::NONE)
        } else if target.is_string() {
            let s = target.as_str(&self.heap).unwrap().to_string();
            if let Some(needle_s) = needle.as_str(&self.heap) {
                match s.find(needle_s) {
                    Some(pos) => Ok(Value::int(s[..pos].chars().count() as i64)),
                    None => Ok(Value::NONE),
                }
            } else {
                Ok(Value::NONE)
            }
        } else {
            Ok(Value::NONE)
        }
    }

    fn op_contains(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.len() < 2 { return Ok(Value::NONE); }
        let target = args[0];
        let needle = args[1];
        if target.is_array() {
            let items = self.heap.array_items(target);
            for &item in items {
                if values_equal(item, needle, &self.heap) {
                    return Ok(needle);
                }
            }
            Ok(Value::NONE)
        } else if target.is_string() {
            let s = target.as_str(&self.heap).unwrap().to_string();
            if let Some(needle_s) = needle.as_str(&self.heap) {
                if s.contains(needle_s) { Ok(needle) } else { Ok(Value::NONE) }
            } else {
                Ok(Value::NONE)
            }
        } else {
            Ok(Value::NONE)
        }
    }

    fn op_slice(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.is_empty() { return Ok(Value::NONE); }
        let target = args[0];
        let start = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as usize;
        let end = args.get(2).and_then(|v| v.as_i64());
        if target.is_array() {
            let items = self.heap.array_items(target);
            let end = end.map(|e| e as usize).unwrap_or(items.len());
            let end = end.min(items.len());
            let start = start.min(end);
            let sliced: Vec<Value> = items[start..end].to_vec();
            Ok(self.heap.alloc_array(sliced))
        } else if target.is_string() {
            let s = target.as_str(&self.heap).unwrap().to_string();
            let chars: Vec<char> = s.chars().collect();
            let end = end.map(|e| e as usize).unwrap_or(chars.len());
            let end = end.min(chars.len());
            let start = start.min(end);
            let sliced: String = chars[start..end].iter().collect();
            Ok(self.heap.intern_value(&sliced))
        } else {
            Ok(Value::NONE)
        }
    }

    fn op_split(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.is_empty() { return Ok(Value::NONE); }
        let s = match args[0].as_str(&self.heap) {
            Some(s) => s.to_string(),
            None => return Ok(Value::NONE),
        };
        let sep = args.get(1).and_then(|v| v.as_str(&self.heap)).unwrap_or(",");
        let sep = sep.to_string();
        let parts: Vec<Value> = s.split(&sep).map(|p| self.heap.intern_value(p)).collect();
        Ok(self.heap.alloc_array(parts))
    }

    fn op_trim(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.is_empty() { return Ok(Value::NONE); }
        let s = match args[0].as_str(&self.heap) { Some(s) => s.trim().to_string(), None => return Ok(Value::NONE) };
        Ok(self.heap.intern_value(&s))
    }

    fn op_starts_with(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.len() < 2 { return Ok(Value::NONE); }
        let s = match args[0].as_str(&self.heap) { Some(s) => s.to_string(), None => return Ok(Value::NONE) };
        let prefix = match args[1].as_str(&self.heap) { Some(s) => s.to_string(), None => return Ok(Value::NONE) };
        if s.starts_with(&prefix) { Ok(args[0]) } else { Ok(Value::NONE) }
    }

    fn op_ends_with(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.len() < 2 { return Ok(Value::NONE); }
        let s = match args[0].as_str(&self.heap) { Some(s) => s.to_string(), None => return Ok(Value::NONE) };
        let suffix = match args[1].as_str(&self.heap) { Some(s) => s.to_string(), None => return Ok(Value::NONE) };
        if s.ends_with(&suffix) { Ok(args[0]) } else { Ok(Value::NONE) }
    }

    fn op_upper(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.is_empty() { return Ok(Value::NONE); }
        match args[0].as_str(&self.heap) {
            Some(s) => Ok(self.heap.intern_value(&s.to_uppercase())),
            None => Ok(Value::NONE),
        }
    }

    fn op_lower(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.is_empty() { return Ok(Value::NONE); }
        match args[0].as_str(&self.heap) {
            Some(s) => Ok(self.heap.intern_value(&s.to_lowercase())),
            None => Ok(Value::NONE),
        }
    }

    fn op_replace(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.len() < 3 { return Ok(Value::NONE); }
        let s = match args[0].as_str(&self.heap) { Some(s) => s.to_string(), None => return Ok(Value::NONE) };
        let from = match args[1].as_str(&self.heap) { Some(s) => s.to_string(), None => return Ok(Value::NONE) };
        let to = match args[2].as_str(&self.heap) { Some(s) => s.to_string(), None => return Ok(Value::NONE) };
        Ok(self.heap.intern_value(&s.replacen(&from, &to, 1)))
    }

    fn op_blob_slice(&mut self, args: &[Value]) -> Result<Value, RexError> {
        if args.len() < 3 { return Ok(Value::NONE); }
        let data = match self.heap.blob_data(args[0]) {
            Some(d) => d.to_vec(),
            None => return Ok(Value::NONE),
        };
        let start = args[1].as_i64().unwrap_or(0) as usize;
        let end = args[2].as_i64().unwrap_or(data.len() as i64) as usize;
        let start = start.min(data.len());
        let end = end.min(data.len()).max(start);
        Ok(self.heap.alloc_blob(data[start..end].to_vec()))
    }

    fn op_bitwise(&self, args: &[Value], f: fn(i64, i64) -> i64) -> Result<Value, RexError> {
        if args.len() < 2 { return Ok(Value::NONE); }
        let a = args[0].as_i64()
            .or_else(|| args[0].as_bool().map(|b| b as i64))
            .or_else(|| args[0].to_i64(&self.heap));
        let b = args[1].as_i64()
            .or_else(|| args[1].as_bool().map(|b| b as i64))
            .or_else(|| args[1].to_i64(&self.heap));
        if let (Some(a), Some(b)) = (a, b) {
            let r = f(a, b);
            // If both inputs were booleans, return boolean
            if args[0].as_bool().is_some() && args[1].as_bool().is_some() {
                Ok(Value::bool(r != 0))
            } else {
                Ok(Value::int(r))
            }
        } else {
            Ok(Value::NONE)
        }
    }
}

// ── Method dispatch tables ─────────────────────────────────────────────

fn array_method(name: &str) -> Option<&'static str> {
    match name {
        "push" => Some("%pu"),
        "pop" => Some("%po"),
        "join" => Some("%jn"),
        "indexOf" => Some("%ix"),
        "contains" => Some("%cn"),
        "slice" => Some("%sl"),
        _ => None,
    }
}

fn string_method(name: &str) -> Option<&'static str> {
    match name {
        "indexOf" => Some("%ix"),
        "contains" => Some("%cn"),
        "slice" => Some("%sl"),
        "split" => Some("%sp"),
        "trim" => Some("%tm"),
        "starts-with" => Some("%sw"),
        "ends-with" => Some("%ew"),
        "upper" => Some("%uc"),
        "lower" => Some("%lc"),
        "replace" => Some("%rp"),
        _ => None,
    }
}

fn values_deep_equal(a: Value, b: Value, heap: &Heap) -> bool {
    if a == b { return true; }
    if let (Some(ai), Some(bi)) = (a.as_i64(), b.as_i64()) { return ai == bi; }
    if let (Some(af), Some(bf)) = (a.as_f64(heap), b.as_f64(heap)) { return af == bf; }
    if let (Some(sa), Some(sb)) = (a.as_str(heap), b.as_str(heap)) { return sa == sb; }
    if let (Some(ba), Some(bb)) = (a.as_bool(), b.as_bool()) { return ba == bb; }
    if a.is_null() && b.is_null() { return true; }
    if a.is_none() && b.is_none() { return true; }
    if a.is_array() && b.is_array() {
        let aa = heap.array_items(a);
        let ba = heap.array_items(b);
        return aa.len() == ba.len()
            && aa.iter().zip(ba.iter()).all(|(&x, &y)| values_deep_equal(x, y, heap));
    }
    if a.is_object() && b.is_object() {
        let ap = heap.object_pairs(a);
        let bp = heap.object_pairs(b);
        return ap.len() == bp.len()
            && ap.iter().zip(bp.iter()).all(|(&(ak, av), &(bk, bv))| {
                ak == bk && values_deep_equal(av, bv, heap)
            });
    }
    false
}

fn values_equal(a: Value, b: Value, heap: &Heap) -> bool {
    if a == b { return true; } // same handle
    if let (Some(ai), Some(bi)) = (a.as_i64(), b.as_i64()) { return ai == bi; }
    if let (Some(af), Some(bf)) = (a.as_f64(heap), b.as_f64(heap)) { return af == bf; }
    if let (Some(sa), Some(sb)) = (a.as_str(heap), b.as_str(heap)) { return sa == sb; }
    if let (Some(ba), Some(bb)) = (a.as_bool(), b.as_bool()) { return ba == bb; }
    a.is_null() && b.is_null()
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(source: &str) -> (Value, Heap) {
        let bytecode = crate::compile(source);
        let result = run(&bytecode, Context::default()).unwrap();
        (result.value, result.heap)
    }

    #[test]
    fn eval_integer() {
        let (v, _) = eval("42");
        assert_eq!(v.as_i64(), Some(42));
        let (v, _) = eval("0");
        assert_eq!(v.as_i64(), Some(0));
        let (v, _) = eval("-1");
        assert_eq!(v.as_i64(), Some(-1));
    }

    #[test]
    fn eval_string() {
        let (v, heap) = eval(r#""hello""#);
        assert_eq!(v.as_str(&heap), Some("hello"));
    }

    #[test]
    fn eval_bool_null() {
        let (v, _) = eval("true");
        assert_eq!(v.as_bool(), Some(true));
        let (v, _) = eval("false");
        assert_eq!(v.as_bool(), Some(false));
        let (v, _) = eval("null");
        assert!(v.is_null());
        let (v, _) = eval("none");
        assert!(v.is_none());
    }

    #[test]
    fn eval_addition() {
        let (v, _) = eval("1 + 2");
        assert_eq!(v.as_i64(), Some(3));
    }

    #[test]
    fn eval_arithmetic() {
        let (v, _) = eval("10 - 3");
        assert_eq!(v.as_i64(), Some(7));
        let (v, _) = eval("4 * 5");
        assert_eq!(v.as_i64(), Some(20));
        let (v, _) = eval("10 / 2");
        assert_eq!(v.as_i64(), Some(5));
        let (v, _) = eval("7 % 3");
        assert_eq!(v.as_i64(), Some(1));
    }

    #[test]
    fn eval_string_concat() {
        let (v, heap) = eval(r#""hello" + " " + "world""#);
        assert_eq!(v.as_str(&heap), Some("hello world"));
    }

    #[test]
    fn eval_comparison() {
        let (v, _) = eval("5 > 3");
        assert!(v.is_defined());
        let (v, _) = eval("3 > 5");
        assert!(!v.is_defined());
        let (v, _) = eval("5 == 5");
        assert!(v.is_defined());
        let (v, _) = eval("5 == 6");
        assert!(!v.is_defined());
    }

    #[test]
    fn eval_assignment() {
        let bc = crate::compile("x = 42\nx");
        let result = run(&bc, Context::default()).unwrap();
        assert_eq!(result.value.as_i64(), Some(42));
    }

    #[test]
    fn eval_when() {
        let (v, _) = eval("when true do 42 end");
        assert_eq!(v.as_i64(), Some(42));
        let (v, _) = eval("when none do 42 end");
        assert!(v.is_none());
    }

    #[test]
    fn eval_when_else() {
        let (v, _) = eval("when true do 1 else 2 end");
        assert_eq!(v.as_i64(), Some(1));
        let (v, _) = eval("when none do 1 else 2 end");
        assert_eq!(v.as_i64(), Some(2));
    }

    #[test]
    fn eval_or() {
        let (v, _) = eval("none or 42");
        assert_eq!(v.as_i64(), Some(42));
        let (v, _) = eval("1 or 42");
        assert_eq!(v.as_i64(), Some(1));
    }

    #[test]
    fn eval_and() {
        let (v, _) = eval("1 and 2");
        assert_eq!(v.as_i64(), Some(2));
        let (v, _) = eval("none and 2");
        assert!(!v.is_defined());
    }

    #[test]
    fn eval_block() {
        let bc = crate::compile("x = 1\ny = 2\nx + y");
        let result = run(&bc, Context::default()).unwrap();
        assert_eq!(result.value.as_i64(), Some(3));
    }

    #[test]
    fn eval_data_array() {
        let (v, heap) = eval("[1, 2, 3]");
        assert_eq!(v.type_name(&heap), "array");
        assert_eq!(heap.array_len(v), 3);
    }

    #[test]
    fn eval_range() {
        let (v, heap) = eval("[v * 2 for v in [1, 2, 3]]");
        assert!(v.is_array());
        assert_eq!(heap.array_len(v), 3);
    }

    #[test]
    fn eval_template_no_interpolation() {
        let (v, heap) = eval("`hello`");
        assert_eq!(v.as_str(&heap), Some("hello"));
    }

    #[test]
    fn eval_template_with_variable() {
        let bc = crate::compile("name = `world`\n`hello ${name}`");
        let result = run(&bc, Context::default()).unwrap();
        assert_eq!(result.value.as_str(&result.heap), Some("hello world"));
    }

    #[test]
    fn eval_template_with_integer() {
        let bc = crate::compile("x = 42\n`the answer is ${x}`");
        let result = run(&bc, Context::default()).unwrap();
        assert_eq!(result.value.as_str(&result.heap), Some("the answer is 42"));
    }

    #[test]
    fn eval_template_with_bool() {
        let bc = crate::compile("`value: ${true}`");
        let result = run(&bc, Context::default()).unwrap();
        assert_eq!(result.value.as_str(&result.heap), Some("value: \u{2713}"));
    }

    #[test]
    fn eval_template_with_none() {
        let bc = crate::compile("`got: ${name}`");
        let result = run(&bc, Context::default()).unwrap();
        assert_eq!(result.value.as_str(&result.heap), Some("got: \u{2205}"));
    }

    #[test]
    fn eval_gas_limit() {
        let bc = crate::compile("while true do 1 end");
        let mut ctx = Context::default();
        ctx.gas_limit = 100;
        let result = run(&bc, ctx);
        assert!(matches!(result, Err(RexError::GasLimitExceeded)));
    }

    #[test]
    fn eval_return() {
        let (v, _) = eval("return 42");
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn eval_bare_return() {
        let (v, _) = eval("return");
        assert!(v.is_none());
    }

    #[test]
    fn eval_return_halts() {
        let (v, _) = eval("return 1\n99");
        assert_eq!(v.as_i64(), Some(1));
    }

    #[test]
    fn eval_return_in_when() {
        let (v, _) = eval("when true do return 1 end\n2");
        assert_eq!(v.as_i64(), Some(1));
    }

    #[test]
    fn eval_return_in_when_skipped() {
        let (v, _) = eval("when none do return 1 end\n2");
        assert_eq!(v.as_i64(), Some(2));
    }

    #[test]
    fn eval_return_in_unless() {
        let (v, _) = eval("unless none do return 42 end\n99");
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn eval_return_in_loop() {
        let bc = crate::compile("x = 0\nwhile true do\n  x = x + 1\n  when x == 5 do return x end\nend\n99");
        let mut ctx = Context::default();
        ctx.gas_limit = 10000;
        let result = run(&bc, ctx).unwrap();
        assert_eq!(result.value.as_i64(), Some(5));
    }

    // ── Length-prefix skip tests ───────────────────────────────────────

    #[test]
    fn skip_length_prefixed_then_branch() {
        let (v, _) = eval("x = none\nwhen x do\n  99\nelse\n  42\nend");
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn skip_length_prefixed_else_branch() {
        let (v, _) = eval("x = 1\nwhen x do\n  42\nelse\n  99\nend");
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn skip_unless_length_prefixed() {
        let (v, _) = eval("unless true do 99 end");
        assert!(v.is_none());
        let (v, _) = eval("unless none do 42 end");
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn skip_or_length_prefixed() {
        let bc = crate::compile("x = 1\nx or [1, 2, 3]");
        let result = run(&bc, Context::default()).unwrap();
        assert_eq!(result.value.as_i64(), Some(1));
    }

    #[test]
    fn skip_and_length_prefixed() {
        let (v, _) = eval("none and [1, 2, 3]");
        assert!(!v.is_defined());
    }

    #[test]
    fn cross_branch_dedup_safe() {
        let (v, _) = eval("x = none\nunless x do y = 401 end\nwhen x do\n  unless x do y = 401 end\nend\ny");
        assert_eq!(v.as_i64(), Some(401));
    }

    #[test]
    fn nested_when_skip() {
        let (v, _) = eval("x = none\nwhen x do\n  when x do 1 else 2 end\nelse\n  when true do 42 else 99 end\nend");
        assert_eq!(v.as_i64(), Some(42));
    }

    // ── Indexed array tests ───────────────────────────────────────────

    #[test]
    fn eval_indexed_array() {
        use crate::bytecode::encode_indexed_array;

        let items = vec![
            crate::bytecode::Value::Integer(1),
            crate::bytecode::Value::Integer(2),
            crate::bytecode::Value::Integer(3),
        ];
        let bc = encode_indexed_array(&items);
        let result = run(&bc, Context::default()).unwrap();
        assert!(result.value.is_array());
        assert_eq!(result.heap.array_len(result.value), 3);
        assert_eq!(result.heap.array_get(result.value, 0).as_i64(), Some(1));
        assert_eq!(result.heap.array_get(result.value, 1).as_i64(), Some(2));
        assert_eq!(result.heap.array_get(result.value, 2).as_i64(), Some(3));
    }

    #[test]
    fn eval_indexed_array_with_strings() {
        use crate::bytecode::encode_indexed_array;

        let items = vec![
            crate::bytecode::Value::String("hello".into()),
            crate::bytecode::Value::String("world".into()),
        ];
        let bc = encode_indexed_array(&items);
        let result = run(&bc, Context::default()).unwrap();
        assert!(result.value.is_array());
        assert_eq!(result.heap.array_len(result.value), 2);
        assert_eq!(result.heap.array_get(result.value, 0).as_str(&result.heap), Some("hello"));
        assert_eq!(result.heap.array_get(result.value, 1).as_str(&result.heap), Some("world"));
    }

    #[test]
    fn eval_indexed_object() {
        use crate::bytecode::{encode_indexed_object, Value as BValue};

        let pairs = vec![
            (BValue::String("name".into()), BValue::String("Ada".into())),
            (BValue::String("score".into()), BValue::Integer(95)),
        ];
        let bc = encode_indexed_object(&pairs);
        let result = run(&bc, Context::default()).unwrap();
        assert!(result.value.is_object());
        let pairs = result.heap.object_pairs(result.value);
        assert_eq!(pairs.len(), 2);
    }

    // ── Regression tests for known bugs ────────────────────────────────

    #[test]
    fn for_in_range_binding() {
        let (v, _) = eval("for v in 1..3 do v end");
        assert_eq!(v.as_i64(), Some(3), "expected Int(3), got {:?}", v);
    }

    #[test]
    fn comprehension_with_bare_variable() {
        let (v, heap) = eval("[v for v in [10, 20, 30]]");
        assert!(v.is_array());
        assert_eq!(heap.array_len(v), 3);
    }

    #[test]
    fn comprehension_filtering() {
        let (v, heap) = eval("[v % 2 == 0 and v for v in [1, 2, 3, 4, 5]]");
        assert!(v.is_array());
        assert_eq!(heap.array_len(v), 2);
        assert_eq!(heap.array_get(v, 0).as_i64(), Some(2));
        assert_eq!(heap.array_get(v, 1).as_i64(), Some(4));
    }

    // ── Object mutation tests (previously known bugs, now fixed) ───────

    #[test]
    fn object_property_mutation() {
        let (v, heap) = eval("obj = {x: 1}\nobj.x = 2\nobj.x");
        assert_eq!(v.as_i64(), Some(2), "object mutation should work, got {:?}", v);
    }

    #[test]
    fn object_dynamic_key_mutation() {
        let (v, _) = eval("obj = {}\nobj.(4) = true\nobj.(4)");
        assert_eq!(v.as_bool(), Some(true), "dynamic key mutation should work, got {:?}", v);
    }

    #[test]
    fn object_none_value_omits_key() {
        let (v, heap) = eval("c = { a: 3 > 2 b: 3 > 5 }\nc");
        assert!(v.is_object());
        let pairs = heap.object_pairs(v);
        assert_eq!(pairs.len(), 1, "expected only key 'a', got {pairs:?}");
        let key = heap.resolve_str(pairs[0].0);
        assert_eq!(key, "a");
        assert_eq!(pairs[0].1.as_i64(), Some(3));
    }

    #[test]
    fn object_null_value_preserves_key() {
        let (v, heap) = eval("c = { a: 3 > 2 b: null }\nc");
        assert!(v.is_object());
        let pairs = heap.object_pairs(v);
        assert_eq!(pairs.len(), 2, "expected keys 'a' and 'b', got {pairs:?}");
        let mut saw_a = false;
        let mut saw_b = false;
        for (k, val) in pairs {
            match heap.resolve_str(*k) {
                "a" => {
                    saw_a = true;
                    assert_eq!(val.as_i64(), Some(3));
                }
                "b" => {
                    saw_b = true;
                    assert!(val.is_null());
                }
                _ => {}
            }
        }
        assert!(saw_a && saw_b, "expected both keys present");
    }

    // ── Semicolons (compound expressions) ─────────────────────────────

    #[test]
    fn semicolon_compound() {
        let (v, _) = eval("1; 2; 3");
        assert_eq!(v.as_i64(), Some(3));
    }

    #[test]
    fn semicolon_forces_boundary() {
        // a; -b is two exprs (a then negate b), not a - b
        let (v, _) = eval("10; -3");
        assert_eq!(v.as_i64(), Some(-3));
        // contrast with subtraction
        let (v, _) = eval("10 - 3");
        assert_eq!(v.as_i64(), Some(7));
    }

    #[test]
    fn semicolon_in_condition() {
        let (v, _) = eval("x = 1; when x; x + 1 do 42 end");
        assert_eq!(v.as_i64(), Some(42));
    }

    // ── Array comprehensions ──────────────────────────────────────────

    #[test]
    fn array_comp_map() {
        let (v, heap) = eval("[ v * 2 for v in [ 1, 2, 3 ] ]");
        assert!(v.is_array());
        assert_eq!(heap.array_len(v), 3);
        assert_eq!(heap.array_get(v, 0).as_i64(), Some(2));
        assert_eq!(heap.array_get(v, 2).as_i64(), Some(6));
    }

    #[test]
    fn array_comp_filter() {
        let (v, heap) = eval("[ v >= 3 and v for v in [ 1, 2, 3, 4, 5 ] ]");
        assert_eq!(heap.array_len(v), 3);
        assert_eq!(heap.array_get(v, 0).as_i64(), Some(3));
    }

    #[test]
    fn array_comp_with_index() {
        let (v, heap) = eval("[ i for i, v in [ 10, 20, 30 ] ]");
        assert_eq!(heap.array_len(v), 3);
        assert_eq!(heap.array_get(v, 0).as_i64(), Some(0));
        assert_eq!(heap.array_get(v, 2).as_i64(), Some(2));
    }

    #[test]
    fn array_comp_for_of() {
        let (v, heap) = eval("[ k for k of { a: 1 b: 2 } ]");
        assert_eq!(heap.array_len(v), 2);
        assert_eq!(heap.array_get(v, 0).as_str(&heap), Some("a"));
    }

    #[test]
    fn array_comp_while() {
        let (v, heap) = eval("x = 1; [ x = x * 2 while x < 100 ]");
        assert!(v.is_array());
        assert_eq!(heap.array_get(v, 0).as_i64(), Some(2));
        // Last element should be < 200 (doubled once more before condition fails)
        let last = heap.array_get(v, heap.array_len(v) - 1);
        assert!(last.as_i64().unwrap() <= 128);
    }

    #[test]
    fn array_comp_multi_expr_body() {
        let (v, heap) = eval("a = 0; b = 1\n[ c = a + b\n  a = b\n  b = c\n  while a <= 20 ]");
        assert!(v.is_array());
        assert_eq!(heap.array_get(v, 0).as_i64(), Some(1));
        assert_eq!(heap.array_get(v, 1).as_i64(), Some(2));
        assert_eq!(heap.array_get(v, 2).as_i64(), Some(3));
        assert_eq!(heap.array_get(v, 3).as_i64(), Some(5));
    }

    // ── Object comprehensions ─────────────────────────────────────────

    #[test]
    fn object_comp_basic() {
        let (v, mut heap) = eval("{ (k): v * 10 for k, v in { a: 1 b: 2 } }");
        assert!(v.is_object());
        let k_a = heap.intern("a");
        let k_b = heap.intern("b");
        assert_eq!(heap.object_get(v, k_a).as_i64(), Some(10));
        assert_eq!(heap.object_get(v, k_b).as_i64(), Some(20));
    }

    #[test]
    fn object_comp_from_array() {
        let (v, mut heap) = eval("{ (u.name): u.score for u in [ { name: \"Ada\" score: 95 } ] }");
        assert!(v.is_object());
        let k = heap.intern("Ada");
        assert_eq!(heap.object_get(v, k).as_i64(), Some(95));
    }

    #[test]
    fn object_comp_filter_value() {
        let (v, mut heap) = eval("{ (k): v >= 2 and v for k, v in { a: 1 b: 2 c: 3 } }");
        assert_eq!(heap.object_len(v), 2); // a excluded
        let k_b = heap.intern("b");
        assert_eq!(heap.object_get(v, k_b).as_i64(), Some(2));
    }

    #[test]
    fn object_comp_filter_key() {
        let (v, mut heap) = eval("{ (k == \"a\" and k): v for k, v in { a: 1 b: 2 } }");
        assert_eq!(heap.object_len(v), 1);
        let k_a = heap.intern("a");
        assert_eq!(heap.object_get(v, k_a).as_i64(), Some(1));
    }

    // ── For loops with key-value iteration ────────────────────────────

    #[test]
    fn for_kv_object_keys() {
        let (v, heap) = eval("[ k for k, v in { x: 1 y: 2 } ]");
        assert_eq!(heap.array_len(v), 2);
        assert_eq!(heap.array_get(v, 0).as_str(&heap), Some("x"));
        assert_eq!(heap.array_get(v, 1).as_str(&heap), Some("y"));
    }

    #[test]
    fn for_kv_object_values() {
        let (v, heap) = eval("[ v for k, v in { x: 1 y: 2 } ]");
        assert_eq!(heap.array_get(v, 0).as_i64(), Some(1));
        assert_eq!(heap.array_get(v, 1).as_i64(), Some(2));
    }

    #[test]
    fn for_kv_array_indices() {
        let (v, heap) = eval("[ i for i, v in [ 10, 20, 30 ] ]");
        assert_eq!(heap.array_get(v, 0).as_i64(), Some(0));
        assert_eq!(heap.array_get(v, 1).as_i64(), Some(1));
    }

    // ── Control flow ──────────────────────────────────────────────────

    #[test]
    fn unless() {
        let (v, _) = eval("unless none do 42 end");
        assert_eq!(v.as_i64(), Some(42));
        let (v, _) = eval("unless true do 42 end");
        assert!(!v.is_defined());
    }

    #[test]
    fn when_unless_chain() {
        let (v, _) = eval("when a do 1 else unless b do 2 else 3 end");
        // a=none, b=none → unless body (2)
        assert_eq!(v.as_i64(), Some(2));
    }

    #[test]
    fn while_loop() {
        let (v, _) = eval("x = 0\nwhile x < 5 do x = x + 1 end\nx");
        assert_eq!(v.as_i64(), Some(5));
    }

    #[test]
    fn break_in_loop() {
        let (v, _) = eval("x = 0\nwhile true do\n  x = x + 1\n  when x == 3 do break end\nend\nx");
        assert_eq!(v.as_i64(), Some(3));
    }

    #[test]
    fn continue_in_loop() {
        // Sum only even numbers 1..5 using continue
        let (v, _) = eval("sum = 0\nfor v in 1..5 do\n  unless v % 2 == 0 do continue end\n  sum = sum + v\nend\nsum");
        assert_eq!(v.as_i64(), Some(6)); // 2 + 4
    }

    // ── Existence logic ───────────────────────────────────────────────

    #[test]
    fn or_returns_first_defined() {
        let (v, _) = eval("none or none or 42");
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn and_returns_last_if_all_defined() {
        let (v, _) = eval("1 and 2 and 3");
        assert_eq!(v.as_i64(), Some(3));
    }

    #[test]
    fn and_returns_none_if_any_undefined() {
        let (v, _) = eval("1 and none and 3");
        assert!(!v.is_defined());
    }

    // ── Navigation ────────────────────────────────────────────────────

    #[test]
    fn static_nav() {
        let (v, _) = eval("{ a: { b: 42 } }.a.b");
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn dynamic_nav() {
        let (v, _) = eval("obj = { x: 1 }\nkey = \"x\"\nobj.(key)");
        assert_eq!(v.as_i64(), Some(1));
    }

    #[test]
    fn array_index() {
        let (v, _) = eval("[ 10, 20, 30 ].1");
        assert_eq!(v.as_i64(), Some(20));
    }

    #[test]
    fn array_size() {
        let (v, _) = eval("[ 1, 2, 3 ].size");
        assert_eq!(v.as_i64(), Some(3));
    }

    #[test]
    fn string_size() {
        let (v, _) = eval("\"hello\".size");
        assert_eq!(v.as_i64(), Some(5));
    }
}

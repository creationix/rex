//! Zero-copy cursor interpreter for REXC/RX bytecode.
//!
//! Evaluates bytecode in-place without deserializing to a `Value` tree.
//! Host objects provide mutable proxy behavior via the `HostObject` trait.

use std::collections::HashMap;

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
    ReturnSignal(RexValue),
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

// ── Runtime values ──────────────────────────────────────────────────────

/// Runtime value produced by the interpreter.
#[derive(Debug, Clone)]
pub enum RexValue {
    RexNone,
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Decimal { sig: i64, exp: i64 },
    Str(String),
    Array(Vec<RexValue>),
    Object(Vec<(String, RexValue)>),
    /// Index into the interpreter's host_objects vec.
    Host(usize),
}

impl RexValue {
    pub fn is_defined(&self) -> bool {
        !matches!(self, RexValue::RexNone)
    }

    pub fn to_f64(&self) -> Option<f64> {
        match self {
            RexValue::Int(n) => Some(*n as f64),
            RexValue::Float(n) => Some(*n),
            RexValue::Decimal { sig, exp } => {
                Some(*sig as f64 * 10f64.powi(*exp as i32))
            }
            _ => None,
        }
    }

    pub fn to_i64(&self) -> Option<i64> {
        match self {
            RexValue::Int(n) => Some(*n),
            RexValue::Float(n) if n.fract() == 0.0 => Some(*n as i64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            RexValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            RexValue::RexNone => "none",
            RexValue::Null => "null",
            RexValue::Bool(_) => "boolean",
            RexValue::Int(_) | RexValue::Float(_) | RexValue::Decimal { .. } => "number",
            RexValue::Str(_) => "string",
            RexValue::Array(_) => "array",
            RexValue::Object(_) => "object",
            RexValue::Host(_) => "object",
        }
    }
}

// ── Host object trait ───────────────────────────────────────────────────

/// Host-provided proxy object with custom read/write/call behavior.
pub trait HostObject {
    fn get(&self, key: &str) -> Option<RexValue>;
    fn get_index(&self, index: usize) -> Option<RexValue>;
    fn set(&mut self, key: &str, value: RexValue) -> Result<(), RexError>;
    fn call(&mut self, method: &str, args: &[RexValue]) -> Result<RexValue, RexError>;
    fn delete(&mut self, key: &str) -> Result<(), RexError> { let _ = key; Ok(()) }
    fn len(&self) -> Option<usize> { None }
    fn iter_values(&self) -> Option<Vec<RexValue>> { None }
    fn iter_keys(&self) -> Option<Vec<RexValue>> { None }
    fn iter_pairs(&self) -> Option<Vec<(RexValue, RexValue)>> { None }
    fn as_string(&self) -> Option<String> { None }
    fn as_number(&self) -> Option<f64> { None }
    fn as_bool(&self) -> Option<bool> { None }
}

// ── Context ─────────────────────────────────────────────────────────────

/// Execution context provided by the host.
pub struct Context<'a> {
    pub refs: HashMap<String, RexValue>,
    pub vars: HashMap<String, RexValue>,
    pub host_objects: Vec<&'a mut dyn HostObject>,
    pub opcodes: HashMap<String, fn(&[RexValue]) -> Result<RexValue, RexError>>,
    pub gas_limit: u64,
}

impl<'a> Default for Context<'a> {
    fn default() -> Self {
        Self {
            refs: HashMap::new(),
            vars: HashMap::new(),
            host_objects: Vec::new(),
            opcodes: HashMap::new(),
            gas_limit: 0,
        }
    }
}

/// Result of running a Rex program.
pub struct RunResult {
    pub value: RexValue,
    pub vars: HashMap<String, RexValue>,
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
        self_stack: Vec::new(),
        vars: std::mem::take(&mut ctx.vars),
        refs: ctx.refs,
        host_objects: ctx.host_objects,
        opcodes: ctx.opcodes,
        gas: 0,
        gas_limit: ctx.gas_limit,
    };

    let value = interp.eval_top()?;

    Ok(RunResult {
        value,
        vars: interp.vars,
        gas: interp.gas,
    })
}

// ── Interpreter ─────────────────────────────────────────────────────────

struct Interpreter<'a> {
    code: &'a [u8],
    pos: usize,
    self_stack: Vec<RexValue>,
    vars: HashMap<String, RexValue>,
    refs: HashMap<String, RexValue>,
    host_objects: Vec<&'a mut dyn HostObject>,
    opcodes: HashMap<String, fn(&[RexValue]) -> Result<RexValue, RexError>>,
    gas: u64,
    gas_limit: u64,
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

    /// Advance past b64 digits, return the raw slice.
    fn read_raw(&mut self) -> &'a [u8] {
        let start = self.pos;
        while self.pos < self.code.len() && is_b64(self.code[self.pos]) {
            self.pos += 1;
        }
        &self.code[start..self.pos]
    }

    fn read_utf8(&mut self, len: usize) -> String {
        let end = (self.pos + len).min(self.code.len());
        let s = std::str::from_utf8(&self.code[self.pos..end])
            .unwrap_or("")
            .to_string();
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

    fn eval_top(&mut self) -> Result<RexValue, RexError> {
        let mut last = RexValue::RexNone;
        while !self.at_end() {
            match self.eval() {
                Ok(val) => last = val,
                Err(RexError::ReturnSignal(val)) => return self.force_value(val),
                Err(e) => return Err(e),
            }
        }
        // Recursively force nested containers.
        self.force_value(last)
    }

    // ── Main eval dispatch ──────────────────────────────────────────

    fn eval(&mut self) -> Result<RexValue, RexError> {
        if self.at_end() {
            return Ok(RexValue::RexNone);
        }

        let raw = self.read_raw();
        let tag = self.read_byte();

        match tag {
            // Scalars
            b'+' => Ok(RexValue::Int(zigzag_decode(parse_uint(raw)))),
            b'*' => {
                let exp = zigzag_decode(parse_uint(raw));
                // Next value must be an integer (the significand)
                let sig_raw = self.read_raw();
                let sig_tag = self.read_byte();
                if sig_tag != b'+' {
                    return Err(RexError::InvalidBytecode("expected + after *".into()));
                }
                let sig = zigzag_decode(parse_uint(sig_raw));
                Ok(RexValue::Decimal { sig, exp })
            }
            b',' => {
                let len = parse_uint(raw) as usize;
                Ok(RexValue::Str(self.read_utf8(len)))
            }
            b'\'' => {
                let name = Self::raw_to_str(raw);
                Ok(self.resolve_ref(name))
            }
            b'$' => {
                let name = Self::raw_to_str(raw);
                Ok(self.vars.get(name).cloned().unwrap_or(RexValue::RexNone))
            }
            b'%' => {
                // Standalone opcode (type predicate keyword)
                let name = Self::raw_to_str(raw).to_string();
                // When used standalone (not in a call), it's a type predicate
                Ok(RexValue::Str(format!("%{name}")))
            }
            b'@' => {
                let depth = parse_uint(raw) as usize;
                let idx = self.self_stack.len().checked_sub(depth + 1);
                Ok(idx.map(|i| self.self_stack[i].clone()).unwrap_or(RexValue::RexNone))
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
                // Return: ;[value] — raw is always empty (no size prefix)
                let val = self.eval()?;
                Err(RexError::ReturnSignal(val))
            }
            b'^' => {
                // Pointer: seek forward by delta, eval target
                let delta = parse_uint(raw) as usize;
                let target = self.pos + delta;
                let save = self.pos;
                self.pos = target;
                let val = self.eval()?;
                self.pos = save;
                Ok(val)
            }

            // String chain (also used for template literals)
            b'.' => {
                let size = parse_uint(raw) as usize;
                let end = self.pos + size;
                let mut result = String::new();
                while self.pos < end {
                    let seg = self.eval()?;
                    match seg {
                        RexValue::Str(s) => result.push_str(&s),
                        RexValue::Int(n) => result.push_str(&n.to_string()),
                        RexValue::Float(f) => {
                            if f.is_infinite() {
                                result.push('\u{221E}'); // ∞
                            } else if f.is_nan() {
                                result.push_str("NaN");
                            } else {
                                result.push_str(&f.to_string());
                            }
                        }
                        RexValue::Decimal { sig, exp } => {
                            if exp >= 0 {
                                result.push_str(&format!("{}e{}", sig, exp));
                            } else {
                                result.push_str(&format!("{}e{}", sig, exp));
                            }
                        }
                        RexValue::Bool(b) => result.push(if b { '\u{2713}' } else { '\u{2717}' }),
                        RexValue::Null => result.push('\u{2400}'), // ␀
                        RexValue::RexNone => result.push('\u{2205}'), // ∅
                        _ => {} // arrays, objects — skip
                    }
                }
                Ok(RexValue::Str(result))
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
            b'?' => self.eval_when(),
            b'!' => self.eval_unless(),
            b'|' => self.eval_or(),
            b'&' => self.eval_and(),
            b'>' => self.eval_for_in(),
            b'<' => self.eval_for_of(),
            b'#' => self.eval_while(),

            // Mutation
            b'=' => self.eval_set(),
            b'~' => self.eval_delete(),

            0 => Ok(RexValue::RexNone),
            _ => Err(RexError::UnexpectedTag(tag)),
        }
    }

    // ── Eager containers ────────────────────────────────────────────

    fn eval_call(&mut self) -> Result<RexValue, RexError> {
        // Read callee
        let callee = self.eval()?;

        // Read args until ')'
        let mut args = Vec::new();
        while self.peek() != b')' && !self.at_end() {
            args.push(self.eval()?);
        }
        self.read_byte(); // consume ')'

        self.dispatch_call(callee, args)
    }

    fn eval_eager_array(&mut self) -> Result<RexValue, RexError> {
        let mut items = Vec::new();
        while self.peek() != b']' && !self.at_end() {
            items.push(self.eval()?);
        }
        self.read_byte(); // consume ']'
        Ok(RexValue::Array(items))
    }

    /// Check if the next bytes form an index header (b64 digits + '#').
    fn peek_is_index(&self) -> bool {
        let mut i = self.pos;
        while i < self.code.len() && is_b64(self.code[i]) { i += 1; }
        i > self.pos && i < self.code.len() && self.code[i] == b'#'
    }

    /// Evaluate an indexed array: skip the index, eval elements eagerly.
    fn eval_indexed_array(&mut self) -> Result<RexValue, RexError> {
        let raw = self.read_raw();
        let packed = parse_uint(raw);
        self.read_byte(); // consume '#'

        let count = (packed >> 3) as usize;
        let width = ((packed & 7) + 1) as usize;

        // Skip pointer table
        self.pos += count * width;

        // Evaluate elements eagerly
        let mut items = Vec::with_capacity(count);
        while self.peek() != b']' && !self.at_end() {
            items.push(self.eval()?);
        }
        self.read_byte(); // consume ']'
        Ok(RexValue::Array(items))
    }

    /// Evaluate an indexed object: skip the pointer table, eval key-value pairs eagerly.
    fn eval_indexed_object(&mut self) -> Result<RexValue, RexError> {
        let raw = self.read_raw();
        let packed = parse_uint(raw);
        self.read_byte(); // consume '#'

        let count = (packed >> 3) as usize;
        let width = ((packed & 7) + 1) as usize;

        // Skip pointer table
        self.pos += count * width;

        // Read key-value pairs eagerly
        let mut pairs = Vec::with_capacity(count);
        while self.peek() != b'}' && !self.at_end() {
            let k = self.eval()?;
            let v = self.eval()?;
            let k_str = match k {
                RexValue::Str(s) => s,
                _ => format!("{k:?}"),
            };
            pairs.push((k_str, v));
        }
        self.read_byte(); // consume '}'
        Ok(RexValue::Object(pairs))
    }

    fn eval_block(&mut self) -> Result<RexValue, RexError> {
        // In v2, {} is used for both code blocks and data objects.
        // Distinguish by peeking at raw bytecode: if the first element
        // is a string literal (varint + ','), it's an object. If the
        // first element is a pointer (varint + '^') that resolves to an
        // object, it's a schema-shared object. Otherwise it's a block.
        if self.peek() == b'}' {
            self.read_byte(); // empty {}
            return Ok(RexValue::Object(vec![]));
        }

        // Indexed object: <packed>#<pointers><key-value-pairs>
        if self.peek_is_index() {
            return self.eval_indexed_object();
        }

        if self.peek_is_string_literal() {
            return self.eval_object();
        }

        // Check for schema pointer: varint + '^'
        if self.peek_is_pointer() {
            // Eval the pointer — if it resolves to an object, treat as schema-shared
            let first = self.eval()?;
            if let RexValue::Object(schema_pairs) = &first {
                let keys: Vec<String> = schema_pairs.iter().map(|(k, _)| k.clone()).collect();
                let mut pairs = Vec::new();
                for key in &keys {
                    let v = self.eval()?;
                    pairs.push((key.clone(), v));
                }
                self.read_byte(); // consume '}'
                return Ok(RexValue::Object(pairs));
            }
            // Not an object pointer — treat as block, continue
            let mut last = first;
            while self.peek() != b'}' && !self.at_end() {
                last = self.eval()?;
            }
            self.read_byte(); // consume '}'
            return Ok(last);
        }

        // Code block — evaluate expressions, return last
        let mut last = RexValue::RexNone;
        while self.peek() != b'}' && !self.at_end() {
            last = self.eval()?;
        }
        self.read_byte(); // consume '}'
        Ok(last)
    }

    /// Check if the next value in the stream is a string literal (varint + ',').
    fn peek_is_string_literal(&self) -> bool {
        let mut i = self.pos;
        // Skip varint digits
        while i < self.code.len() && is_b64(self.code[i]) {
            i += 1;
        }
        i < self.code.len() && self.code[i] == b','
    }

    /// Check if the next value in the stream is a pointer (varint + '^').
    fn peek_is_pointer(&self) -> bool {
        let mut i = self.pos;
        while i < self.code.len() && is_b64(self.code[i]) {
            i += 1;
        }
        i < self.code.len() && self.code[i] == b'^'
    }

    /// Evaluate an object: alternating string keys and values until '}'.
    fn eval_object(&mut self) -> Result<RexValue, RexError> {
        let mut pairs = Vec::new();
        while self.peek() != b'}' && !self.at_end() {
            let k = self.eval()?;
            let v = self.eval()?;
            let k_str = match k {
                RexValue::Str(s) => s,
                _ => format!("{k:?}"),
            };
            pairs.push((k_str, v));
        }
        self.read_byte(); // consume '}'
        Ok(RexValue::Object(pairs))
    }

    // ── Control flow ────────────────────────────────────────────────

    fn eval_when(&mut self) -> Result<RexValue, RexError> {
        self.read_byte(); // consume '('
        let cond = self.eval()?;
        if cond.is_defined() {
            self.self_stack.push(cond);
            let result = self.eval()?;
            self.self_stack.pop();
            // Skip else branch if present
            if self.peek() != b')' {
                self.skip_value_fast()?;
            }
            self.read_byte(); // ')'
            Ok(result)
        } else {
            self.skip_value_fast()?; // skip then
            let result = if self.peek() != b')' {
                self.eval()?
            } else {
                RexValue::RexNone
            };
            self.read_byte(); // ')'
            Ok(result)
        }
    }

    fn eval_unless(&mut self) -> Result<RexValue, RexError> {
        self.read_byte(); // '('
        let cond = self.eval()?;
        if !cond.is_defined() {
            // Condition is none → execute then branch
            let result = self.eval()?;
            if self.peek() != b')' {
                self.skip_value_fast()?;
            }
            self.read_byte(); // ')'
            Ok(result)
        } else {
            self.skip_value_fast()?; // skip then
            let result = if self.peek() != b')' {
                self.eval()?
            } else {
                RexValue::RexNone
            };
            self.read_byte(); // ')'
            Ok(result)
        }
    }

    fn eval_or(&mut self) -> Result<RexValue, RexError> {
        self.read_byte(); // '('
        let left = self.eval()?;
        if left.is_defined() {
            // Skip right
            if self.peek() != b')' {
                self.skip_value_fast()?;
            }
            self.read_byte(); // ')'
            Ok(left)
        } else {
            let right = self.eval()?;
            self.read_byte(); // ')'
            Ok(right)
        }
    }

    fn eval_and(&mut self) -> Result<RexValue, RexError> {
        self.read_byte(); // '('
        let left = self.eval()?;
        if !left.is_defined() {
            // Skip right
            if self.peek() != b')' {
                self.skip_value_fast()?;
            }
            self.read_byte(); // ')'
            Ok(RexValue::RexNone)
        } else {
            let right = self.eval()?;
            self.read_byte(); // ')'
            Ok(right)
        }
    }

    fn eval_for_in(&mut self) -> Result<RexValue, RexError> {
        self.read_byte(); // '(' or '['
        let opener = self.code[self.pos - 1];
        let closer = match opener { b'(' => b')', b'[' => b']', b'{' => b'}', _ => b')' };

        // Read iterable
        let iterable = self.eval()?;

        // Read bindings (consecutive $variables before the body)
        let mut bindings = Vec::new();
        while self.peek() != closer && !self.at_end() {
            let save = self.pos;
            let raw = self.read_raw();
            if self.peek() == b'$' {
                self.read_byte();
                bindings.push(Self::raw_to_str(raw).to_string());
            } else {
                self.pos = save;
                break;
            }
        }

        // Body position
        let body_start = self.pos;
        // Skip to find end
        self.skip_until(closer)?;
        let body_end = self.pos - 1; // before closer

        // Iterate
        let items = self.materialize_iterable(&iterable)?;
        let mut results = Vec::new();

        for (i, item) in items.iter().enumerate() {
            self.tick()?;
            self.self_stack.push(item.clone());

            // Bind variables
            if bindings.len() == 1 {
                self.vars.insert(bindings[0].clone(), item.clone());
            } else if bindings.len() == 2 {
                self.vars.insert(bindings[0].clone(), RexValue::Int(i as i64));
                self.vars.insert(bindings[1].clone(), item.clone());
            }

            self.pos = body_start;
            match self.eval_until(closer) {
                Ok(val) => {
                    // Force-evaluate during iteration so values referencing
                    // the loop variable are resolved before it's overwritten.
                    let forced = if opener != b'(' {
                        self.force_value(val)?
                    } else {
                        val
                    };
                    results.push(forced);
                }
                Err(RexError::BreakSignal(0)) => { self.self_stack.pop(); break; }
                Err(RexError::ContinueSignal(0)) => { self.self_stack.pop(); continue; }
                Err(e) => { self.self_stack.pop(); return Err(e); }
            }
            self.self_stack.pop();
        }

        self.pos = body_end + 1; // past closer

        if opener == b'(' {
            Ok(results.last().cloned().unwrap_or(RexValue::RexNone))
        } else {
            Ok(RexValue::Array(results))
        }
    }

    fn eval_for_of(&mut self) -> Result<RexValue, RexError> {
        // Same structure as for_in but iterates keys
        self.read_byte(); // opener
        let opener = self.code[self.pos - 1];
        let closer = match opener { b'(' => b')', b'[' => b']', b'{' => b'}', _ => b')' };

        let iterable = self.eval()?;

        let mut bindings = Vec::new();
        while self.peek() != closer && !self.at_end() {
            let save = self.pos;
            let raw = self.read_raw();
            if self.peek() == b'$' {
                self.read_byte();
                bindings.push(Self::raw_to_str(raw).to_string());
            } else {
                self.pos = save;
                break;
            }
        }

        let body_start = self.pos;
        self.skip_until(closer)?;
        let body_end = self.pos - 1;

        let keys = self.materialize_keys(&iterable)?;
        let mut results = Vec::new();

        for key in &keys {
            self.tick()?;
            self.self_stack.push(key.clone());

            if bindings.len() >= 1 {
                self.vars.insert(bindings[0].clone(), key.clone());
            }

            self.pos = body_start;
            match self.eval_until(closer) {
                Ok(val) => {
                    let forced = if opener != b'(' {
                        self.force_value(val)?
                    } else {
                        val
                    };
                    results.push(forced);
                }
                Err(RexError::BreakSignal(0)) => { self.self_stack.pop(); break; }
                Err(RexError::ContinueSignal(0)) => { self.self_stack.pop(); continue; }
                Err(e) => { self.self_stack.pop(); return Err(e); }
            }
            self.self_stack.pop();
        }

        self.pos = body_end + 1;
        if opener == b'(' {
            Ok(results.last().cloned().unwrap_or(RexValue::RexNone))
        } else {
            Ok(RexValue::Array(results))
        }
    }

    fn eval_while(&mut self) -> Result<RexValue, RexError> {
        self.read_byte(); // opener
        let opener = self.code[self.pos - 1];
        let closer = match opener { b'(' => b')', b'[' => b']', b'{' => b'}', _ => b')' };

        // Find the body start (after condition)
        let cond_start = self.pos;
        // We need to eval condition, then body, then loop
        // Skip to find structure first
        let save = self.pos;
        self.skip_value()?; // skip cond to find body start
        let body_start = self.pos;
        self.skip_until(closer)?;
        let body_end = self.pos - 1;
        self.pos = save;

        let mut results = Vec::new();
        loop {
            self.tick()?;
            self.pos = cond_start;
            let cond = self.eval()?;
            if !cond.is_defined() {
                break;
            }
            self.self_stack.push(cond);
            self.pos = body_start;
            match self.eval_until(closer) {
                Ok(val) => {
                    let forced = if opener != b'(' {
                        self.force_value(val)?
                    } else {
                        val
                    };
                    results.push(forced);
                }
                Err(RexError::BreakSignal(0)) => { self.self_stack.pop(); break; }
                Err(RexError::ContinueSignal(0)) => { self.self_stack.pop(); continue; }
                Err(e) => { self.self_stack.pop(); return Err(e); }
            }
            self.self_stack.pop();
        }

        self.pos = body_end + 1;
        if opener == b'(' {
            Ok(results.last().cloned().unwrap_or(RexValue::RexNone))
        } else {
            Ok(RexValue::Array(results))
        }
    }

    /// Eval values until we see `closer`, return the last result.
    fn eval_until(&mut self, closer: u8) -> Result<RexValue, RexError> {
        let mut last = RexValue::RexNone;
        while self.peek() != closer && !self.at_end() {
            last = self.eval()?;
        }
        Ok(last)
    }

    // ── Call dispatch ────────────────────────────────────────────────

    fn dispatch_call(&mut self, callee: RexValue, args: Vec<RexValue>) -> Result<RexValue, RexError> {
        match &callee {
            // Opcode call: %ad, %lt, etc.
            RexValue::Str(s) if s.starts_with('%') => {
                // Force-materialize args for opcodes.
                let eager_args: Vec<RexValue> = args.into_iter()
                    .map(|a| self.force_value(a))
                    .collect::<Result<Vec<_>, _>>()?;
                self.apply_opcode(&s[1..], &eager_args)
            }
            // Host object call: if the callee is a Host and the first arg
            // isn't a string key, invoke the Host's call method directly.
            // This allows hosts to register callable objects (e.g., tagged templates).
            RexValue::Host(idx) if !args.is_empty() && args[0].as_str().is_none() => {
                let eager_args: Vec<RexValue> = args.into_iter()
                    .map(|a| self.force_value(a))
                    .collect::<Result<Vec<_>, _>>()?;
                self.host_objects[*idx].call("", &eager_args)
            }
            // Navigation from variable or host
            _ => {
                let mut target = callee;
                for arg in &args {
                    target = self.read_property(&target, arg)?;
                }
                Ok(target)
            }
        }
    }

    // ── Property access ─────────────────────────────────────────────

    fn read_property(&mut self, target: &RexValue, key: &RexValue) -> Result<RexValue, RexError> {
        match target {
            RexValue::Object(pairs) => {
                if let Some(k) = key.as_str() {
                    for (pk, pv) in pairs {
                        if pk == k { return Ok(pv.clone()); }
                    }
                }
                Ok(RexValue::RexNone)
            }
            RexValue::Array(items) => {
                if let Some(k) = key.as_str() {
                    if k == "size" { return Ok(RexValue::Int(items.len() as i64)); }
                }
                if let Some(idx) = key.to_i64() {
                    if idx >= 0 && (idx as usize) < items.len() {
                        return Ok(items[idx as usize].clone());
                    }
                }
                Ok(RexValue::RexNone)
            }
            RexValue::Str(s) => {
                if let Some(k) = key.as_str() {
                    if k == "size" { return Ok(RexValue::Int(s.chars().count() as i64)); }
                }
                if let Some(idx) = key.to_i64() {
                    if idx >= 0 {
                        if let Some(c) = s.chars().nth(idx as usize) {
                            return Ok(RexValue::Str(c.to_string()));
                        }
                    }
                }
                Ok(RexValue::RexNone)
            }
            RexValue::Host(idx) => {
                if let Some(k) = key.as_str() {
                    Ok(self.host_objects[*idx].get(k).unwrap_or(RexValue::RexNone))
                } else if let Some(i) = key.to_i64() {
                    Ok(self.host_objects[*idx].get_index(i as usize).unwrap_or(RexValue::RexNone))
                } else {
                    Ok(RexValue::RexNone)
                }
            }
            _ => Ok(RexValue::RexNone),
        }
    }

    // ── Mutation (set/delete) ──────────────────────────────────────

    fn eval_set(&mut self) -> Result<RexValue, RexError> {
        // The place is the first child. Peek at tag to handle:
        // - $varname → simple variable assignment
        // - (call chain) → navigation assignment (host object write)
        let raw = self.read_raw();
        let tag = self.peek();

        if tag == b'$' {
            // Simple variable: $name = value
            self.read_byte(); // consume '$'
            let name = Self::raw_to_str(raw).to_string();
            let val = self.eval()?;
            self.vars.insert(name, val.clone());
            Ok(val)
        } else if tag == b'(' {
            // Navigation chain: (target keys...) = value
            // Eval the call to get the navigation path, but we need to
            // intercept the last step to write instead of read.
            self.read_byte(); // consume '('

            // Read all children of the call
            let mut parts = Vec::new();
            while self.peek() != b')' && !self.at_end() {
                parts.push(self.eval()?);
            }
            self.read_byte(); // consume ')'

            let val = self.eval()?;

            if parts.len() >= 2 {
                // Navigate to parent, then write the last key
                let mut target = parts[0].clone();
                for i in 1..parts.len() - 1 {
                    target = self.read_property(&target, &parts[i])?;
                }
                let last_key = &parts[parts.len() - 1];
                if let RexValue::Host(idx) = &target {
                    if let Some(k) = last_key.as_str() {
                        self.host_objects[*idx].set(k, val.clone())?;
                    }
                }
                // For non-host objects, variable assignment via navigation
                // would need copy-on-write semantics — skip for now
            } else if parts.len() == 1 {
                // Single variable navigation — shouldn't normally happen
            }

            Ok(val)
        } else {
            // Some other place expression — eval it
            let _place = self.eval()?;
            let val = self.eval()?;
            Ok(val)
        }
    }

    fn eval_delete(&mut self) -> Result<RexValue, RexError> {
        let raw = self.read_raw();
        let tag = self.peek();
        if tag == b'$' {
            self.read_byte();
            let name = Self::raw_to_str(raw);
            self.vars.remove(name);
        } else {
            self.skip_value()?;
        }
        Ok(RexValue::RexNone)
    }

    // ── Iteration helpers ───────────────────────────────────────────

    fn materialize_iterable(&mut self, value: &RexValue) -> Result<Vec<RexValue>, RexError> {
        match value {
            RexValue::Array(items) => Ok(items.clone()),
            RexValue::Object(pairs) => Ok(pairs.iter().map(|(_, v)| v.clone()).collect()),
            RexValue::Str(s) => Ok(s.chars().map(|c| RexValue::Str(c.to_string())).collect()),
            RexValue::Host(idx) => {
                Ok(self.host_objects[*idx].iter_values().unwrap_or_default())
            }
            _ => Ok(vec![]),
        }
    }

    fn materialize_keys(&mut self, value: &RexValue) -> Result<Vec<RexValue>, RexError> {
        match value {
            RexValue::Object(pairs) => Ok(pairs.iter().map(|(k, _)| RexValue::Str(k.clone())).collect()),
            RexValue::Array(items) => Ok((0..items.len()).map(|i| RexValue::Int(i as i64)).collect()),
            RexValue::Host(idx) => {
                Ok(self.host_objects[*idx].iter_keys().unwrap_or_default())
            }
            _ => Ok(vec![]),
        }
    }

    // ── Force evaluation ────────────────────────────────────────────

    /// Recursively force nested containers.
    fn force_value(&mut self, value: RexValue) -> Result<RexValue, RexError> {
        match value {
            RexValue::Array(items) => {
                let forced: Result<Vec<_>, _> = items.into_iter()
                    .map(|v| self.force_value(v))
                    .collect();
                Ok(RexValue::Array(forced?))
            }
            RexValue::Object(pairs) => {
                let forced: Result<Vec<_>, _> = pairs.into_iter()
                    .map(|(k, v)| self.force_value(v).map(|fv| (k, fv)))
                    .collect();
                Ok(RexValue::Object(forced?))
            }
            other => Ok(other),
        }
    }

    // ── Refs ────────────────────────────────────────────────────────

    fn resolve_ref(&self, name: &str) -> RexValue {
        match name {
            "t" => RexValue::Bool(true),
            "f" => RexValue::Bool(false),
            "n" => RexValue::Null,
            "no" => RexValue::RexNone,
            "nan" => RexValue::Float(f64::NAN),
            "inf" => RexValue::Float(f64::INFINITY),
            "nif" => RexValue::Float(f64::NEG_INFINITY),
            other => self.refs.get(other).cloned().unwrap_or(RexValue::RexNone),
        }
    }

    // ── Skip ────────────────────────────────────────────────────────

    fn skip_value(&mut self) -> Result<(), RexError> {
        if self.at_end() { return Ok(()); }
        let raw = self.read_raw();
        let tag = self.read_byte();
        match tag {
            b'+' | b'\'' | b'$' | b'%' | b'@' | b'\\' | b'^' => {}
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
                    // Skip index header if present (arrays and objects only)
                    if tag != b'(' && self.peek_is_index() {
                        self.skip_index();
                    }
                    self.skip_until(closer)?;
                }
            }
            b'?' | b'!' | b'|' | b'&' | b'>' | b'<' | b'#' => {
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

    /// Skip past an index header: <packed>#<pointers>.
    /// Assumes peek_is_index() returned true.
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
        if !self.at_end() { self.read_byte(); } // consume closer
        Ok(())
    }

    // ── Opcodes ─────────────────────────────────────────────────────

    fn apply_opcode(&mut self, name: &str, args: &[RexValue]) -> Result<RexValue, RexError> {
        // Check custom opcodes first
        if let Some(f) = self.opcodes.get(name) {
            return f(args);
        }

        match name {
            "ad" => self.op_add(args),
            "sb" => self.op_arith(args, |a, b| a - b),
            "ml" => self.op_arith(args, |a, b| a * b),
            "dv" => self.op_arith(args, |a, b| if b != 0.0 { a / b } else { f64::NAN }),
            "md" => self.op_arith(args, |a, b| if b != 0.0 { a % b } else { f64::NAN }),
            "ng" => {
                if args.is_empty() { return Ok(RexValue::RexNone); }
                match &args[0] {
                    RexValue::Int(n) => Ok(RexValue::Int(-n)),
                    RexValue::Float(n) => Ok(RexValue::Float(-n)),
                    _ => Ok(RexValue::RexNone),
                }
            }
            "eq" => self.op_compare(args, |o| o == std::cmp::Ordering::Equal),
            "nq" => self.op_compare(args, |o| o != std::cmp::Ordering::Equal),
            "gt" => self.op_compare(args, |o| o == std::cmp::Ordering::Greater),
            "ge" => self.op_compare(args, |o| o != std::cmp::Ordering::Less),
            "lt" => self.op_compare(args, |o| o == std::cmp::Ordering::Less),
            "le" => self.op_compare(args, |o| o != std::cmp::Ordering::Greater),
            "an" => self.op_bitwise(args, |a, b| a & b),
            "or" => self.op_bitwise(args, |a, b| a | b),
            "xr" => self.op_bitwise(args, |a, b| a ^ b),
            "nt" => {
                if args.is_empty() { return Ok(RexValue::RexNone); }
                match &args[0] {
                    RexValue::Int(n) => Ok(RexValue::Int(!n)),
                    RexValue::Bool(b) => Ok(RexValue::Bool(!b)),
                    _ => Ok(RexValue::RexNone),
                }
            }
            "rn" => {
                if args.len() < 2 { return Ok(RexValue::Array(vec![])); }
                let from = args[0].to_i64().unwrap_or(0);
                let to = args[1].to_i64().unwrap_or(0);
                let items: Vec<RexValue> = if from <= to {
                    (from..=to).map(RexValue::Int).collect()
                } else {
                    (to..=from).rev().map(RexValue::Int).collect()
                };
                Ok(RexValue::Array(items))
            }
            // Type predicates
            "st" => Ok(if args.first().map_or(false, |a| matches!(a, RexValue::Str(_))) { args[0].clone() } else { RexValue::RexNone }),
            "nm" => Ok(if args.first().map_or(false, |a| matches!(a, RexValue::Int(_) | RexValue::Float(_) | RexValue::Decimal{..})) { args[0].clone() } else { RexValue::RexNone }),
            "ob" => Ok(if args.first().map_or(false, |a| matches!(a, RexValue::Object(_) | RexValue::Host(_))) { args[0].clone() } else { RexValue::RexNone }),
            "ar" => Ok(if args.first().map_or(false, |a| matches!(a, RexValue::Array(_))) { args[0].clone() } else { RexValue::RexNone }),
            "bt" => Ok(if args.first().map_or(false, |a| matches!(a, RexValue::Bool(_))) { args[0].clone() } else { RexValue::RexNone }),
            _ => Ok(RexValue::RexNone),
        }
    }

    fn op_add(&self, args: &[RexValue]) -> Result<RexValue, RexError> {
        if args.len() < 2 { return Ok(RexValue::RexNone); }
        // String concatenation
        if let (RexValue::Str(a), RexValue::Str(b)) = (&args[0], &args[1]) {
            return Ok(RexValue::Str(format!("{a}{b}")));
        }
        // Numeric add
        if let (Some(a), Some(b)) = (args[0].to_f64(), args[1].to_f64()) {
            let r = a + b;
            if r.fract() == 0.0 && r.abs() < i64::MAX as f64 {
                return Ok(RexValue::Int(r as i64));
            }
            return Ok(RexValue::Float(r));
        }
        Ok(RexValue::RexNone)
    }

    fn op_arith(&self, args: &[RexValue], f: fn(f64, f64) -> f64) -> Result<RexValue, RexError> {
        if args.len() < 2 { return Ok(RexValue::RexNone); }
        if let (Some(a), Some(b)) = (args[0].to_f64(), args[1].to_f64()) {
            let r = f(a, b);
            if r.fract() == 0.0 && r.abs() < i64::MAX as f64 && !r.is_nan() {
                return Ok(RexValue::Int(r as i64));
            }
            return Ok(RexValue::Float(r));
        }
        Ok(RexValue::RexNone)
    }

    fn op_compare(&self, args: &[RexValue], pred: fn(std::cmp::Ordering) -> bool) -> Result<RexValue, RexError> {
        if args.len() < 2 { return Ok(RexValue::RexNone); }
        let ord = match (&args[0], &args[1]) {
            (RexValue::Int(a), RexValue::Int(b)) => a.cmp(b),
            (a, b) => {
                if let (Some(fa), Some(fb)) = (a.to_f64(), b.to_f64()) {
                    fa.partial_cmp(&fb).unwrap_or(std::cmp::Ordering::Equal)
                } else if let (RexValue::Str(a), RexValue::Str(b)) = (a, b) {
                    a.cmp(b)
                } else {
                    return Ok(RexValue::RexNone);
                }
            }
        };
        if pred(ord) { Ok(args[0].clone()) } else { Ok(RexValue::RexNone) }
    }

    fn op_bitwise(&self, args: &[RexValue], f: fn(i64, i64) -> i64) -> Result<RexValue, RexError> {
        if args.len() < 2 { return Ok(RexValue::RexNone); }
        if let (Some(a), Some(b)) = (args[0].to_i64(), args[1].to_i64()) {
            Ok(RexValue::Int(f(a, b)))
        } else {
            Ok(RexValue::RexNone)
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(source: &str) -> RexValue {
        let bytecode = crate::compile(source);
        run(&bytecode, Context::default()).unwrap().value
    }

    #[test]
    fn eval_integer() {
        assert!(matches!(eval("42"), RexValue::Int(42)));
        assert!(matches!(eval("0"), RexValue::Int(0)));
        assert!(matches!(eval("-1"), RexValue::Int(-1)));
    }

    #[test]
    fn eval_string() {
        assert_eq!(eval(r#""hello""#).as_str(), Some("hello"));
    }

    #[test]
    fn eval_bool_null() {
        assert!(matches!(eval("true"), RexValue::Bool(true)));
        assert!(matches!(eval("false"), RexValue::Bool(false)));
        assert!(matches!(eval("null"), RexValue::Null));
        assert!(matches!(eval("none"), RexValue::RexNone));
    }

    #[test]
    fn eval_addition() {
        assert!(matches!(eval("1 + 2"), RexValue::Int(3)));
    }

    #[test]
    fn eval_arithmetic() {
        assert!(matches!(eval("10 - 3"), RexValue::Int(7)));
        assert!(matches!(eval("4 * 5"), RexValue::Int(20)));
        assert!(matches!(eval("10 / 2"), RexValue::Int(5)));
        assert!(matches!(eval("7 % 3"), RexValue::Int(1)));
    }

    #[test]
    fn eval_string_concat() {
        assert_eq!(eval(r#""hello" + " " + "world""#).as_str(), Some("hello world"));
    }

    #[test]
    fn eval_comparison() {
        assert!(eval("5 > 3").is_defined());
        assert!(!eval("3 > 5").is_defined());
        assert!(eval("5 == 5").is_defined());
        assert!(!eval("5 == 6").is_defined());
    }

    #[test]
    fn eval_assignment() {
        let bc = crate::compile("x = 42\nx");
        let result = run(&bc, Context::default()).unwrap();
        assert!(matches!(result.value, RexValue::Int(42)));
    }

    #[test]
    fn eval_when() {
        assert!(matches!(eval("when true do 42 end"), RexValue::Int(42)));
        assert!(matches!(eval("when none do 42 end"), RexValue::RexNone));
    }

    #[test]
    fn eval_when_else() {
        assert!(matches!(eval("when true do 1 else 2 end"), RexValue::Int(1)));
        assert!(matches!(eval("when none do 1 else 2 end"), RexValue::Int(2)));
    }

    #[test]
    fn eval_or() {
        assert!(matches!(eval("none or 42"), RexValue::Int(42)));
        assert!(matches!(eval("1 or 42"), RexValue::Int(1)));
    }

    #[test]
    fn eval_and() {
        assert!(matches!(eval("1 and 2"), RexValue::Int(2)));
        assert!(!eval("none and 2").is_defined());
    }

    #[test]
    fn eval_block() {
        let bc = crate::compile("x = 1\ny = 2\nx + y");
        let result = run(&bc, Context::default()).unwrap();
        assert!(matches!(result.value, RexValue::Int(3)));
    }

    #[test]
    fn eval_data_array() {
        // Data arrays produce concrete values.
        let v = eval("[1, 2, 3]");
        assert_eq!(v.type_name(), "array");
        if let RexValue::Array(items) = v {
            assert_eq!(items.len(), 3);
        }
    }

    #[test]
    fn eval_range() {
        let v = eval("[self in 1..3]");
        if let RexValue::Array(items) = v {
            assert_eq!(items.len(), 3);
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn eval_template_no_interpolation() {
        let v = eval("`hello`");
        if let RexValue::Str(s) = v {
            assert_eq!(s, "hello");
        } else {
            panic!("expected string, got {:?}", v);
        }
    }

    #[test]
    fn eval_template_with_variable() {
        let bc = crate::compile("name = `world`\n`hello ${name}`");
        let result = run(&bc, Context::default()).unwrap();
        if let RexValue::Str(s) = result.value {
            assert_eq!(s, "hello world");
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn eval_template_with_integer() {
        let bc = crate::compile("x = 42\n`the answer is ${x}`");
        let result = run(&bc, Context::default()).unwrap();
        if let RexValue::Str(s) = result.value {
            assert_eq!(s, "the answer is 42");
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn eval_template_with_bool() {
        let bc = crate::compile("`value: ${true}`");
        let result = run(&bc, Context::default()).unwrap();
        if let RexValue::Str(s) = result.value {
            assert_eq!(s, "value: \u{2713}");
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn eval_template_with_none() {
        let bc = crate::compile("`got: ${name}`");
        let result = run(&bc, Context::default()).unwrap();
        if let RexValue::Str(s) = result.value {
            // name is undefined → none → ∅
            assert_eq!(s, "got: \u{2205}");
        } else {
            panic!("expected string");
        }
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
        assert!(matches!(eval("return 42"), RexValue::Int(42)));
    }

    #[test]
    fn eval_bare_return() {
        assert!(matches!(eval("return"), RexValue::RexNone));
    }

    #[test]
    fn eval_return_halts() {
        // Return halts execution — 99 is never reached
        assert!(matches!(eval("return 1\n99"), RexValue::Int(1)));
    }

    #[test]
    fn eval_return_in_when() {
        // Return exits the entire program, not just the when block
        assert!(matches!(eval("when true do return 1 end\n2"), RexValue::Int(1)));
    }

    #[test]
    fn eval_return_in_when_skipped() {
        // When condition is false, return is not hit
        assert!(matches!(eval("when none do return 1 end\n2"), RexValue::Int(2)));
    }

    #[test]
    fn eval_return_in_unless() {
        assert!(matches!(eval("unless none do return 42 end\n99"), RexValue::Int(42)));
    }

    #[test]
    fn eval_return_in_loop() {
        let bc = crate::compile("x = 0\nwhile true do\n  x = x + 1\n  when x == 5 do return x end\nend\n99");
        let mut ctx = Context::default();
        ctx.gas_limit = 10000;
        let result = run(&bc, ctx).unwrap();
        assert!(matches!(result.value, RexValue::Int(5)));
    }

    // ── Length-prefix skip tests ───────────────────────────────────────

    #[test]
    fn skip_length_prefixed_then_branch() {
        // x is none → skip then block, eval else
        assert!(matches!(eval("x = none\nwhen x do\n  99\nelse\n  42\nend"), RexValue::Int(42)));
    }

    #[test]
    fn skip_length_prefixed_else_branch() {
        // x is defined → eval then, skip else block
        assert!(matches!(eval("x = 1\nwhen x do\n  42\nelse\n  99\nend"), RexValue::Int(42)));
    }

    #[test]
    fn skip_unless_length_prefixed() {
        assert!(matches!(eval("unless true do 99 end"), RexValue::RexNone));
        assert!(matches!(eval("unless none do 42 end"), RexValue::Int(42)));
    }

    #[test]
    fn skip_or_length_prefixed() {
        // left is defined → skip right (which is a block)
        let bc = crate::compile("x = 1\nx or [1, 2, 3]");
        let result = run(&bc, Context::default()).unwrap();
        assert!(matches!(result.value, RexValue::Int(1)));
    }

    #[test]
    fn skip_and_length_prefixed() {
        // left is none → skip right (which is a block)
        assert!(!eval("none and [1, 2, 3]").is_defined());
    }

    #[test]
    fn cross_branch_dedup_safe() {
        // Ensure pointer dedup doesn't create cross-branch references
        let source = "x = none\nunless x do y = 401 end\nwhen x do\n  unless x do y = 401 end\nend\ny";
        assert!(matches!(eval(source), RexValue::Int(401)));
    }

    #[test]
    fn nested_when_skip() {
        // Nested conditionals with blocks — all should skip correctly
        let source = "x = none\nwhen x do\n  when x do 1 else 2 end\nelse\n  when true do 42 else 99 end\nend";
        assert!(matches!(eval(source), RexValue::Int(42)));
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
        if let RexValue::Array(vals) = result.value {
            assert_eq!(vals.len(), 3);
            assert!(matches!(vals[0], RexValue::Int(1)));
            assert!(matches!(vals[1], RexValue::Int(2)));
            assert!(matches!(vals[2], RexValue::Int(3)));
        } else {
            panic!("expected array");
        }
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
        if let RexValue::Array(vals) = result.value {
            assert_eq!(vals.len(), 2);
            assert_eq!(vals[0].as_str(), Some("hello"));
            assert_eq!(vals[1].as_str(), Some("world"));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn eval_indexed_object() {
        use crate::bytecode::{encode_indexed_object, Value};

        let pairs = vec![
            (Value::String("name".into()), Value::String("Ada".into())),
            (Value::String("score".into()), Value::Integer(95)),
        ];
        let bc = encode_indexed_object(&pairs);
        let result = run(&bc, Context::default()).unwrap();
        if let RexValue::Object(vals) = result.value {
            assert_eq!(vals.len(), 2);
            assert_eq!(vals[0].0, "name");
            assert_eq!(vals[0].1.as_str(), Some("Ada"));
            assert_eq!(vals[1].0, "score");
            assert!(matches!(vals[1].1, RexValue::Int(95)));
        } else {
            panic!("expected object, got {:?}", result.value);
        }
    }
}

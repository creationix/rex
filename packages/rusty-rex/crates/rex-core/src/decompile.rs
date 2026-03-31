//! Decompile bytecode `Value` back to pretty-printed Rex source code.

use std::fmt::Write as _;

use crate::bytecode::Value;

/// Decompile a bytecode Value to Rex source code.
/// Pointers and chains should already be resolved by the decoder.
pub fn decompile(value: &Value) -> String {
    let mut out = String::new();
    let mut ctx = Ctx { indent: 0, raw: false };
    ctx.write(value, &mut out, Prec::Top);
    out
}

/// Decompile in raw mode — preserves pointers and shows internal structure.
pub fn decompile_raw(value: &Value) -> String {
    let mut out = String::new();
    let mut ctx = Ctx { indent: 0, raw: true };
    ctx.write(value, &mut out, Prec::Top);
    out
}

/// Operator precedence levels (lower = binds looser).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prec {
    Top,      // statement level
    Assign,   // = :=
    Or,       // or nor
    And,      // and
    BitOr,    // |
    BitXor,   // ^
    BitAnd,   // &
    Compare,  // == != > >= < <=
    Range,    // ..
    Add,      // + -
    Mul,      // * / %
    Unary,    // - ~ not delete
    Postfix,  // . .() ()
    Atom,     // literals, identifiers
}

struct Ctx {
    indent: usize,
    raw: bool,
}

impl Ctx {
    fn write(&mut self, value: &Value, out: &mut String, prec: Prec) {
        match value {
            Value::Integer(n) => write_integer(*n, out),
            Value::Decimal { sig, exp } => write_decimal(*sig, *exp, out),
            Value::String(s) => write_string(s, out),
            Value::Ref(name) => write_ref(name, out),
            Value::Variable(name) => out.push_str(name),
            Value::Opcode(name) => out.push_str(name), // standalone opcode (type predicate)
            Value::SelfRef(depth) => write_self(*depth, out),
            Value::BreakCont(v) => write_break_cont(*v, out),
            Value::Pointer(delta) => {
                if self.raw {
                    write!(out, "^{delta}").unwrap();
                } else {
                    write!(out, "/* ^{delta} */").unwrap();
                }
            }

            Value::List(items) => self.write_list(items, out),
            Value::Map(pairs) => self.write_map(pairs, out),
            Value::Array(items) => self.write_array(items, out),
            Value::Block(items) => self.write_block(items, out, prec),
            Value::Call(items) => self.write_call(items, out, prec),

            Value::When(items) => self.write_conditional("when", items, out),
            Value::Unless(items) => self.write_conditional("unless", items, out),
            Value::Or(items) => self.write_binary_logic("or", items, out, prec, Prec::Or),
            Value::And(items) => self.write_binary_logic("and", items, out, prec, Prec::And),
            Value::ForIn(items) => self.write_for("in", items, out),
            Value::ForOf(items) => self.write_for("of", items, out),
            Value::While(items) => self.write_while(items, out),

            Value::ListCompIn(items) => self.write_list_comp("in", items, out),
            Value::ListCompOf(items) => self.write_list_comp("of", items, out),
            Value::ListCompWhile(items) => self.write_while_list_comp(items, out),
            Value::MapCompIn(items) => self.write_map_comp("in", items, out),
            Value::MapCompOf(items) => self.write_map_comp("of", items, out),
            Value::MapCompWhile(items) => self.write_while_map_comp(items, out),

            Value::Set(place, val) => self.write_assign("=", place, val, out, prec),
            Value::Swap(place, val) => self.write_assign(":=", place, val, out, prec),
            Value::Delete(place) => {
                out.push_str("delete ");
                self.write(place, out, Prec::Unary);
            }

            Value::Chain(segments) => {
                // Decompile chain back to template literal syntax
                out.push('`');
                for seg in segments {
                    match seg {
                        Value::String(s) => {
                            // Escape backticks and ${
                            for c in s.chars() {
                                match c {
                                    '`' => out.push_str("\\`"),
                                    '\\' => out.push_str("\\\\"),
                                    '$' => out.push_str("\\$"),
                                    _ => out.push(c),
                                }
                            }
                        }
                        _ => {
                            out.push_str("${");
                            self.write(seg, out, Prec::Top);
                            out.push('}');
                        }
                    }
                }
                out.push('`');
            }
        }
    }

    fn write_list(&mut self, items: &[Value], out: &mut String) {
        out.push('[');
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            self.write(item, out, Prec::Top);
        }
        out.push(']');
    }

    fn write_array(&mut self, items: &[Value], out: &mut String) {
        out.push('[');
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            self.write(item, out, Prec::Top);
        }
        out.push(']');
    }

    fn write_map(&mut self, pairs: &[(Value, Value)], out: &mut String) {
        if pairs.is_empty() {
            out.push_str("{}");
            return;
        }
        out.push('{');
        let multiline = pairs.len() > 1;
        if multiline {
            self.indent += 1;
        }
        for (i, (key, val)) in pairs.iter().enumerate() {
            if multiline {
                out.push('\n');
                write_indent(self.indent, out);
            }
            // Bare identifier keys don't need quotes
            if let Value::String(k) = key {
                if is_bare_key(k) {
                    out.push_str(k);
                } else {
                    write_string(k, out);
                }
            } else {
                out.push('(');
                self.write(key, out, Prec::Top);
                out.push(')');
            }
            out.push_str(": ");
            self.write(val, out, Prec::Top);
            if !multiline && i + 1 < pairs.len() {
                out.push_str(", ");
            }
        }
        if multiline {
            self.indent -= 1;
            out.push('\n');
            write_indent(self.indent, out);
        }
        out.push('}');
    }

    fn write_block(&mut self, items: &[Value], out: &mut String, _parent_prec: Prec) {
        // A block at top level just emits statements
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                out.push('\n');
                write_indent(self.indent, out);
            }
            self.write(item, out, Prec::Top);
        }
    }

    fn write_call(&mut self, items: &[Value], out: &mut String, parent_prec: Prec) {
        if items.is_empty() {
            out.push_str("()");
            return;
        }

        let callee = &items[0];
        let args = &items[1..];

        match callee {
            // Opcode call → operator or function
            Value::Opcode(op) => self.write_opcode_call(op, args, out, parent_prec),
            // Variable/expression call with string args → navigation
            _ => {
                // Check if this is navigation (string args) or function call
                let is_nav = !args.is_empty()
                    && args.iter().all(|a| {
                        matches!(a, Value::String(_) | Value::Variable(_)
                            | Value::Call(_) | Value::SelfRef(_))
                    });

                if is_nav && args.iter().all(|a| matches!(a, Value::String(_))) {
                    // Pure static navigation: foo.bar.baz
                    self.write(callee, out, Prec::Postfix);
                    for arg in args {
                        if let Value::String(key) = arg {
                            out.push('.');
                            out.push_str(key);
                        }
                    }
                } else if is_nav {
                    // Mixed navigation
                    self.write(callee, out, Prec::Postfix);
                    for arg in args {
                        match arg {
                            Value::String(key) if is_bare_key(key) => {
                                out.push('.');
                                out.push_str(key);
                            }
                            _ => {
                                out.push_str(".(");
                                self.write(arg, out, Prec::Top);
                                out.push(')');
                            }
                        }
                    }
                } else {
                    // Function call: f(a, b)
                    self.write(callee, out, Prec::Postfix);
                    out.push('(');
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        self.write(arg, out, Prec::Top);
                    }
                    out.push(')');
                }
            }
        }
    }

    fn write_opcode_call(&mut self, op: &str, args: &[Value], out: &mut String, parent_prec: Prec) {
        match (op, args.len()) {
            // Binary operators
            ("ad", 2) => self.write_binop("+", &args[0], &args[1], out, parent_prec, Prec::Add),
            ("sb", 2) => self.write_binop("-", &args[0], &args[1], out, parent_prec, Prec::Add),
            ("ml", 2) => self.write_binop("*", &args[0], &args[1], out, parent_prec, Prec::Mul),
            ("dv", 2) => self.write_binop("/", &args[0], &args[1], out, parent_prec, Prec::Mul),
            ("md", 2) => self.write_binop("%", &args[0], &args[1], out, parent_prec, Prec::Mul),
            ("an", 2) => self.write_binop("&", &args[0], &args[1], out, parent_prec, Prec::BitAnd),
            ("or", 2) => self.write_binop("|", &args[0], &args[1], out, parent_prec, Prec::BitOr),
            ("xr", 2) => self.write_binop("^", &args[0], &args[1], out, parent_prec, Prec::BitXor),
            ("eq", 2) => self.write_binop("==", &args[0], &args[1], out, parent_prec, Prec::Compare),
            ("nq", 2) => self.write_binop("!=", &args[0], &args[1], out, parent_prec, Prec::Compare),
            ("gt", 2) => self.write_binop(">", &args[0], &args[1], out, parent_prec, Prec::Compare),
            ("ge", 2) => self.write_binop(">=", &args[0], &args[1], out, parent_prec, Prec::Compare),
            ("lt", 2) => self.write_binop("<", &args[0], &args[1], out, parent_prec, Prec::Compare),
            ("le", 2) => self.write_binop("<=", &args[0], &args[1], out, parent_prec, Prec::Compare),
            ("rn", 2) => self.write_binop("..", &args[0], &args[1], out, parent_prec, Prec::Range),
            // Unary operators
            ("ng", 1) => {
                let need_parens = parent_prec > Prec::Unary;
                if need_parens { out.push('('); }
                out.push('-');
                self.write(&args[0], out, Prec::Unary);
                if need_parens { out.push(')'); }
            }
            ("nt", 1) => {
                let need_parens = parent_prec > Prec::Unary;
                if need_parens { out.push('('); }
                out.push('~');
                self.write(&args[0], out, Prec::Unary);
                if need_parens { out.push(')'); }
            }
            // Type predicates as standalone keywords
            ("st", 0) => out.push_str("string"),
            ("nm", 0) => out.push_str("number"),
            ("ob", 0) => out.push_str("object"),
            ("ar", 0) => out.push_str("array"),
            ("bt", 0) => out.push_str("boolean"),
            // Type predicates as calls
            ("st", _) => self.write_func_call("string", args, out),
            ("nm", _) => self.write_func_call("number", args, out),
            ("ob", _) => self.write_func_call("object", args, out),
            ("ar", _) => self.write_func_call("array", args, out),
            ("bt", _) => self.write_func_call("boolean", args, out),
            // Unknown opcode → function-style call
            _ => self.write_func_call(op, args, out),
        }
    }

    fn write_binop(
        &mut self,
        op: &str,
        lhs: &Value,
        rhs: &Value,
        out: &mut String,
        parent_prec: Prec,
        op_prec: Prec,
    ) {
        let need_parens = parent_prec > op_prec;
        if need_parens {
            out.push('(');
        }
        self.write(lhs, out, op_prec);
        out.push(' ');
        out.push_str(op);
        out.push(' ');
        // Right side gets one level tighter for left-associativity
        self.write(rhs, out, Prec::from_u8(op_prec.to_u8() + 1));
        if need_parens {
            out.push(')');
        }
    }

    fn write_func_call(&mut self, name: &str, args: &[Value], out: &mut String) {
        out.push_str(name);
        out.push('(');
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            self.write(arg, out, Prec::Top);
        }
        out.push(')');
    }

    fn write_binary_logic(
        &mut self,
        keyword: &str,
        items: &[Value],
        out: &mut String,
        parent_prec: Prec,
        op_prec: Prec,
    ) {
        if items.len() < 2 {
            if let Some(item) = items.first() {
                self.write(item, out, parent_prec);
            }
            return;
        }
        let need_parens = parent_prec > op_prec;
        if need_parens {
            out.push('(');
        }
        self.write(&items[0], out, op_prec);
        out.push(' ');
        out.push_str(keyword);
        out.push(' ');
        self.write(&items[1], out, Prec::from_u8(op_prec.to_u8() + 1));
        if need_parens {
            out.push(')');
        }
    }

    fn write_conditional(&mut self, keyword: &str, items: &[Value], out: &mut String) {
        out.push_str(keyword);
        out.push(' ');
        if let Some(cond) = items.first() {
            self.write(cond, out, Prec::Top);
        }
        out.push_str(" do");
        if items.len() > 1 {
            self.indent += 1;
            out.push('\n');
            write_indent(self.indent, out);
            self.write(&items[1], out, Prec::Top);
            self.indent -= 1;
        }
        if items.len() > 2 {
            out.push('\n');
            write_indent(self.indent, out);
            out.push_str("else");
            // Check if else branch is another conditional
            match &items[2] {
                Value::When(inner) => {
                    out.push(' ');
                    self.write_conditional("when", inner, out);
                    return;
                }
                Value::Unless(inner) => {
                    out.push(' ');
                    self.write_conditional("unless", inner, out);
                    return;
                }
                _ => {
                    self.indent += 1;
                    out.push('\n');
                    write_indent(self.indent, out);
                    self.write(&items[2], out, Prec::Top);
                    self.indent -= 1;
                }
            }
        }
        out.push('\n');
        write_indent(self.indent, out);
        out.push_str("end");
    }

    fn write_for(&mut self, kind: &str, items: &[Value], out: &mut String) {
        out.push_str("for ");
        // items: iterable, [$bindings], body
        let mut i = 0;
        let iterable_idx = 0;
        i += 1;
        // Collect bindings (consecutive Variable values)
        let mut bindings = Vec::new();
        while i < items.len() - 1 {
            if let Value::Variable(_) = &items[i] {
                bindings.push(&items[i]);
                i += 1;
            } else {
                break;
            }
        }
        // Write bindings
        for (j, b) in bindings.iter().enumerate() {
            if j > 0 {
                out.push_str(", ");
            }
            self.write(b, out, Prec::Atom);
        }
        if !bindings.is_empty() {
            out.push(' ');
        }
        out.push_str(kind);
        out.push(' ');
        self.write(&items[iterable_idx], out, Prec::Top);
        out.push_str(" do");
        if i < items.len() {
            self.indent += 1;
            out.push('\n');
            write_indent(self.indent, out);
            self.write(&items[i], out, Prec::Top);
            self.indent -= 1;
        }
        out.push('\n');
        write_indent(self.indent, out);
        out.push_str("end");
    }

    fn write_while(&mut self, items: &[Value], out: &mut String) {
        out.push_str("while ");
        if let Some(cond) = items.first() {
            self.write(cond, out, Prec::Top);
        }
        out.push_str(" do");
        if items.len() > 1 {
            self.indent += 1;
            out.push('\n');
            write_indent(self.indent, out);
            self.write(&items[1], out, Prec::Top);
            self.indent -= 1;
        }
        out.push('\n');
        write_indent(self.indent, out);
        out.push_str("end");
    }

    fn write_assign(&mut self, op: &str, place: &Value, val: &Value, out: &mut String, _parent_prec: Prec) {
        self.write(place, out, Prec::Postfix);
        out.push(' ');
        // Detect compound assignment: x = add(x, expr) → x += expr
        if op == "=" {
            if let Some((compound_op, rhs)) = detect_compound_assign(place, val) {
                out.push_str(compound_op);
                out.push(' ');
                self.write(rhs, out, Prec::Top);
                return;
            }
        }
        out.push_str(op);
        out.push(' ');
        self.write(val, out, Prec::Top);
    }

    fn write_list_comp(&mut self, kind: &str, items: &[Value], out: &mut String) {
        out.push('[');
        // items: iterable, [$bindings], value_expr
        let mut i = 0;
        let iterable_idx = 0;
        i += 1;
        let mut bindings = Vec::new();
        while i < items.len() - 1 {
            if let Value::Variable(_) = &items[i] {
                bindings.push(&items[i]);
                i += 1;
            } else {
                break;
            }
        }
        // value expression
        if i < items.len() {
            self.write(&items[i], out, Prec::Top);
        }
        if bindings.is_empty() {
            out.push(' ');
            out.push_str(kind);
            out.push(' ');
        } else {
            out.push_str(" for ");
            for (j, b) in bindings.iter().enumerate() {
                if j > 0 {
                    out.push_str(", ");
                }
                self.write(b, out, Prec::Atom);
            }
            out.push(' ');
            out.push_str(kind);
            out.push(' ');
        }
        self.write(&items[iterable_idx], out, Prec::Top);
        out.push(']');
    }

    fn write_while_list_comp(&mut self, items: &[Value], out: &mut String) {
        out.push('[');
        if items.len() > 1 {
            self.write(&items[1], out, Prec::Top);
        }
        out.push_str(" while ");
        if let Some(cond) = items.first() {
            self.write(cond, out, Prec::Top);
        }
        out.push(']');
    }

    fn write_map_comp(&mut self, kind: &str, items: &[Value], out: &mut String) {
        out.push('{');
        // items: iterable, [$bindings], key_expr, value_expr
        let mut i = 0;
        let iterable_idx = 0;
        i += 1;
        let mut bindings = Vec::new();
        while i < items.len() - 2 {
            if let Value::Variable(_) = &items[i] {
                bindings.push(&items[i]);
                i += 1;
            } else {
                break;
            }
        }
        // key: value
        if i + 1 < items.len() {
            self.write(&items[i], out, Prec::Top);
            out.push_str(": ");
            self.write(&items[i + 1], out, Prec::Top);
        }
        if bindings.is_empty() {
            out.push(' ');
            out.push_str(kind);
            out.push(' ');
        } else {
            out.push_str(" for ");
            for (j, b) in bindings.iter().enumerate() {
                if j > 0 {
                    out.push_str(", ");
                }
                self.write(b, out, Prec::Atom);
            }
            out.push(' ');
            out.push_str(kind);
            out.push(' ');
        }
        self.write(&items[iterable_idx], out, Prec::Top);
        out.push('}');
    }

    fn write_while_map_comp(&mut self, items: &[Value], out: &mut String) {
        out.push('{');
        if items.len() > 2 {
            self.write(&items[1], out, Prec::Top);
            out.push_str(": ");
            self.write(&items[2], out, Prec::Top);
        }
        out.push_str(" while ");
        if let Some(cond) = items.first() {
            self.write(cond, out, Prec::Top);
        }
        out.push('}');
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

impl Prec {
    fn to_u8(self) -> u8 {
        self as u8
    }

    fn from_u8(n: u8) -> Prec {
        match n {
            0 => Prec::Top,
            1 => Prec::Assign,
            2 => Prec::Or,
            3 => Prec::And,
            4 => Prec::BitOr,
            5 => Prec::BitXor,
            6 => Prec::BitAnd,
            7 => Prec::Compare,
            8 => Prec::Range,
            9 => Prec::Add,
            10 => Prec::Mul,
            11 => Prec::Unary,
            12 => Prec::Postfix,
            _ => Prec::Atom,
        }
    }
}

fn write_indent(level: usize, out: &mut String) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn write_integer(n: i64, out: &mut String) {
    use std::fmt::Write;
    write!(out, "{n}").unwrap();
}

fn write_decimal(sig: i64, exp: i64, out: &mut String) {
    use std::fmt::Write;
    write!(out, "{sig}e{exp}").unwrap();
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            c if c.is_control() => {
                use std::fmt::Write;
                write!(out, "\\u{:04x}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_ref(name: &str, out: &mut String) {
    match name {
        "t" => out.push_str("true"),
        "f" => out.push_str("false"),
        "n" => out.push_str("null"),
        "no" => out.push_str("none"),
        "nan" => out.push_str("nan"),
        "inf" => out.push_str("inf"),
        "nif" => out.push_str("-inf"),
        other => {
            out.push('\'');
            out.push_str(other);
        }
    }
}

fn write_self(depth: u32, out: &mut String) {
    out.push_str("self");
    if depth > 0 {
        use std::fmt::Write;
        write!(out, "@{depth}").unwrap();
    }
}

fn write_break_cont(v: u32, out: &mut String) {
    if v % 2 == 0 {
        out.push_str("break");
    } else {
        out.push_str("continue");
    }
}

/// Check if a string can be used as a bare key (no quotes needed).
fn is_bare_key(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    let first = bytes[0];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Detect compound assignment patterns like x = add(x, expr) → ("+= ", expr).
fn detect_compound_assign<'a>(place: &Value, val: &'a Value) -> Option<(&'static str, &'a Value)> {
    let Value::Call(items) = val else {
        return None;
    };
    if items.len() != 3 {
        return None;
    }
    let Value::Opcode(op) = &items[0] else {
        return None;
    };
    // Check if first arg matches the place
    if &items[1] != place {
        return None;
    }
    let compound_op = match op.as_str() {
        "ad" => "+=",
        "sb" => "-=",
        "ml" => "*=",
        "dv" => "/=",
        "md" => "%=",
        "an" => "&=",
        "or" => "|=",
        "xr" => "^=",
        _ => return None,
    };
    Some((compound_op, &items[2]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::Value;

    #[test]
    fn decompile_integer() {
        assert_eq!(decompile(&Value::Integer(42)), "42");
        assert_eq!(decompile(&Value::Integer(-1)), "-1");
        assert_eq!(decompile(&Value::Integer(0)), "0");
    }

    #[test]
    fn decompile_decimal() {
        assert_eq!(decompile(&Value::Decimal { sig: 314, exp: -2 }), "314e-2");
        assert_eq!(decompile(&Value::Decimal { sig: 5, exp: -1 }), "5e-1");
        assert_eq!(decompile(&Value::Decimal { sig: 100, exp: 2 }), "100e2");
        assert_eq!(decompile(&Value::Decimal { sig: 0, exp: 0 }), "0e0");
    }

    #[test]
    fn decompile_string() {
        assert_eq!(decompile(&Value::String("hello".into())), "\"hello\"");
        assert_eq!(decompile(&Value::String("a\"b".into())), "\"a\\\"b\"");
    }

    #[test]
    fn decompile_refs() {
        assert_eq!(decompile(&Value::Ref("t".into())), "true");
        assert_eq!(decompile(&Value::Ref("f".into())), "false");
        assert_eq!(decompile(&Value::Ref("n".into())), "null");
        assert_eq!(decompile(&Value::Ref("no".into())), "none");
    }

    #[test]
    fn decompile_variable() {
        assert_eq!(decompile(&Value::Variable("x".into())), "x");
        assert_eq!(decompile(&Value::Variable("my-var".into())), "my-var");
    }

    #[test]
    fn decompile_self() {
        assert_eq!(decompile(&Value::SelfRef(0)), "self");
        assert_eq!(decompile(&Value::SelfRef(2)), "self@2");
    }

    #[test]
    fn decompile_break_continue() {
        assert_eq!(decompile(&Value::BreakCont(0)), "break");
        assert_eq!(decompile(&Value::BreakCont(1)), "continue");
    }

    #[test]
    fn decompile_addition() {
        let v = Value::Call(vec![
            Value::Opcode("ad".into()),
            Value::Integer(1),
            Value::Integer(2),
        ]);
        assert_eq!(decompile(&v), "1 + 2");
    }

    #[test]
    fn decompile_precedence() {
        // 1 + 2 * 3
        let v = Value::Call(vec![
            Value::Opcode("ad".into()),
            Value::Integer(1),
            Value::Call(vec![
                Value::Opcode("ml".into()),
                Value::Integer(2),
                Value::Integer(3),
            ]),
        ]);
        assert_eq!(decompile(&v), "1 + 2 * 3");
    }

    #[test]
    fn decompile_precedence_parens() {
        // (1 + 2) * 3
        let v = Value::Call(vec![
            Value::Opcode("ml".into()),
            Value::Call(vec![
                Value::Opcode("ad".into()),
                Value::Integer(1),
                Value::Integer(2),
            ]),
            Value::Integer(3),
        ]);
        assert_eq!(decompile(&v), "(1 + 2) * 3");
    }

    #[test]
    fn decompile_assignment() {
        let v = Value::Set(
            Box::new(Value::Variable("x".into())),
            Box::new(Value::Integer(42)),
        );
        assert_eq!(decompile(&v), "x = 42");
    }

    #[test]
    fn decompile_compound_assign() {
        let v = Value::Set(
            Box::new(Value::Variable("x".into())),
            Box::new(Value::Call(vec![
                Value::Opcode("ad".into()),
                Value::Variable("x".into()),
                Value::Integer(1),
            ])),
        );
        assert_eq!(decompile(&v), "x += 1");
    }

    #[test]
    fn decompile_navigation() {
        let v = Value::Call(vec![
            Value::Variable("user".into()),
            Value::String("name".into()),
        ]);
        assert_eq!(decompile(&v), "user.name");
    }

    #[test]
    fn decompile_when() {
        let v = Value::When(vec![
            Value::Variable("x".into()),
            Value::Variable("y".into()),
        ]);
        assert_eq!(decompile(&v), "when x do\n  y\nend");
    }

    #[test]
    fn decompile_when_else() {
        let v = Value::When(vec![
            Value::Variable("x".into()),
            Value::Integer(1),
            Value::Integer(2),
        ]);
        assert_eq!(decompile(&v), "when x do\n  1\nelse\n  2\nend");
    }

    #[test]
    fn decompile_or() {
        let v = Value::Or(vec![
            Value::Variable("a".into()),
            Value::Integer(100),
        ]);
        assert_eq!(decompile(&v), "a or 100");
    }

    #[test]
    fn decompile_for_in() {
        let v = Value::ForIn(vec![
            Value::Variable("items".into()),
            Value::Variable("x".into()),
            Value::Variable("x".into()),
        ]);
        assert_eq!(decompile(&v), "for x in items do\n  x\nend");
    }

    #[test]
    fn decompile_while() {
        let v = Value::While(vec![
            Value::Call(vec![
                Value::Opcode("gt".into()),
                Value::Variable("n".into()),
                Value::Integer(0),
            ]),
            Value::Variable("n".into()),
        ]);
        assert_eq!(decompile(&v), "while n > 0 do\n  n\nend");
    }

    #[test]
    fn decompile_list_comp() {
        let v = Value::ListCompIn(vec![
            Value::Variable("items".into()),
            Value::Call(vec![
                Value::Opcode("ml".into()),
                Value::SelfRef(0),
                Value::SelfRef(0),
            ]),
        ]);
        assert_eq!(decompile(&v), "[self * self in items]");
    }

    #[test]
    fn decompile_map() {
        let v = Value::Map(vec![
            (Value::String("a".into()), Value::Integer(1)),
            (Value::String("b".into()), Value::Integer(2)),
        ]);
        assert_eq!(decompile(&v), "{\n  a: 1\n  b: 2\n}");
    }

    #[test]
    fn decompile_empty_map() {
        assert_eq!(decompile(&Value::Map(vec![])), "{}");
    }

    #[test]
    fn decompile_list() {
        let v = Value::List(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]);
        assert_eq!(decompile(&v), "[1, 2, 3]");
    }

    #[test]
    fn decompile_delete() {
        let v = Value::Delete(Box::new(Value::Variable("x".into())));
        assert_eq!(decompile(&v), "delete x");
    }

    #[test]
    fn decompile_block() {
        let v = Value::Block(vec![
            Value::Set(
                Box::new(Value::Variable("x".into())),
                Box::new(Value::Integer(1)),
            ),
            Value::Variable("x".into()),
        ]);
        assert_eq!(decompile(&v), "x = 1\nx");
    }

    // ── Round-trip: compile → decompile ─────────────────────────

    fn roundtrip_source(source: &str) -> String {
        use crate::{lexer, parser, syntax, lower};
        let tokens = lexer::lex(source);
        let (green, errors) = parser::parse(source, &tokens);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let root = syntax::SyntaxNode::new_root(green);
        let value = lower::lower(&root);
        decompile(&value)
    }

    #[test]
    fn roundtrip_simple() {
        assert_eq!(roundtrip_source("42"), "42");
        assert_eq!(roundtrip_source("true"), "true");
        assert_eq!(roundtrip_source("null"), "null");
        assert_eq!(roundtrip_source("x"), "x");
        assert_eq!(roundtrip_source("self"), "self");
    }

    #[test]
    fn roundtrip_arithmetic() {
        assert_eq!(roundtrip_source("1 + 2"), "1 + 2");
        assert_eq!(roundtrip_source("1 + 2 * 3"), "1 + 2 * 3");
        assert_eq!(roundtrip_source("a - b"), "a - b");
    }

    #[test]
    fn roundtrip_assignment() {
        assert_eq!(roundtrip_source("x = 42"), "x = 42");
        assert_eq!(roundtrip_source("x += 1"), "x += 1");
    }

    #[test]
    fn roundtrip_logic() {
        assert_eq!(roundtrip_source("a or b"), "a or b");
        assert_eq!(roundtrip_source("a and b"), "a and b");
    }

    #[test]
    fn roundtrip_conditional() {
        assert_eq!(
            roundtrip_source("when x do y end"),
            "when x do\n  y\nend"
        );
    }

    #[test]
    fn roundtrip_for() {
        assert_eq!(
            roundtrip_source("for x in items do x end"),
            "for x in items do\n  x\nend"
        );
    }

    #[test]
    fn roundtrip_data() {
        assert_eq!(roundtrip_source("[1, 2, 3]"), "[1, 2, 3]");
        assert_eq!(roundtrip_source("{}"), "{}");
    }
}

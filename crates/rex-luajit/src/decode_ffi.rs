use std::ffi::{CString, c_char};

const REX_OK: i32 = 0;
const REX_ERR: i32 = 1;
const REX_EOF: i32 = 2;
const REX_TYPE: i32 = 3;

pub const REX_KIND_INT: i32 = 1;
pub const REX_KIND_DECIMAL: i32 = 2;
pub const REX_KIND_STRING: i32 = 3;
pub const REX_KIND_REF: i32 = 4;
pub const REX_KIND_VARIABLE: i32 = 5;
pub const REX_KIND_OPCODE: i32 = 6;
pub const REX_KIND_BREAK_CONT: i32 = 7;
pub const REX_KIND_POINTER: i32 = 8;
pub const REX_KIND_ARRAY: i32 = 9;
pub const REX_KIND_OBJECT: i32 = 10;
pub const REX_KIND_CALL: i32 = 11;
pub const REX_KIND_COMPOUND: i32 = 12;
pub const REX_KIND_CHAIN: i32 = 13;
pub const REX_KIND_SET: i32 = 14;
pub const REX_KIND_SWAP: i32 = 15;
pub const REX_KIND_DELETE: i32 = 16;
pub const REX_KIND_RETURN: i32 = 17;

#[derive(Clone, Copy)]
struct Frame {
    closer: u8,
    body_end: Option<usize>,
    indexed: bool,
    count: usize,
    width: usize,
    table_start: usize,
    body_start: usize,
    container_tag: u8,
}

pub struct RexCursor {
    data: Vec<u8>,
    pos: usize,
    stack: Vec<Frame>,
    last_error: CString,
}

#[inline]
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

fn parse_varint(input: &[u8], pos: &mut usize) -> (u64, usize) {
    let start = *pos;
    let mut n: u64 = 0;
    while *pos < input.len() {
        if let Some(v) = b64_val(input[*pos]) {
            n = n.saturating_mul(64).saturating_add(v as u64);
            *pos += 1;
        } else {
            break;
        }
    }
    (n, *pos - start)
}

fn varint_width(mut n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut w = 0;
    while n > 0 {
        w += 1;
        n /= 64;
    }
    w
}

fn encode_varint_ascii(mut n: usize, out: &mut Vec<u8>) {
    if n == 0 {
        return;
    }
    out.clear();
    while n > 0 {
        let digit = n % 64;
        let ch = if digit < 10 {
            b'0' + digit as u8
        } else if digit < 36 {
            b'a' + (digit as u8 - 10)
        } else if digit < 62 {
            b'A' + (digit as u8 - 36)
        } else if digit == 62 {
            b'-'
        } else {
            b'_'
        };
        out.push(ch);
        n /= 64;
    }
    out.reverse();
}

fn read_fixed_b64(input: &[u8], start: usize, width: usize) -> Option<usize> {
    if start.saturating_add(width) > input.len() {
        return None;
    }
    let mut n: usize = 0;
    for i in 0..width {
        let v = b64_val(input[start + i])? as usize;
        n = n.saturating_mul(64).saturating_add(v);
    }
    Some(n)
}

fn encoded_string_span_at(input: &[u8], pos: usize) -> Option<(usize, usize)> {
    if pos >= input.len() {
        return None;
    }
    let mut p = pos;
    let (len, _) = parse_varint(input, &mut p);
    if p >= input.len() || input[p] != b',' {
        return None;
    }
    p += 1;
    let end = p.saturating_add(len as usize);
    if end > input.len() {
        return None;
    }
    Some((pos, end))
}

fn skip_value_from(input: &[u8], mut pos: usize) -> Result<usize, &'static str> {
    if pos >= input.len() {
        return Err("unexpected end of input");
    }

    let (size_or_var, digits_len) = {
        let mut p = pos;
        parse_varint(input, &mut p)
    };
    pos += digits_len;

    if pos >= input.len() {
        return Err("unexpected end of input after varint");
    }

    let tag = input[pos];
    pos += 1;

    match tag {
        b'+' | b'\'' | b'$' | b'%' | b'\\' | b'^' => Ok(pos),
        b',' => {
            let len = size_or_var as usize;
            let end = pos.saturating_add(len);
            if end > input.len() {
                return Err("string length out of bounds");
            }
            Ok(end)
        }
        b'*' => skip_value_from(input, pos),
        b'.' => {
            let body_end = pos.saturating_add(size_or_var as usize);
            if body_end > input.len() {
                return Err("chain body out of bounds");
            }
            Ok(body_end)
        }
        b'=' | b'/' => {
            let p = skip_value_from(input, pos)?;
            skip_value_from(input, p)
        }
        b'~' | b';' => skip_value_from(input, pos),
        b'(' | b'[' | b'{' => {
            let closer = match tag {
                b'(' => b')',
                b'[' => b']',
                _ => b'}',
            };

            if digits_len > 0 {
                let body_end = pos.saturating_add(size_or_var as usize);
                if body_end >= input.len() {
                    return Err("sized container out of bounds");
                }
                if input[body_end] != closer {
                    return Err("sized container closer mismatch");
                }
                return Ok(body_end + 1);
            }

            // Unsized container. Handle indexed table prelude for [] and {}.
            if (tag == b'[' || tag == b'{') && pos < input.len() {
                let mut p = pos;
                let (packed, dlen) = parse_varint(input, &mut p);
                if dlen > 0 && p < input.len() && input[p] == b'#' {
                    let count = (packed >> 3) as usize;
                    let width = ((packed & 7) as usize) + 1;
                    p += 1;
                    let table_len = count.saturating_mul(width);
                    if p.saturating_add(table_len) > input.len() {
                        return Err("indexed table out of bounds");
                    }
                    pos = p + table_len;
                }
            }

            while pos < input.len() && input[pos] != closer {
                pos = skip_value_from(input, pos)?;
            }
            if pos >= input.len() {
                return Err("missing container closer");
            }
            Ok(pos + 1)
        }
        b'?' | b'!' | b'|' | b'&' | b'>' | b'<' | b'#' => {
            if pos >= input.len() {
                return Err("compound missing opener");
            }
            let open = input[pos];
            let closer = match open {
                b'(' => b')',
                b'[' => b']',
                b'{' => b'}',
                _ => return Err("invalid compound opener"),
            };
            pos += 1;

            if digits_len > 0 {
                let body_end = pos.saturating_add(size_or_var as usize);
                if body_end >= input.len() {
                    return Err("sized compound out of bounds");
                }
                if input[body_end] != closer {
                    return Err("sized compound closer mismatch");
                }
                return Ok(body_end + 1);
            }

            while pos < input.len() && input[pos] != closer {
                pos = skip_value_from(input, pos)?;
            }
            if pos >= input.len() {
                return Err("missing compound closer");
            }
            Ok(pos + 1)
        }
        _ => Err("unknown tag"),
    }
}

fn kind_from_tag(tag: u8) -> i32 {
    match tag {
        b'+' => REX_KIND_INT,
        b'*' => REX_KIND_DECIMAL,
        b',' => REX_KIND_STRING,
        b'\'' => REX_KIND_REF,
        b'$' => REX_KIND_VARIABLE,
        b'%' => REX_KIND_OPCODE,
        b'\\' => REX_KIND_BREAK_CONT,
        b'^' => REX_KIND_POINTER,
        b'[' => REX_KIND_ARRAY,
        b'{' => REX_KIND_OBJECT,
        b'(' => REX_KIND_CALL,
        b'?' | b'!' | b'|' | b'&' | b'>' | b'<' | b'#' => REX_KIND_COMPOUND,
        b'.' => REX_KIND_CHAIN,
        b'=' => REX_KIND_SET,
        b'/' => REX_KIND_SWAP,
        b'~' => REX_KIND_DELETE,
        b';' => REX_KIND_RETURN,
        _ => 0,
    }
}

impl RexCursor {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            pos: 0,
            stack: Vec::new(),
            last_error: CString::new("ok").expect("static string cannot contain null"),
        }
    }

    fn set_error(&mut self, msg: &str) {
        self.last_error = CString::new(msg).unwrap_or_else(|_| CString::new("invalid error").expect("static string cannot contain null"));
    }

    fn parse_header_at_pos(&self) -> Result<(u64, usize, u8, usize), &'static str> {
        if self.pos >= self.data.len() {
            return Err("eof");
        }
        let mut p = self.pos;
        let (v, digits_len) = parse_varint(&self.data, &mut p);
        if p >= self.data.len() {
            return Err("unexpected end of input after varint");
        }
        let tag = self.data[p];
        Ok((v, digits_len, tag, p + 1))
    }

    fn read_name_like(
        &mut self,
        expected_tag: u8,
        out_ptr: *mut *const c_char,
        out_len: *mut usize,
        what: &str,
    ) -> i32 {
        let start = self.pos;
        let (_, _, tag, body_start) = match self.parse_header_at_pos() {
            Ok(h) => h,
            Err(e) => {
                self.set_error(e);
                return REX_EOF;
            }
        };
        if tag != expected_tag {
            self.set_error(what);
            return REX_TYPE;
        }
        let name_end = body_start.saturating_sub(1);
        if name_end < start || name_end > self.data.len() {
            self.set_error("name span out of bounds");
            return REX_ERR;
        }
        unsafe {
            *out_ptr = self.data[start..name_end].as_ptr() as *const c_char;
            *out_len = name_end - start;
        }
        self.pos = body_start;
        REX_OK
    }

    fn open_container(&mut self, expected_tag: u8, out_indexed: *mut i32, out_count: *mut usize) -> i32 {
        let (varint, digits_len, tag, mut body_pos) = match self.parse_header_at_pos() {
            Ok(h) => h,
            Err(e) => {
                self.set_error(e);
                return REX_EOF;
            }
        };

        if tag != expected_tag {
            self.set_error("unexpected value kind for container open");
            return REX_TYPE;
        }

        let closer = match tag {
            b'[' => b']',
            b'{' => b'}',
            b'(' => b')',
            _ => {
                self.set_error("invalid container tag");
                return REX_ERR;
            }
        };

        let mut body_end: Option<usize> = None;
        if digits_len > 0 {
            let end = body_pos.saturating_add(varint as usize);
            if end >= self.data.len() {
                self.set_error("sized container body out of bounds");
                return REX_ERR;
            }
            if self.data[end] != closer {
                self.set_error("sized container closer mismatch");
                return REX_ERR;
            }
            body_end = Some(end);
        }

        if !out_indexed.is_null() {
            unsafe { *out_indexed = 0 };
        }
        if !out_count.is_null() {
            unsafe { *out_count = 0 };
        }

        let mut indexed = false;
        let mut count = 0usize;
        let mut width = 0usize;
        let mut table_start = 0usize;

        // Handle indexed prelude for [] and {} when unsized.
        if body_end.is_none() && (tag == b'[' || tag == b'{') {
            let mut p = body_pos;
            let (packed, dlen) = parse_varint(&self.data, &mut p);
            if dlen > 0 && p < self.data.len() && self.data[p] == b'#' {
                count = (packed >> 3) as usize;
                width = ((packed & 7) as usize) + 1;
                p += 1;
                table_start = p;
                let table_len = count.saturating_mul(width);
                if p.saturating_add(table_len) > self.data.len() {
                    self.set_error("indexed table out of bounds");
                    return REX_ERR;
                }
                indexed = true;
                if !out_indexed.is_null() {
                    unsafe { *out_indexed = 1 };
                }
                if !out_count.is_null() {
                    unsafe { *out_count = count };
                }
                body_pos = p + table_len;
            }
        }

        self.pos = body_pos;
        self.stack.push(Frame {
            closer,
            body_end,
            indexed,
            count,
            width,
            table_start,
            body_start: body_pos,
            container_tag: tag,
        });
        REX_OK
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_status_ok() -> i32 { REX_OK }

#[unsafe(no_mangle)]
pub extern "C" fn rex_status_err() -> i32 { REX_ERR }

#[unsafe(no_mangle)]
pub extern "C" fn rex_status_eof() -> i32 { REX_EOF }

#[unsafe(no_mangle)]
pub extern "C" fn rex_status_type() -> i32 { REX_TYPE }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_int() -> i32 { REX_KIND_INT }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_decimal() -> i32 { REX_KIND_DECIMAL }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_string() -> i32 { REX_KIND_STRING }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_ref() -> i32 { REX_KIND_REF }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_variable() -> i32 { REX_KIND_VARIABLE }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_opcode() -> i32 { REX_KIND_OPCODE }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_break_cont() -> i32 { REX_KIND_BREAK_CONT }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_pointer() -> i32 { REX_KIND_POINTER }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_array() -> i32 { REX_KIND_ARRAY }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_object() -> i32 { REX_KIND_OBJECT }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_call() -> i32 { REX_KIND_CALL }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_compound() -> i32 { REX_KIND_COMPOUND }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_chain() -> i32 { REX_KIND_CHAIN }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_set() -> i32 { REX_KIND_SET }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_swap() -> i32 { REX_KIND_SWAP }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_delete() -> i32 { REX_KIND_DELETE }

#[unsafe(no_mangle)]
pub extern "C" fn rex_kind_return() -> i32 { REX_KIND_RETURN }

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_new(data_ptr: *const c_char, data_len: usize) -> *mut RexCursor {
    if data_ptr.is_null() {
        return std::ptr::null_mut();
    }
    let bytes = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, data_len) };
    Box::into_raw(Box::new(RexCursor::new(bytes.to_vec())))
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_free(cursor: *mut RexCursor) {
    if !cursor.is_null() {
        unsafe { drop(Box::from_raw(cursor)) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_reset(cursor: *mut RexCursor) -> i32 {
    if cursor.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    c.pos = 0;
    c.stack.clear();
    c.set_error("ok");
    REX_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_pos(cursor: *const RexCursor) -> usize {
    if cursor.is_null() {
        return 0;
    }
    let c = unsafe { &*cursor };
    c.pos
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_len(cursor: *const RexCursor) -> usize {
    if cursor.is_null() {
        return 0;
    }
    let c = unsafe { &*cursor };
    c.data.len()
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_frame_indexed(cursor: *const RexCursor) -> i32 {
    if cursor.is_null() {
        return 0;
    }
    let c = unsafe { &*cursor };
    let Some(frame) = c.stack.last() else {
        return 0;
    };
    if frame.indexed { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_frame_count(cursor: *const RexCursor) -> usize {
    if cursor.is_null() {
        return 0;
    }
    let c = unsafe { &*cursor };
    let Some(frame) = c.stack.last() else {
        return 0;
    };
    frame.count
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_last_error(cursor: *const RexCursor) -> *const c_char {
    if cursor.is_null() {
        return std::ptr::null();
    }
    let c = unsafe { &*cursor };
    c.last_error.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_peek_kind(cursor: *mut RexCursor) -> i32 {
    if cursor.is_null() {
        return 0;
    }
    let c = unsafe { &mut *cursor };
    match c.parse_header_at_pos() {
        Ok((_, _, tag, _)) => kind_from_tag(tag),
        Err(e) => {
            c.set_error(e);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_skip_value(cursor: *mut RexCursor) -> i32 {
    if cursor.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    match skip_value_from(&c.data, c.pos) {
        Ok(new_pos) => {
            c.pos = new_pos;
            REX_OK
        }
        Err(e) => {
            c.set_error(e);
            REX_ERR
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_read_int(cursor: *mut RexCursor, out: *mut i64) -> i32 {
    if cursor.is_null() || out.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    let (v, _, tag, body_start) = match c.parse_header_at_pos() {
        Ok(h) => h,
        Err(e) => {
            c.set_error(e);
            return REX_EOF;
        }
    };
    if tag != b'+' {
        c.set_error("expected integer value");
        return REX_TYPE;
    }
    let n = if v % 2 == 0 {
        (v / 2) as i64
    } else {
        -((v / 2) as i64) - 1
    };
    unsafe { *out = n };
    c.pos = body_start;
    REX_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_read_string(
    cursor: *mut RexCursor,
    out_ptr: *mut *const c_char,
    out_len: *mut usize,
) -> i32 {
    if cursor.is_null() || out_ptr.is_null() || out_len.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    let (len, _, tag, body_start) = match c.parse_header_at_pos() {
        Ok(h) => h,
        Err(e) => {
            c.set_error(e);
            return REX_EOF;
        }
    };
    if tag != b',' {
        c.set_error("expected string value");
        return REX_TYPE;
    }
    let n = len as usize;
    let end = body_start.saturating_add(n);
    if end > c.data.len() {
        c.set_error("string out of bounds");
        return REX_ERR;
    }
    unsafe {
        *out_ptr = c.data[body_start..end].as_ptr() as *const c_char;
        *out_len = n;
    }
    c.pos = end;
    REX_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_read_ref(
    cursor: *mut RexCursor,
    out_ptr: *mut *const c_char,
    out_len: *mut usize,
) -> i32 {
    if cursor.is_null() || out_ptr.is_null() || out_len.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    c.read_name_like(b'\'', out_ptr, out_len, "expected ref value")
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_read_variable(
    cursor: *mut RexCursor,
    out_ptr: *mut *const c_char,
    out_len: *mut usize,
) -> i32 {
    if cursor.is_null() || out_ptr.is_null() || out_len.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    c.read_name_like(b'$', out_ptr, out_len, "expected variable value")
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_read_opcode(
    cursor: *mut RexCursor,
    out_ptr: *mut *const c_char,
    out_len: *mut usize,
) -> i32 {
    if cursor.is_null() || out_ptr.is_null() || out_len.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    c.read_name_like(b'%', out_ptr, out_len, "expected opcode value")
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_open_array(cursor: *mut RexCursor, out_indexed: *mut i32, out_count: *mut usize) -> i32 {
    if cursor.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    c.open_container(b'[', out_indexed, out_count)
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_open_object(cursor: *mut RexCursor, out_indexed: *mut i32, out_count: *mut usize) -> i32 {
    if cursor.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    c.open_container(b'{', out_indexed, out_count)
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_open_call(cursor: *mut RexCursor) -> i32 {
    if cursor.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    c.open_container(b'(', std::ptr::null_mut(), std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_at_end(cursor: *mut RexCursor) -> i32 {
    if cursor.is_null() {
        return 1;
    }
    let c = unsafe { &mut *cursor };
    let Some(frame) = c.stack.last().copied() else {
        return if c.pos >= c.data.len() { 1 } else { 0 };
    };

    if let Some(end) = frame.body_end {
        return if c.pos >= end { 1 } else { 0 };
    }

    if c.pos >= c.data.len() {
        return 1;
    }

    if c.data[c.pos] == frame.closer { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_close(cursor: *mut RexCursor) -> i32 {
    if cursor.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    let Some(frame) = c.stack.pop() else {
        c.set_error("no open container frame");
        return REX_ERR;
    };

    if let Some(end) = frame.body_end {
        c.pos = end;
    } else {
        while c.pos < c.data.len() && c.data[c.pos] != frame.closer {
            match skip_value_from(&c.data, c.pos) {
                Ok(next) => c.pos = next,
                Err(e) => {
                    c.set_error(e);
                    return REX_ERR;
                }
            }
        }
    }

    if c.pos >= c.data.len() || c.data[c.pos] != frame.closer {
        c.set_error("container closer mismatch");
        return REX_ERR;
    }

    c.pos += 1;
    REX_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_array_seek_index(cursor: *mut RexCursor, index: usize) -> i32 {
    if cursor.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    let Some(frame) = c.stack.last().copied() else {
        c.set_error("no open container frame");
        return REX_ERR;
    };
    if frame.container_tag != b'[' {
        c.set_error("array_seek_index requires open array frame");
        return REX_TYPE;
    }
    if !frame.indexed {
        c.set_error("array_seek_index requires indexed array (#)");
        return REX_TYPE;
    }
    if index >= frame.count {
        c.set_error("array index out of bounds");
        return REX_EOF;
    }

    let ptr_pos = frame.table_start.saturating_add(index.saturating_mul(frame.width));
    let Some(off) = read_fixed_b64(&c.data, ptr_pos, frame.width) else {
        c.set_error("invalid indexed array pointer table entry");
        return REX_ERR;
    };

    let abs = frame.body_start.saturating_add(off);
    if abs >= c.data.len() {
        c.set_error("indexed array pointer target out of bounds");
        return REX_ERR;
    }

    c.pos = abs;
    REX_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn rex_cursor_object_seek_key(
    cursor: *mut RexCursor,
    key_ptr: *const c_char,
    key_len: usize,
) -> i32 {
    if cursor.is_null() || key_ptr.is_null() {
        return REX_ERR;
    }
    let c = unsafe { &mut *cursor };
    let Some(frame) = c.stack.last().copied() else {
        c.set_error("no open container frame");
        return REX_ERR;
    };
    if frame.container_tag != b'{' {
        c.set_error("object_seek_key requires open object frame");
        return REX_TYPE;
    }
    if !frame.indexed {
        c.set_error("object_seek_key requires indexed object (#)");
        return REX_TYPE;
    }

    let key = unsafe { std::slice::from_raw_parts(key_ptr as *const u8, key_len) };
    let mut encoded_key = Vec::with_capacity(varint_width(key_len).saturating_add(1).saturating_add(key_len));
    encode_varint_ascii(key_len, &mut encoded_key);
    encoded_key.push(b',');
    encoded_key.extend_from_slice(key);

    let mut lo = 0usize;
    let mut hi = frame.count;

    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let ptr_pos = frame.table_start.saturating_add(mid.saturating_mul(frame.width));
        let Some(off) = read_fixed_b64(&c.data, ptr_pos, frame.width) else {
            c.set_error("invalid indexed object pointer table entry");
            return REX_ERR;
        };
        let pair_start = frame.body_start.saturating_add(off);
        let Some((k_start, k_end)) = encoded_string_span_at(&c.data, pair_start) else {
            c.set_error("indexed object key is not a valid encoded string");
            return REX_ERR;
        };

        let entry_key = &c.data[k_start..k_end];
        match entry_key.cmp(encoded_key.as_slice()) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => {
                c.pos = k_end;
                return REX_OK;
            }
        }
    }

    c.set_error("object key not found");
    REX_EOF
}

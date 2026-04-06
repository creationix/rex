# Rex Generic FFI Decoder API (Draft)

This document specifies a generic C ABI for lazy RX/REXC decoding.

Design goals:

- No Lua C API dependency (`lua_State*` is not used)
- Usable from LuaJIT FFI, Node FFI, Python ctypes/cffi, etc.
- Cursor-style navigation with cheap skipping for lazy workloads
- Stable status codes and explicit memory ownership

## Status Codes

- `REX_OK = 0`
- `REX_ERR = 1`
- `REX_EOF = 2`
- `REX_TYPE = 3`

In the current implementation these are exposed as functions:

- `rex_status_ok()`
- `rex_status_err()`
- `rex_status_eof()`
- `rex_status_type()`

## Value Kind Codes

- `REX_KIND_INT`
- `REX_KIND_DECIMAL`
- `REX_KIND_STRING`
- `REX_KIND_REF`
- `REX_KIND_VARIABLE`
- `REX_KIND_OPCODE`
- `REX_KIND_BREAK_CONT`
- `REX_KIND_POINTER`
- `REX_KIND_ARRAY`
- `REX_KIND_OBJECT`
- `REX_KIND_CALL`
- `REX_KIND_COMPOUND`
- `REX_KIND_CHAIN`
- `REX_KIND_SET`
- `REX_KIND_SWAP`
- `REX_KIND_DELETE`
- `REX_KIND_RETURN`

Each kind is exposed as a function, for example `rex_kind_int()`.

## Cursor Lifecycle

```c
RexCursor* rex_cursor_new(const char* data_ptr, size_t data_len);
void rex_cursor_free(RexCursor* cursor);
int rex_cursor_reset(RexCursor* cursor);
size_t rex_cursor_pos(const RexCursor* cursor);
size_t rex_cursor_len(const RexCursor* cursor);
int rex_cursor_frame_indexed(const RexCursor* cursor);
size_t rex_cursor_frame_count(const RexCursor* cursor);
const char* rex_cursor_last_error(const RexCursor* cursor);
```

Ownership:

- `rex_cursor_new` copies the input bytes into cursor-owned memory.
- Returned pointers from read APIs are valid while the cursor is alive.

## Reading and Navigation

```c
int rex_cursor_peek_kind(RexCursor* cursor);
int rex_cursor_skip_value(RexCursor* cursor);

int rex_cursor_read_int(RexCursor* cursor, int64_t* out);
int rex_cursor_read_string(RexCursor* cursor, const char** out_ptr, size_t* out_len);
int rex_cursor_read_ref(RexCursor* cursor, const char** out_ptr, size_t* out_len);
int rex_cursor_read_variable(RexCursor* cursor, const char** out_ptr, size_t* out_len);
int rex_cursor_read_opcode(RexCursor* cursor, const char** out_ptr, size_t* out_len);

int rex_cursor_open_array(RexCursor* cursor, int* out_indexed, size_t* out_count);
int rex_cursor_open_object(RexCursor* cursor, int* out_indexed, size_t* out_count);
int rex_cursor_open_call(RexCursor* cursor);

int rex_cursor_array_seek_index(RexCursor* cursor, size_t index);
int rex_cursor_object_seek_key(RexCursor* cursor, const char* key_ptr, size_t key_len);

int rex_cursor_at_end(RexCursor* cursor);
int rex_cursor_close(RexCursor* cursor);
```

Behavior notes:

- `peek_kind` does not advance.
- `skip_value` recursively skips exactly one full value.
- `open_array` / `open_object` detect indexed forms (`#`) and skip index tables.
- `out_indexed=1` means indexed form was detected.
- `out_count` is filled for indexed containers.
- `frame_indexed` and `frame_count` expose current-frame metadata without
  re-parsing headers in host code.
- `read_ref`, `read_variable`, `read_opcode` read symbolic scalar names directly.
- `array_seek_index` performs direct pointer-table lookup for indexed arrays.
- `object_seek_key` performs binary search over sorted key pointers for indexed objects.
- `at_end` checks the currently open container frame.
- `close` consumes the expected closer (`]`, `}`, `)`).

## LuaJIT FFI Example

```lua
local ffi = require("ffi")

ffi.cdef[[
  typedef struct RexCursor RexCursor;
  RexCursor* rex_cursor_new(const char* data_ptr, size_t data_len);
  void rex_cursor_free(RexCursor* cursor);
  int rex_cursor_peek_kind(RexCursor* cursor);
  int rex_cursor_skip_value(RexCursor* cursor);
  int rex_cursor_open_object(RexCursor* cursor, int* out_indexed, size_t* out_count);
  int rex_cursor_at_end(RexCursor* cursor);
  int rex_cursor_read_string(RexCursor* cursor, const char** out_ptr, size_t* out_len);
  int rex_cursor_close(RexCursor* cursor);
  int rex_kind_string(void);
  int rex_status_ok(void);
]]
```

Typical loop for object traversal:

1. `rex_cursor_open_object(...)`
2. while `rex_cursor_at_end(...) == 0`:
3. read key with `rex_cursor_read_string(...)`
4. inspect value kind via `rex_cursor_peek_kind(...)`
5. either decode value or `rex_cursor_skip_value(...)`
6. `rex_cursor_close(...)`

## Current Scope

Implemented:

- Cursor lifecycle
- Scalar reads for integer and string
- Symbolic reads for ref, variable, and opcode names
- Kind peeking
- Recursive skip
- Container open/close for array/object/call
- Indexed prelude skipping for array/object
- Frame metadata getters for indexed/count
- Direct indexed array element seek (`array_seek_index`)
- Indexed object key seek with binary search (`object_seek_key`)

Planned next:

- Direct decimal/ref/opcode/variable readers
- Fast object-key seek helper for indexed objects
- Borrowed child cursor handles for random access
- Optional "no-copy" constructor with caller-managed lifetime contract

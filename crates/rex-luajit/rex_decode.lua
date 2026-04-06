-- rex_decode.lua — Generic C-ABI cursor decoder bindings for LuaJIT FFI

local ffi = require("ffi")

ffi.cdef[[
  typedef struct RexCursor RexCursor;

  int rex_status_ok(void);
  int rex_status_err(void);
  int rex_status_eof(void);
  int rex_status_type(void);

  int rex_kind_int(void);
  int rex_kind_decimal(void);
  int rex_kind_string(void);
  int rex_kind_ref(void);
  int rex_kind_variable(void);
  int rex_kind_opcode(void);
  int rex_kind_break_cont(void);
  int rex_kind_pointer(void);
  int rex_kind_array(void);
  int rex_kind_object(void);
  int rex_kind_call(void);
  int rex_kind_compound(void);
  int rex_kind_chain(void);
  int rex_kind_set(void);
  int rex_kind_swap(void);
  int rex_kind_delete(void);
  int rex_kind_return(void);

  RexCursor* rex_cursor_new(const char* data_ptr, size_t data_len);
  void rex_cursor_free(RexCursor* cursor);
  int rex_cursor_reset(RexCursor* cursor);
  size_t rex_cursor_pos(const RexCursor* cursor);
  size_t rex_cursor_len(const RexCursor* cursor);
  int rex_cursor_frame_indexed(const RexCursor* cursor);
  size_t rex_cursor_frame_count(const RexCursor* cursor);
  const char* rex_cursor_last_error(const RexCursor* cursor);

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
]]

local script_dir = debug.getinfo(1, "S").source:match("@(.*/)") or "./"
local ext = ffi.os == "OSX" and "dylib" or (ffi.os == "Windows" and "dll" or "so")
local lib = ffi.load(script_dir .. "librex_luajit." .. ext)

local M = {}

M.STATUS_OK = tonumber(lib.rex_status_ok())
M.STATUS_ERR = tonumber(lib.rex_status_err())
M.STATUS_EOF = tonumber(lib.rex_status_eof())
M.STATUS_TYPE = tonumber(lib.rex_status_type())

M.KIND_INT = tonumber(lib.rex_kind_int())
M.KIND_DECIMAL = tonumber(lib.rex_kind_decimal())
M.KIND_STRING = tonumber(lib.rex_kind_string())
M.KIND_REF = tonumber(lib.rex_kind_ref())
M.KIND_VARIABLE = tonumber(lib.rex_kind_variable())
M.KIND_OPCODE = tonumber(lib.rex_kind_opcode())
M.KIND_BREAK_CONT = tonumber(lib.rex_kind_break_cont())
M.KIND_POINTER = tonumber(lib.rex_kind_pointer())
M.KIND_ARRAY = tonumber(lib.rex_kind_array())
M.KIND_OBJECT = tonumber(lib.rex_kind_object())
M.KIND_CALL = tonumber(lib.rex_kind_call())
M.KIND_COMPOUND = tonumber(lib.rex_kind_compound())
M.KIND_CHAIN = tonumber(lib.rex_kind_chain())
M.KIND_SET = tonumber(lib.rex_kind_set())
M.KIND_SWAP = tonumber(lib.rex_kind_swap())
M.KIND_DELETE = tonumber(lib.rex_kind_delete())
M.KIND_RETURN = tonumber(lib.rex_kind_return())

local Cursor = {}
Cursor.__index = Cursor

function Cursor:free()
  if self._ptr ~= nil then
    lib.rex_cursor_free(self._ptr)
    self._ptr = nil
  end
end

function Cursor:error()
  local p = lib.rex_cursor_last_error(self._ptr)
  if p == nil then return "" end
  return ffi.string(p)
end

function Cursor:pos()
  return tonumber(lib.rex_cursor_pos(self._ptr))
end

function Cursor:len()
  return tonumber(lib.rex_cursor_len(self._ptr))
end

function Cursor:frame_indexed()
  return tonumber(lib.rex_cursor_frame_indexed(self._ptr))
end

function Cursor:frame_count()
  return tonumber(lib.rex_cursor_frame_count(self._ptr))
end

function Cursor:peek_kind()
  return tonumber(lib.rex_cursor_peek_kind(self._ptr))
end

function Cursor:skip()
  return tonumber(lib.rex_cursor_skip_value(self._ptr))
end

function Cursor:read_int()
  local out = ffi.new("int64_t[1]")
  local st = tonumber(lib.rex_cursor_read_int(self._ptr, out))
  if st ~= M.STATUS_OK then return nil, st, self:error() end
  return tonumber(out[0]), st
end

function Cursor:read_string()
  local p = ffi.new("const char*[1]")
  local n = ffi.new("size_t[1]")
  local st = tonumber(lib.rex_cursor_read_string(self._ptr, p, n))
  if st ~= M.STATUS_OK then return nil, st, self:error() end
  return ffi.string(p[0], n[0]), st
end

function Cursor:read_ref()
  local p = ffi.new("const char*[1]")
  local n = ffi.new("size_t[1]")
  local st = tonumber(lib.rex_cursor_read_ref(self._ptr, p, n))
  if st ~= M.STATUS_OK then return nil, st, self:error() end
  return ffi.string(p[0], n[0]), st
end

function Cursor:read_variable()
  local p = ffi.new("const char*[1]")
  local n = ffi.new("size_t[1]")
  local st = tonumber(lib.rex_cursor_read_variable(self._ptr, p, n))
  if st ~= M.STATUS_OK then return nil, st, self:error() end
  return ffi.string(p[0], n[0]), st
end

function Cursor:read_opcode()
  local p = ffi.new("const char*[1]")
  local n = ffi.new("size_t[1]")
  local st = tonumber(lib.rex_cursor_read_opcode(self._ptr, p, n))
  if st ~= M.STATUS_OK then return nil, st, self:error() end
  return ffi.string(p[0], n[0]), st
end

function Cursor:open_array()
  local indexed = ffi.new("int[1]")
  local count = ffi.new("size_t[1]")
  local st = tonumber(lib.rex_cursor_open_array(self._ptr, indexed, count))
  return st, tonumber(indexed[0]), tonumber(count[0])
end

function Cursor:open_object()
  local indexed = ffi.new("int[1]")
  local count = ffi.new("size_t[1]")
  local st = tonumber(lib.rex_cursor_open_object(self._ptr, indexed, count))
  return st, tonumber(indexed[0]), tonumber(count[0])
end

function Cursor:open_call()
  return tonumber(lib.rex_cursor_open_call(self._ptr))
end

function Cursor:at_end()
  return tonumber(lib.rex_cursor_at_end(self._ptr)) ~= 0
end

function Cursor:array_seek_index(index)
  return tonumber(lib.rex_cursor_array_seek_index(self._ptr, index))
end

function Cursor:object_seek_key(key)
  return tonumber(lib.rex_cursor_object_seek_key(self._ptr, key, #key))
end

function Cursor:close()
  return tonumber(lib.rex_cursor_close(self._ptr))
end

function M.cursor(rx_or_rexc)
  local ptr = lib.rex_cursor_new(rx_or_rexc, #rx_or_rexc)
  if ptr == nil then
    error("failed to create cursor")
  end
  return setmetatable({ _ptr = ptr }, Cursor)
end

return M

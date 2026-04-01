-- rex_ffi.lua — Pure FFI encoder for LuaJIT (faster than C API path)
--
-- Usage:
--   local rex = require("rex_ffi")
--   local rx = rex.encode({hello = "world"})

local ffi = require("ffi")

ffi.cdef[[
  typedef struct FfiEncoder FfiEncoder;
  FfiEncoder* rex_enc_new(void);
  void rex_enc_free(FfiEncoder* enc);
  const char* rex_enc_finish(FfiEncoder* enc, size_t* out_len);
  void rex_enc_reset(FfiEncoder* enc);

  void rex_enc_null(FfiEncoder* enc);
  void rex_enc_boolean(FfiEncoder* enc, int val);
  void rex_enc_integer(FfiEncoder* enc, int64_t val);
  void rex_enc_decimal(FfiEncoder* enc, int64_t sig, int64_t exp);
  void rex_enc_number(FfiEncoder* enc, double val);
  void rex_enc_string(FfiEncoder* enc, const char* ptr, size_t len);
  void rex_enc_key(FfiEncoder* enc, const char* ptr, size_t len);
  void rex_enc_open_array(FfiEncoder* enc);
  void rex_enc_close_array(FfiEncoder* enc);
  void rex_enc_open_object(FfiEncoder* enc);
  void rex_enc_close_object(FfiEncoder* enc);
]]

local script_dir = debug.getinfo(1, "S").source:match("@(.*/)")  or "./"
local ext = ffi.os == "OSX" and "dylib" or (ffi.os == "Windows" and "dll" or "so")
local lib = ffi.load(script_dir .. "librex_luajit." .. ext)

local M = {}

local function write(enc, v)
  local t = type(v)
  if t == "table" then
    local n = #v
    if n > 0 then
      lib.rex_enc_open_array(enc)
      for i = 1, n do
        write(enc, v[i])
      end
      lib.rex_enc_close_array(enc)
    else
      lib.rex_enc_open_object(enc)
      for k, val in pairs(v) do
        local ks = tostring(k)
        lib.rex_enc_key(enc, ks, #ks)
        write(enc, val)
      end
      lib.rex_enc_close_object(enc)
    end
  elseif t == "string" then
    lib.rex_enc_string(enc, v, #v)
  elseif t == "number" then
    if v % 1 == 0 and v >= -2^53 and v <= 2^53 then
      lib.rex_enc_integer(enc, v)
    else
      lib.rex_enc_number(enc, v)
    end
  elseif t == "boolean" then
    lib.rex_enc_boolean(enc, v and 1 or 0)
  else
    lib.rex_enc_null(enc)
  end
end

-- Reusable encoder instance
local _enc = lib.rex_enc_new()

--- Encode a Lua value to RX bytecode.
--- @param value any
--- @return string
function M.encode(value)
  lib.rex_enc_reset(_enc)
  write(_enc, value)
  local len = ffi.new("size_t[1]")
  local ptr = lib.rex_enc_finish(_enc, len)
  return ffi.string(ptr, len[0])
end

return M

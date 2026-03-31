-- rex_eval.lua — FFI eval bindings for LuaJIT
--
-- Usage:
--   local rex = require("rex_eval")
--   local ctx = rex.context()
--   ctx:bind("method", '"GET"')           -- RX-encoded string
--   ctx:bind_mut("status", '38+')         -- RX-encoded integer 200
--   ctx:gas(1000000)
--   local result = ctx:eval(bytecode)
--   print(result.value)         -- RX bytes of return value
--   print(result.mutations)     -- RX object of dirty extern mut bindings
--   print(result.gas)           -- gas used
--   result:free()
--   ctx:free()

local ffi = require("ffi")

ffi.cdef[[
  typedef struct EvalContext EvalContext;
  typedef struct EvalResult EvalResult;

  EvalContext* rex_ctx_new(void);
  void rex_ctx_free(EvalContext* ctx);
  void rex_ctx_gas(EvalContext* ctx, uint64_t limit);
  void rex_ctx_bind(EvalContext* ctx, const char* name, size_t name_len,
                    const char* rx, size_t rx_len);
  void rex_ctx_bind_mut(EvalContext* ctx, const char* name, size_t name_len,
                        const char* rx, size_t rx_len);
  void rex_ctx_reset(EvalContext* ctx);
  EvalResult* rex_ctx_eval(EvalContext* ctx, const char* bytecode, size_t bc_len);

  const char* rex_result_value(const EvalResult* result, size_t* out_len);
  const char* rex_result_mutations(const EvalResult* result, size_t* out_len);
  uint64_t rex_result_gas(const EvalResult* result);
  void rex_result_free(EvalResult* result);
]]

local script_dir = debug.getinfo(1, "S").source:match("@(.*/)")  or "./"
local ext = ffi.os == "OSX" and "dylib" or (ffi.os == "Windows" and "dll" or "so")
local lib = ffi.load(script_dir .. "librex_luajit." .. ext)

-- ── Result wrapper ─────────────────────────────────────────────────────

local Result = {}
Result.__index = Result

function Result:free()
  if self._ptr ~= nil then
    lib.rex_result_free(self._ptr)
    self._ptr = nil
  end
end

-- ── Context wrapper ────────────────────────────────────────────────────

local Ctx = {}
Ctx.__index = Ctx

function Ctx:bind(name, rx)
  lib.rex_ctx_bind(self._ptr, name, #name, rx, #rx)
end

function Ctx:bind_mut(name, rx)
  lib.rex_ctx_bind_mut(self._ptr, name, #name, rx, #rx)
end

function Ctx:gas(limit)
  lib.rex_ctx_gas(self._ptr, limit)
end

function Ctx:reset()
  lib.rex_ctx_reset(self._ptr)
end

function Ctx:eval(bytecode)
  local rptr = lib.rex_ctx_eval(self._ptr, bytecode, #bytecode)

  local vlen = ffi.new("size_t[1]")
  local vptr = lib.rex_result_value(rptr, vlen)

  local mlen = ffi.new("size_t[1]")
  local mptr = lib.rex_result_mutations(rptr, mlen)

  local r = setmetatable({
    _ptr = rptr,
    value = ffi.string(vptr, vlen[0]),
    mutations = ffi.string(mptr, mlen[0]),
    gas = tonumber(lib.rex_result_gas(rptr)),
  }, Result)

  return r
end

function Ctx:free()
  if self._ptr ~= nil then
    lib.rex_ctx_free(self._ptr)
    self._ptr = nil
  end
end

-- ── Module ─────────────────────────────────────────────────────────────

local M = {}

function M.context()
  return setmetatable({ _ptr = lib.rex_ctx_new() }, Ctx)
end

return M

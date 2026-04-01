-- test_eval.lua — Test the FFI eval round-trip
--
-- Run: luajit test_eval.lua (from crates/rex-luajit/)
-- Requires: librex_luajit.dylib built via `cargo build -p rex-luajit`

local rex_eval = require("rex_eval")
local rex_native = require("rex_native")

local passed = 0
local failed = 0

local function test(name, fn)
  local ok, err = pcall(fn)
  if ok then
    passed = passed + 1
    print("  ✓ " .. name)
  else
    failed = failed + 1
    print("  ✗ " .. name .. ": " .. tostring(err))
  end
end

local function assert_eq(a, b, msg)
  if a ~= b then
    error((msg or "assert_eq") .. ": expected " .. tostring(b) .. ", got " .. tostring(a))
  end
end

print("rex_eval tests:")

-- Helper: compile Rex source to REXC bytecode
local function compile(source)
  -- Use rex_native.compile via Lua C API
  return rex_native.compile(source)
end

-- ── Basic eval ─────────────────────────────────────────────────────────

test("eval simple integer", function()
  local ctx = rex_eval.context()
  local result = ctx:eval(compile("42"))
  assert_eq(result.value, "1k+")  -- zigzag(42) = 84 = "1k" in b64
  assert_eq(result.mutations, "")
  result:free()
  ctx:free()
end)

test("eval string", function()
  local ctx = rex_eval.context()
  local result = ctx:eval(compile('"hello"'))
  assert_eq(result.value, "5,hello")
  result:free()
  ctx:free()
end)

-- ── Read-only bindings ─────────────────────────────────────────────────

test("read-only binding", function()
  local ctx = rex_eval.context()
  ctx:bind("method", "3,GET")  -- RX string "GET"
  local result = ctx:eval(compile("method"))
  assert_eq(result.value, "3,GET")
  assert_eq(result.mutations, "")
  result:free()
  ctx:free()
end)

-- ── Mutable bindings with COW ──────────────────────────────────────────

test("mut binding not written = no mutations", function()
  local ctx = rex_eval.context()
  ctx:bind_mut("status", "6g+")  -- RX integer 200
  local result = ctx:eval(compile("status"))  -- just read it
  assert_eq(result.value, "6g+")  -- returns 200
  assert_eq(result.mutations, "")  -- no writes
  result:free()
  ctx:free()
end)

test("mut binding written = mutation captured", function()
  local ctx = rex_eval.context()
  ctx:bind_mut("status", "6g+")  -- 200
  local result = ctx:eval(compile("status = 403"))
  -- Return value is 403 (assignment returns the value)
  assert_eq(result.value, "cC+")  -- zigzag(403) = 806 = "ce" in b64
  -- Mutations should contain status = 403
  assert(#result.mutations > 0, "expected non-empty mutations")
  result:free()
  ctx:free()
end)

test("mut binding unchanged = no mutation", function()
  local ctx = rex_eval.context()
  ctx:bind_mut("status", "6g+")  -- 200
  local result = ctx:eval(compile("status = 200"))  -- write same value
  assert_eq(result.mutations, "")  -- same value, not dirty
  result:free()
  ctx:free()
end)

-- ── Mixed bindings ─────────────────────────────────────────────────────

test("mixed readonly + mut", function()
  local ctx = rex_eval.context()
  ctx:bind("method", "3,GET")
  ctx:bind_mut("status", "6g+")  -- 200
  local source = [[
    when method == "GET" do
      status = 200
    else
      status = 405
    end
  ]]
  local result = ctx:eval(compile(source))
  -- method is "GET", so status stays 200 — no mutation
  assert_eq(result.mutations, "")
  result:free()
  ctx:free()
end)

test("mixed readonly + mut with change", function()
  local ctx = rex_eval.context()
  ctx:bind("method", "4,POST")
  ctx:bind_mut("status", "6g+")  -- 200
  local source = [[
    when method == "GET" do
      status = 200
    else
      status = 405
    end
  ]]
  local result = ctx:eval(compile(source))
  -- method is "POST", so status changes to 405 — mutation captured
  assert(#result.mutations > 0, "expected mutations for status change")
  result:free()
  ctx:free()
end)

-- ── Gas limit ──────────────────────────────────────────────────────────

test("gas tracking", function()
  local ctx = rex_eval.context()
  ctx:gas(1000000)
  local result = ctx:eval(compile("42"))
  assert(result.gas >= 0)
  result:free()
  ctx:free()
end)

-- ── Summary ────────────────────────────────────────────────────────────

print(string.format("\n%d passed, %d failed", passed, failed))
if failed > 0 then os.exit(1) end

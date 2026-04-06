-- test_decode.lua — Tests for generic C-ABI cursor decoder

package.cpath = "./?.so;" .. package.cpath

local rex_native = require("rex_native")
local decode = require("rex_decode")

local passed, failed = 0, 0

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

print("rex_decode tests:")

test("read top-level integer", function()
  local c = decode.cursor(rex_native.compile("42"))
  assert_eq(c:peek_kind(), decode.KIND_INT)
  local n, st = c:read_int()
  assert_eq(st, decode.STATUS_OK)
  assert_eq(n, 42)
  c:free()
end)

test("lazy object scan and selective decode", function()
  local rx = rex_native.encode({
    name = "Ada",
    age = 42,
    meta = { active = true, tags = {"a", "b", "c"} },
    notes = "skip me",
  })

  local c = decode.cursor(rx)

  local st = c:open_object()
  assert_eq(st, decode.STATUS_OK)

  local name, age = nil, nil

  while not c:at_end() do
    local key
    key, st = c:read_string()
    assert_eq(st, decode.STATUS_OK)

    local kind = c:peek_kind()
    if key == "name" then
      name, st = c:read_string()
      assert_eq(st, decode.STATUS_OK)
    elseif key == "age" then
      age, st = c:read_int()
      assert_eq(st, decode.STATUS_OK)
    else
      st = c:skip()
      assert_eq(st, decode.STATUS_OK)
    end
  end

  st = c:close()
  assert_eq(st, decode.STATUS_OK)
  assert_eq(name, "Ada")
  assert_eq(age, 42)
  c:free()
end)

test("nested arrays can be skipped lazily", function()
  local rx = rex_native.encode({
    kind = "payload",
    rows = { {id = 1}, {id = 2}, {id = 3} },
    status = 200,
  })

  local c = decode.cursor(rx)
  local st = c:open_object()
  assert_eq(st, decode.STATUS_OK)

  local status = nil
  while not c:at_end() do
    local key
    key, st = c:read_string()
    assert_eq(st, decode.STATUS_OK)

    if key == "status" then
      status, st = c:read_int()
      assert_eq(st, decode.STATUS_OK)
    else
      st = c:skip()
      assert_eq(st, decode.STATUS_OK)
    end
  end

  st = c:close()
  assert_eq(st, decode.STATUS_OK)
  assert_eq(status, 200)
  c:free()
end)

test("indexed array supports direct index seek", function()
  local rexc = rex_native.compile("[# 10 20 30 40]")
  local c = decode.cursor(rexc)

  local st, indexed, count = c:open_array()
  assert_eq(st, decode.STATUS_OK)
  assert_eq(indexed, 1)
  assert_eq(count, 4)
  assert_eq(c:frame_indexed(), 1)
  assert_eq(c:frame_count(), 4)

  st = c:array_seek_index(2)
  assert_eq(st, decode.STATUS_OK)

  local v
  v, st = c:read_int()
  assert_eq(st, decode.STATUS_OK)
  assert_eq(v, 30)

  st = c:close()
  assert_eq(st, decode.STATUS_OK)
  c:free()
end)

test("indexed object supports binary-search key seek", function()
  local rexc = rex_native.compile("{# z:1 b:2 m:3 a:4}")
  local c = decode.cursor(rexc)

  local st, indexed = c:open_object()
  assert_eq(st, decode.STATUS_OK)
  assert_eq(indexed, 1)
  assert_eq(c:frame_indexed(), 1)
  assert_eq(c:frame_count(), 4)

  st = c:object_seek_key("m")
  assert_eq(st, decode.STATUS_OK)

  local v
  v, st = c:read_int()
  assert_eq(st, decode.STATUS_OK)
  assert_eq(v, 3)

  st = c:object_seek_key("missing")
  assert_eq(st, decode.STATUS_EOF)

  st = c:close()
  assert_eq(st, decode.STATUS_OK)
  c:free()
end)

test("reads variable/ref/opcode symbols", function()
  local c1 = decode.cursor(rex_native.compile("status"))
  assert_eq(c1:peek_kind(), decode.KIND_VARIABLE)
  local name, st = c1:read_variable()
  assert_eq(st, decode.STATUS_OK)
  assert_eq(name, "status")
  c1:free()

  local c2 = decode.cursor(rex_native.compile("true"))
  local ref
  ref, st = c2:read_ref()
  assert_eq(st, decode.STATUS_OK)
  assert_eq(ref, "t")
  c2:free()

  local c3 = decode.cursor(rex_native.compile("1 + 2"))
  st = c3:open_call()
  assert_eq(st, decode.STATUS_OK)
  local op
  op, st = c3:read_opcode()
  assert_eq(st, decode.STATUS_OK)
  assert_eq(op, "ad")
  local n1, n2
  n1, st = c3:read_int()
  assert_eq(st, decode.STATUS_OK)
  n2, st = c3:read_int()
  assert_eq(st, decode.STATUS_OK)
  assert_eq(n1, 1)
  assert_eq(n2, 2)
  assert_eq(c3:close(), decode.STATUS_OK)
  c3:free()
end)

print(string.format("\n%d passed, %d failed", passed, failed))
if failed > 0 then os.exit(1) end

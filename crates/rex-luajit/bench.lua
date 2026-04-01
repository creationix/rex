package.cpath = "./?.so;" .. package.cpath

local rex_capi = require("rex_native")
local rex_ffi = require("rex_ffi")
local ffi = require("ffi")
ffi.cdef[[ typedef unsigned long clock_t; clock_t clock(void); ]]

local runs = 10

local function bench(name, fn)
  fn() -- warm up
  local times = {}
  local output
  for i = 1, runs do
    local t = ffi.C.clock()
    output = fn()
    times[i] = tonumber(ffi.C.clock() - t) / 1000
  end
  table.sort(times)
  local median = times[math.ceil(runs / 2)]
  print(string.format("  %-24s %8d bytes  %8.2f ms", name, #output, median))
  return output
end

local function make_users(n)
  local t = {}
  for i = 1, n do
    t[i] = {
      name = "user-" .. i,
      email = "user" .. i .. "@example.com",
      active = (i % 3 ~= 0),
      score = i * 17 % 100,
      tags = {"alpha", "beta", "gamma"},
      address = {
        street = "123 Main St",
        city = "Springfield",
        zip = string.format("%05d", i % 100000),
      }
    }
  end
  return t
end

print("=== C API vs FFI: encode(lua_table) ===")
print()

for _, n in ipairs({100, 1000, 10000}) do
  local data = make_users(n)
  print(string.format("%d users:", n))
  local out_capi = bench("C API (rex_native)", function() return rex_capi.encode(data) end)
  local out_ffi  = bench("FFI   (rex_ffi)",    function() return rex_ffi.encode(data) end)
  -- Verify same output
  if out_capi == out_ffi then
    print("  outputs match ✓")
  else
    print(string.format("  outputs differ: capi=%d ffi=%d", #out_capi, #out_ffi))
  end
  print()
end

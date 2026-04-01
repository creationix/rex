-- bench_eval.lua — Benchmark the FFI eval round-trip
--
-- Measures: compile once, eval many times with different bindings.
-- Simulates a CDN edge middleware evaluating routing rules per request.

local ffi = require("ffi")
local rex_eval = require("rex_eval")
local rex_native = require("rex_native")

-- ── Compile the middleware once ─────────────────────────────────────────

local source = [[
  when method == "GET" and path == "/api/users" do
    status = 200
    headers.content-type = "application/json"
  else when method == "POST" and path == "/api/users" do
    status = 201
    headers.content-type = "application/json"
  else when method == "GET" and path == "/health" do
    status = 200
    headers.content-type = "text/plain"
  else
    status = 404
    headers.x-error = "not found"
  end
]]

local bytecode = rex_native.compile(source)
print(string.format("Bytecode: %d bytes", #bytecode))

-- ── RX-encoded test inputs ──────────────────────────────────────────────

-- Encode test values using the FFI encoder
local rex_ffi = require("rex_ffi")

local function rx(val)
  return rex_ffi.encode(val)
end

local requests = {
  { method = rx("GET"),  path = rx("/api/users") },
  { method = rx("POST"), path = rx("/api/users") },
  { method = rx("GET"),  path = rx("/health") },
  { method = rx("GET"),  path = rx("/unknown") },
}

local empty_headers = rx({})
local initial_status = rx(0)

-- ── Warmup ──────────────────────────────────────────────────────────────

local ctx = rex_eval.context()
ctx:gas(100000)

for i = 1, 100 do
  local req = requests[(i % #requests) + 1]
  ctx:reset()
  ctx:bind("method", req.method)
  ctx:bind("path", req.path)
  ctx:bind_mut("status", initial_status)
  ctx:bind_mut("headers", empty_headers)
  local result = ctx:eval(bytecode)
  result:free()
end

-- ── Benchmark ───────────────────────────────────────────────────────────

local iterations = 100000

-- Time the full cycle: reset + bind + eval + read result + free
local clock = ffi.C or ffi
-- Use os.clock for CPU time
local start = os.clock()
for i = 1, iterations do
  local req = requests[(i % #requests) + 1]
  ctx:reset()
  ctx:bind("method", req.method)
  ctx:bind("path", req.path)
  ctx:bind_mut("status", initial_status)
  ctx:bind_mut("headers", empty_headers)
  local result = ctx:eval(bytecode)
  -- Touch the results to prevent optimization
  local _ = result.value
  local _ = result.mutations
  result:free()
end
local elapsed = os.clock() - start

ctx:free()

print(string.format("\n── Eval benchmark ──"))
print(string.format("Iterations:  %d", iterations))
print(string.format("Total time:  %.3f s", elapsed))
print(string.format("Per eval:    %.1f µs", elapsed / iterations * 1e6))
print(string.format("Throughput:  %.0f evals/sec", iterations / elapsed))

-- ── Compile benchmark ───────────────────────────────────────────────────

local compile_iters = 10000
start = os.clock()
for i = 1, compile_iters do
  rex_native.compile(source)
end
elapsed = os.clock() - start

print(string.format("\n── Compile benchmark ──"))
print(string.format("Iterations:  %d", compile_iters))
print(string.format("Total time:  %.3f s", elapsed))
print(string.format("Per compile: %.1f µs", elapsed / compile_iters * 1e6))

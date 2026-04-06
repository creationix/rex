package.cpath = "./?.so;" .. package.cpath

local rex = require("rex_native")

-- Test encode with Lua values
print("encode tests:")
print("  42          →", rex.encode(42))
print('  "hello"     →', rex.encode("hello"))
print("  true        →", rex.encode(true))
print("  false       →", rex.encode(false))
print("  nil         →", rex.encode(nil))
print("  {1,2,3}     →", rex.encode({1, 2, 3}))
print("  {a=1}       →", rex.encode({a = 1}))
print("  {a=1, b=2}  →", rex.encode({a = 1, b = 2}))
print("  nested      →", rex.encode({users = {{name = "Ada"}, {name = "Ben"}}}))

-- Test compile
print("\ncompile tests:")
print("  1 + 2       →", rex.compile("1 + 2"))
print("  x = 42      →", rex.compile("x = 42"))

-- Domain-aware APIs
local domain = [[
extern method: str
extern mut status: int
]]

print("\ndomain compile/check tests:")
print("  compile_with_domain(status = 200) →", rex.compile_with_domain("status = 200", domain))

local ok_diags = rex.check("status = 200", domain)
print("  check(status = 200) diagnostics    →", #ok_diags)

local bad_diags = rex.check("x: int = \"oops\"", domain)
print("  check(x: int = \"oops\") diagnostics →", #bad_diags)
if #bad_diags > 0 then
	print("    first:", bad_diags[1].kind, bad_diags[1].start, bad_diags[1]["end"], bad_diags[1].message)
end

print("\ndone!")

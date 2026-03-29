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

print("\ndone!")

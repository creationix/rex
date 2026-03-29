-- rex.lua — LuaJIT bindings for the Rex encoder
--
-- Usage:
--   local rex = require("rex")
--   local rx = rex.encode({hello = "world"})
--   local rexc = rex.compile("1 + 2")
--
-- The native module (rex_native.so/.dylib/.dll) must be on package.cpath.

return require("rex_native")

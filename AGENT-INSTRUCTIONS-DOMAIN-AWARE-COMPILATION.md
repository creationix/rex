# Instructions: Domain-Aware Compilation

## Goal

Make the Rex compiler read `.rexd` declarations so it can emit opcode calls directly instead of generating variable navigation that requires runtime namespace HostObjects.

## The Problem

When Rex source says `time.uuid()`, the compiler has no idea this is a function call. It sees a variable `time` being navigated with key `"uuid"`, then called with no arguments. The bytecode it emits:

```
((time$ 4,uuid))     →  call(call($time, "uuid"))
```

The interpreter evaluates this in two steps:
1. Inner call: navigate `$time` with key `"uuid"` → HostObject returns `"%tu"`
2. Outer call: dispatch `"%tu"` as opcode → `op_time_uuid([])`

Rex-serve creates 9 `OpcodeNamespace` HostObjects per request just to make step 1 work. These are pure overhead — the mapping `time.uuid → %tu` is static.

## The Solution

If the compiler reads the `.rexd` file and sees:

```rex
extern time.uuid() -> string
```

It knows `time.uuid()` is a host function, not a variable navigation. It can emit:

```
(tu%)     →  call(%tu)
```

One step, no HostObject needed. The opcode mnemonic `tu` comes from the dotted name: `time.uuid` → some deterministic mapping to a short code.

## How It Differs from Refs

The existing **refs system** (`'H`, `'M`, `'S`) maps names to values via `.config.rex`:

```
headers = 'H
method = 'M
```

This was designed for the JS/Vercel edge runtime where short codes are manually assigned. It handles **data bindings** — `headers` resolves to a value in `Context.refs`.

Domain-aware compilation handles **function resolution** — `time.uuid()` resolves to an opcode call. The two are complementary:

| | Refs (`.config.rex`) | Domain-aware (`.rexd`) |
|---|---|---|
| **What** | Data bindings: `headers`, `method`, `body` | Function calls: `time.uuid()`, `json.parse()` |
| **How** | Compiler substitutes variable → ref code | Compiler substitutes dotted call → opcode |
| **When** | Compile time (source transformation) | Compile time (lowering) |
| **Runtime** | `Context.refs` HashMap lookup | Direct opcode dispatch |

## Opcode Mnemonic Mapping

The `.rexd` file declares functions with dotted names. The compiler needs to map these to opcode mnemonics. Options:

### Option A: Convention-based (recommended)

Derive the mnemonic from the function name — first letter of namespace + first letter of method:

```
time.now    → tn
time.uuid   → tu
json.parse  → jp
db.get      → dg
db.set      → ds
fs.read     → fr
html.escape → he
```

This is what rex-serve already uses. The convention is simple and predictable. The `.rexd` file doesn't need to specify mnemonics — the compiler derives them.

### Option B: Explicit annotation

Add mnemonic annotations to `.rexd`:

```rex
extern json.parse(text: string) -> some  // @opcode jp
```

More flexible but adds noise to the interface file.

### Option C: Hash-based

Generate a short hash from the full function name. Deterministic but not human-readable in bytecode dumps.

**Recommendation: Option A** with a fallback to full-name encoding for collisions. The first two letters of `namespace.method` cover all current rex-serve functions without collisions.

## What Changes

### Compiler (`crates/rex-core/`)

1. **`lib.rs`**: Add `compile_with_domain(source: &str, domain: &str) -> String` that parses the `.rexd` file and passes the function declarations to the lowerer.

2. **`lower.rs`**: During lowering, when a `PostfixExpr` is `identifier.identifier(args)`:
   - Check if `namespace.method` matches a declared `extern` function
   - If yes: emit `Value::Call([Value::Opcode(mnemonic), ...args])` instead of `Value::Call([Value::Call([Value::Variable(ns), Value::String(method)]), ...args])`
   - If no: emit as before (variable navigation)

3. **No interpreter changes needed** — opcodes are already dispatched by mnemonic.

### rex-serve (`crates/rex-serve/`)

1. **`router.rs`**: Call `compile_with_domain(source, domain_content)` instead of `compile(source)`. Read the `.rexd` file once at startup.

2. **`handler.rs`**: Remove the 9 `OpcodeNamespace` HostObjects (`ns_time`, `ns_json`, etc.) and the corresponding `vars.insert("time", Host(5))` lines. The opcodes are now called directly.

3. **`refs.rs`**: The `OpcodeNamespace` struct can be removed entirely (or kept for the `html` tagged template, which uses `HostObject::call`).

### Tagged templates — special case

The `html` tagged template compiles as `call($html, [parts], values)`. With domain-aware compilation, the compiler could recognize `html` as a tag function from the `.rexd` declaration and emit an opcode call directly. But the current `HostObject::call` pattern also works. Either approach is fine.

## What Stays the Same

- **Data bindings** (`headers`, `method`, `body`, `res`, `params`) remain as vars. The `.rexd` file declares them as `extern` data, not functions.
- **The opcode registry** in `handler.rs` stays. Opcodes are still registered as `fn` pointers in the `Context.opcodes` HashMap.
- **HostObjects for request/response** (`HeadersObject`, `ResponseObject`, etc.) stay. These provide runtime behavior (case-insensitive header lookup, mutable status).

## Verification

```sh
# 1. Compile with domain — should produce opcode calls
echo 'time.uuid()' | rex compile --domain rex-serve.rexd
# Before: ((time$ 4,uuid))
# After:  (tu%)

# 2. Compile without domain — should fall back to variable navigation
echo 'time.uuid()' | rex compile
# Still: ((time$ 4,uuid))

# 3. rex-serve should work with fewer host objects
cargo run -p rex-serve -- --dir examples/knowledge-base --port 4000
# All routes, middleware, API, tour pages work as before

# 4. Benchmark — should be faster without namespace HostObjects
ab -n 5000 -c 50 http://localhost:4000/health
# Fewer allocations per request
```

## Migration Path

This can be done incrementally:

1. **Phase 1**: Add `compile_with_domain` to rex-core. Rex-serve continues to work as before — the domain-aware path is opt-in.

2. **Phase 2**: Update rex-serve's `router.rs` to use `compile_with_domain`. Remove namespace HostObjects. Verify all tests pass.

3. **Phase 3**: Make `rex check` and `compile_with_domain` share the `.rexd` parsing code. The type checker already parses these files — reuse that parser.

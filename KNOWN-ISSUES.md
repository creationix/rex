# Known Issues

## Rust Interpreter

### Object mutation doesn't work on local values

`obj.key = value` and `obj.(key) = value` don't modify the object. The cursor interpreter uses value semantics — objects are immutable once created. Property writes silently do nothing.

```rex
obj = {x: 1}
obj.x = 2
obj           // still {x: 1}

composites = {}
composites.(4) = true
composites.(4)  // none — not set
```

**Impact:** The primes sieve sample (`samples/primes.rex`) doesn't work. Any algorithm that builds up an object with mutations in a loop is broken.

**Workaround:** Use comprehensions to build objects in one pass instead of mutating:
```rex
// Instead of:
result = {}
for k, v in data do result.(k) = v * 2 end

// Use:
result = {(k): v * 2 for k, v in data}
```

**Fix:** The interpreter needs either mutable reference semantics for local variables (copy-on-write or ref-counted objects) or a host object system for mutable containers. The rex-serve host objects (`req`, `res`, `headers`) work correctly because they go through the `HostObject` trait which handles mutation.

### Array methods not implemented

`.push()`, `.pop()`, `.shift()`, `.unshift()`, `.slice()`, `.join()` are documented in the language spec but not implemented in the Rust interpreter. They return `none`.

```rex
[1, 2].push(3)     // none (should be [1, 2, 3])
[1, 2, 3].join("-") // none (should be "1-2-3")
```

**Impact:** Programs that build arrays incrementally can't use `.push()`.

**Workaround:** Use comprehensions or concatenation patterns.

### String methods not implemented

`.split()`, `.slice()`, `.starts-with()`, `.ends-with()`, `.size` are documented but not implemented.

**Note:** These methods work in the TypeScript/bun Rex implementation (`packages/rex-lang`). The Rust interpreter is newer and hasn't caught up on built-in methods yet.

## Planned Features

### `match` expression (design incomplete)

A multi-arm dispatch construct for O(log n) value matching, replacing linear `when`/`else when` chains. Design exploration reached the following conclusions before being paused:

**Syntax decided:**
```rex
match method do
  "GET" => list-users()
  "POST" do
    input = json.parse(body)
    {ok: true}
  end
  else not-found()
end
```

- `match expr do ... end` — consistent with `when`/`for`/`while` shape
- `key => expr` for single-expression arms, `key do block end` for multi-expression
- String keys compile to indexed object (O(log n)), consecutive integer keys to indexed array (O(1))
- `match` is an expression — nested matches form a userspace prefix trie for path routing

**Bytecode — stuck on lazy evaluation:**

The core insight is that `match` is a lazy indexed object lookup — only the matched arm should execute. Two prerequisites are needed:

1. **Lazy indexed objects** — indexed objects (which already have pointer tables for random access) should not evaluate values eagerly. Values should be evaluated on access via `.()` navigation, not on construction. This is a broader interpreter change that affects all indexed objects.

2. **`in` operator** — `key in container` checks if a key exists without evaluating the value. Returns the container if the key exists, `none` if not. Needed to distinguish "key not found" (use fallback) from "matched arm returned `none`" (return `none`).

With both, `match` desugars to existing constructs:
```rex
when method in {"GET": list(), "POST": create()} do
  /* result of `in` is the table */ .(method)
else
  fallback
end
```

**Open problem:** the desugaring requires referencing the indexed object twice (once for `in`, once for `.(key)`), which needs either a temporary variable (undesirable as compiler-generated hidden state) or a new bytecode opcode (`@`) that combines the check + lazy eval + fallback in one operation.

**Path routing:** not built into core `match`. Instead, nested matches + host-provided `path.split()` form a userspace prefix trie. Each nesting level matches one path segment. Static segments are match keys, captures fall through to `else`.

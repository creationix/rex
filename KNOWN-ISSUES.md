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

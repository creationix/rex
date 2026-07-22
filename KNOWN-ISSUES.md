# Known Issues

## Release Blocker: rex-luajit repo artifacts are not portable

`crates/rex-luajit/librex_luajit.dylib` currently points to an absolute local
path (`/Users/tim/...`), which is machine-specific and non-portable.

**Impact:** fresh clones or CI environments can fail to load LuaJIT native
bindings unless local symlinks are manually repaired.

**Fix:** remove machine-local symlinks from source control and make test/runtime
loading deterministic via one of:

- Build-step generated local symlink in `target` only.
- Runtime loader that resolves platform-specific build outputs.
- A script/Make target that creates correct links for Linux/macOS.

## Release Readiness Gap: no single cross-runtime test entrypoint

The repo has multiple valid test surfaces (Rust workspace, VS Code extension,
rex-ts, LuaJIT), but no single command currently guarantees full release
verification across all runtimes.

**Impact:** release confidence depends on manual orchestration and can miss
migration regressions between crates/packages.

**Fix:** add a root test command (for example via `just`, `make`, or npm/bun
script) that runs at least:

- `cargo test`
- `bun test` in `packages/vscode-rex`
- `bun test` in `packages/rex-ts`
- LuaJIT tests in `crates/rex-luajit`

and use it as the required pre-release gate.

---

## Path to 1.0

The current Rust implementation is a prototype — valuable for proving the
design, but not the final artifact. The real product is the specification:
documents precise enough that a competent engineer (or coding agent) can build a
conforming Rex implementation from scratch without guesswork. Implementations
can be replaced; the spec cannot.

1.0 means the spec is complete, the bytecode format is frozen, and at least one
conforming implementation exists with real-world distribution (npm/WASM).

### Phase 0: Spec completeness (prerequisite for everything else)

The specification must be sufficient for a clean-room reimplementation. The
current docs have meaningful gaps — places where behavior is defined only by the
Rust source code, not by any document. Every gap below is something a new
implementor would have to reverse-engineer or guess at.

**Gaps in `docs/rx-format.md` (data layer):**

- **Deduplication is underspecified for edge cases.** The encoding strategy and
  cost-check heuristic are documented, but the interaction between deduplication
  and schema-shared objects is not. ~~The `dedup_comprehension_with_shared_keys`
  panic is a symptom~~ (fixed — the interpreter now scans schema target bytecode
  to extract keys without evaluating values). The spec still needs to state
  whether pointers may target individual values within a schema-shared object,
  or only the schema object itself.

- **Chain (`.`) segment type rules.** The spec says "all segments in a chain
  must be the same type" but doesn't define what happens when they aren't (error?
  undefined? first-segment-wins?). Specify the behavior or make it an encoder
  constraint that decoders needn't check.

- **Pointer chain resolution limits.** Pointers can chain (a pointer to a
  pointer). The spec says decoders "must handle them" but doesn't cap the depth.
  Add a recommended limit or state that unbounded chaining is valid.

**Gaps in `docs/rexc-bytecode.md` (language layer):**

- **Built-in methods are not documented.** The interpreter implements 13
  built-in method opcodes that are completely absent from the bytecode spec:

  | Mnemonic | Method | Applies to |
  |----------|--------|------------|
  | `pu` | `push(value)` | arrays |
  | `po` | `pop()` | arrays |
  | `jn` | `join(separator)` | arrays |
  | `ix` | `indexOf(value)` | arrays, strings |
  | `cn` | `contains(value)` | arrays, strings |
  | `sl` | `slice(start, end)` | arrays, strings |
  | `sp` | `split(separator)` | strings |
  | `tm` | `trim()` | strings |
  | `sw` | `starts-with(prefix)` | strings |
  | `ew` | `ends-with(suffix)` | strings |
  | `uc` | `upper()` | strings |
  | `lc` | `lower()` | strings |
  | `rp` | `replace(from, to)` | strings |

  These need full specification: argument types, return types, and existence
  semantics (e.g., `indexOf` returns `none` on miss, `contains` returns the
  matched value or `none`).

- **`.size` property is implicit, not compiled.** Array `.size` and string
  `.size` are handled at runtime by the interpreter, not compiled to an opcode.
  The spec doesn't mention `.size` at all. Decide: is this a language feature
  (spec it) or a host concern (remove it from the interpreter)?

- **Method-to-opcode rewriting is undocumented.** When the interpreter
  encounters `array.push(x)`, it resolves `.push` to the opcode string `%pu`
  at runtime via a lookup table. This mechanism — navigation returning an opcode
  string that becomes a callee — is not described in the bytecode spec. A new
  implementation needs to know whether methods are resolved at compile time, at
  runtime, or both.

- **Comparison semantics across types.** Comparisons "return the left-hand value
  on success, `none` on failure" — but what does `3 > "hello"` return? The
  implementation uses Rust's `partial_cmp` which returns `None` for
  cross-type comparison, yielding `none`. Spec this explicitly: are cross-type
  comparisons always `none`, or are they errors?

- **Equality semantics.** Deep structural equality is implemented for arrays and
  objects, but the spec says only "equal" without defining whether `[1] == [1]`
  is structural or referential. Spec the algorithm.

- **Arithmetic overflow.** Integer arithmetic that exceeds `i64` range silently
  promotes to float. The type system doc says overflow is "not checked" — but the
  runtime behavior (promote to float) should still be specified so implementations
  agree on what `9999999999999999999 + 1` returns.

- **Division and modulo edge cases.** Division by zero returns `nan` — this is
  in the implementation but not in the spec. Modulo by zero also returns `nan`.
  Integer division that produces a non-integer promotes to float (`7 / 2` →
  `3.5`). All of these need to be stated.

- **`add` (`ad`) polymorphism.** The `ad` opcode does arithmetic on numbers,
  concatenation on strings, and concatenation on arrays. The implementation
  returns `none` for incompatible types (number + string). Spec the full type
  dispatch table.

- **Spread in bytecode.** Spread (`...expr`) compiles to chains (`.`), which is
  documented. But the interaction with object spread and key-override semantics
  (`{ ...base, key: none }` removes `key`) is only in `spec-by-example.md`, not
  in the bytecode spec. The `key: none` removal rule needs to be in
  `rexc-bytecode.md` since it affects how object construction works at the
  bytecode level.

- **Gas metering.** The spec says gas is "charged per loop/comprehension
  iteration" and "host sets limit; 0 = unlimited." Missing: is gas charged
  once per iteration or once per expression within the iteration body? What is
  the `RexError` behavior — does it unwind or halt? Can a host catch a gas
  error and resume? The current implementation charges once per iteration
  entry and returns an unrecoverable error.

**Gaps in `docs/language.md` (surface syntax):**

- **Built-in methods not listed.** The language reference mentions `isString`,
  `isNumber`, etc. but doesn't list `.push()`, `.pop()`, `.join()`, `.split()`,
  `.trim()`, `.indexOf()`, `.contains()`, `.starts-with()`, `.ends-with()`,
  `.upper()`, `.lower()`, `.replace()`, `.slice()`, or `.size`. These are
  language features, not host extensions — they should be in the reference.

- **String escape sequences.** The lexer handles `\n`, `\t`, `\r`, `\\`, `\"`,
  `\'`, `\/`, `\b`, `\f`, `\0`, `\uXXXX`, and `\u{XXXX}`. The language
  reference doesn't list them.

- **Identifier rules.** Bare identifiers can contain hyphens (`first-name`,
  `starts-with`). The exact lexical grammar (start character, continuation
  characters, reserved word exclusion) is not specified.

- **Comma optionality rules.** "Commas optional" is stated for arrays and
  objects. But what disambiguates `[a -b]` — is it `[a, -b]` (two elements) or
  `[a - b]` (subtraction)? The parser makes a decision here; the spec should
  document it. (The semicolon section hints at this: "`;` forces expression
  boundaries" — but the array/object comma rules need their own treatment.)

**Gaps in `docs/rex-types.md` (type system):**

- **Built-in method types.** None of the 13 built-in methods have type
  signatures in the type system doc. These are needed for the typechecker to
  produce correct diagnostics.

- **`for..in` element narrowing** (already listed above as a known issue).

**Gaps in `docs/spec-by-example.md` (golden tests):**

- **No error case tests.** Every test exercises the happy path. A conforming
  implementation also needs to agree on what happens for: division by zero,
  navigation on `none`, navigation on scalars, type mismatches in arithmetic,
  gas limit exceeded, out-of-bounds array access, writing to read-only bindings.
  These should be `rex` + `json` pairs showing the expected result (usually
  `none` or a specific error).

- **No deduplication round-trip tests in the spec.** Dedup is tested in a
  separate Rust test file (`tests/dedup.rs`), not in the golden spec. Since dedup
  is an encoder optimization, the spec should include `rex` → `rext` pairs that
  exercise pointer and schema-sharing output.

- **No `.size` or built-in method bytecode tests.** The spec has runtime
  result tests for `push`, `pop`, `join`, etc. but no `rext` column showing the
  expected bytecode. An implementor building a compiler doesn't know what to emit.

- **The `TODO: is this right` marker on line 1024** indicates the spec itself
  has an unresolved question about `or` type inference.

### Phase 1: Design decisions

Some gaps above aren't missing documentation — they're missing design decisions.
These should be resolved before documenting:

- **Are built-in methods a language primitive or a convention?** Currently
  they're hardcoded in the interpreter with special dispatch. If they're
  language primitives, they need opcodes and spec entries. If they're a
  convention, they should be expressible as `.rexd` extern declarations and not
  need special runtime support. This is the single biggest spec ambiguity.

- **Is `.size` a property or a method?** It behaves like a property
  (`array.size` with no parens) but is intercepted by the runtime. This is the
  only property-like built-in. Decide: keep it and spec it, or replace with
  `size(x)` as an opcode like the type predicates.

- **Shortcode assignment: automatic or manual?** The spec shows both
  `compile_with_domain` (auto-generates 2-letter codes) and explicit shortcodes
  (`extern "Jp" json.parse`). The relationship between these two mechanisms
  isn't clear. Can they conflict? Which takes precedence?

### Phase 2: Implementation correctness

Once the spec is complete, the existing implementation should pass it cleanly.

- **Fix typechecker: `for..in` element narrowing.**

### Phase 3: Distribution (required for v0.1 tag)

These make Rex usable beyond Rust embedding.

- **`rex-node` cross-platform distribution.** Pre-built NAPI binaries for at
  minimum Linux x64, macOS arm64, macOS x64, and Windows x64. Publish to npm
  with TypeScript definitions.
- **`rex-wasm` build pipeline.** Verify the crate builds, document the build
  step, and wire it into CI. WASM is the universal fallback for environments
  without native modules.
- **Unified test entrypoint.** A single root command that runs all test
  surfaces and serves as the pre-release gate.

### Phase 4: 1.0 stability signal

- **Bytecode format freeze.** REXC stored in databases must be
  forward-compatible. Document the stability guarantee.
- **LuaJIT portability fix.** Remove machine-local symlinks from source
  control.

### Intentional non-goals

The following are out of scope and will not block 1.0:

- User-defined functions, lambdas, or closures. Rex is intentionally scoped to
  scripting against host-provided bindings. Keeping all callable logic `extern`
  preserves the boundary between Rex as a guest language and the host runtime.
- Module imports or code reuse across `.rex` files. Programs are self-contained
  by design.
- Standard library. All capabilities are host-provided via `.rexd` domain files.

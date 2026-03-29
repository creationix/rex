# rusty-rex

Rust implementation of the Rex language compiler and RX bytecode encoder. This is a workspace containing multiple crates that share a common core library.

## Crates

```
crates/
├── rex-core/       Core library: lexer, parser, CST, lowering, bytecode, decompiler
├── rex-cli/        CLI tool: `rex compile`, `rex encode`, `rex decode`, `rex decompile`, `rex inspect`
├── rex-node/       Node.js bindings via napi-rs (npm package: rex-rs)
└── rex-luajit/     LuaJIT bindings via C FFI and Lua C API (experimental)
```

## Building

Requires Rust 2024 edition (1.85+).

```sh
cargo build --release
cargo test
```

The CLI binary is at `target/release/rex`. The Node.js native module is built separately:

```sh
cd crates/rex-node
bun install
bun run build        # builds release .node binary
```

## Architecture

### rex-core

The core library has no external dependencies beyond `logos` (lexer generator) and `rowan` (lossless CST).

**Modules:**

- **`lexer`** — logos-based tokenizer. 28 keywords with word-boundary guards, operators, string/number literals, comments. Produces trivia tokens (whitespace, comments) for lossless round-tripping.

- **`syntax`** — `SyntaxKind` enum bridging logos tokens to rowan's type system. Defines both leaf token kinds and composite CST node kinds.

- **`parser`** — Hand-written recursive descent parser with Pratt precedence climbing for expressions. 8 operator precedence levels collapsed into a single function. Builds a lossless rowan `GreenNode` CST. Supports error recovery. Uses `rowan::NodeCache` for deduplication across parses (useful for LSP incremental re-parsing).

- **`ast`** — Thin typed wrappers over `rowan::SyntaxNode` for ergonomic CST access (`BinaryExpr::lhs()`, `ConditionalExpr::condition()`, etc).

- **`lower`** — CST → bytecode `Value` tree. Walks the untyped syntax tree and produces the IR used by the encoder.

- **`bytecode`** — The `Value` enum (28 variants covering all Rex/RX constructs), the encoder, the dedup encoder, and the decoder.

  - **`encode(value)`** — Simple forward encoder, no dedup.
  - **`encode_dedup(value)`** — Reverse encoder (`RevEncoder`) that writes children before parents so sized containers know their body length without fixups. Includes string dedup, string chaining (delimiter-based prefix sharing), schema sharing (objects with identical key sets share a schema pointer), and container dedup (small subtrees with ≤32 nodes).
  - **`decode(input)` / `decode_raw(input)`** — Left-to-right decoder. `decode` resolves pointers, chains, and schema-shared objects. `decode_raw` preserves them for inspection.

- **`decompile`** — `Value` tree → Rex source code pretty-printer. Handles operator precedence (inserting parens only when needed), compound assignment detection (`x = add(x, 1)` → `x += 1`), and multi-line formatting for blocks/objects. `decompile_raw` preserves pointers and chains for debugging.

- **`json_fast`** — Fast path: parses a token stream directly to `Value` without building a CST. Used when the input is pure JSON/data. Skips rowan entirely, avoiding Arc allocations.

### rex-cli

CLI binary (`rex`). Commands:

```
rex compile <file>              Rex source → REXC bytecode
rex decompile <file> [--raw]    REXC/RX bytecode → Rex source
rex encode <file>               JSON → RX bytecode
rex decode <file> [--pretty]    RX bytecode → JSON
rex inspect <file>              Colored bytecode structure tree
```

All commands support `--time` for timing info, stdin/stdout piping, and `-o` for output files.

### rex-node

Node.js native module published as `rex-rs` on npm. Two functions:

```typescript
encode(value: unknown): string   // JS value → RX bytecode (with dedup)
compile(source: string): string  // Rex source → REXC bytecode
```

`encode` walks JS objects directly via the napi C API, builds a `Value` tree, and runs `encode_dedup`. `compile` goes through the full Rex pipeline.

### rex-luajit

LuaJIT bindings. Two interfaces:

1. **Lua C API** (`luaopen_rex_native`) — `rex.encode(lua_value)` walks Lua tables directly via the Lua C API. `rex.compile(source)` compiles Rex source.

2. **FFI** (`rex_ffi.lua`) — Experimental. Small C functions (`rex_enc_open_object`, `rex_enc_string`, etc.) that LuaJIT calls via FFI. The table walk happens in JIT-compiled Lua. Faster than the C API path (~2-3x) but doesn't yet have full schema sharing.

## Bytecode Format

Left-to-right, UTF-8 safe. Core rule: `[b64 varint][tag][body]`.

**Tags:**
- `+` integer (zigzag), `*` decimal exponent prefix, `,` string (length-prefixed)
- `'` reference (`t'`=true, `n'`=null), `$` variable, `%` opcode, `@` self
- `;` list (lazy, sized), `:` map (lazy, sized)
- `(` `)` call, `[` `]` array, `{` `}` block
- `?(`...`)` when, `!(`...`)` unless, `|(`...`)` or, `&(`...`)` and
- `>(`...`)` for-in, `<(`...`)` for-of, `#(`...`)` while
- `=` set, `:=` swap, `~` delete, `\` break/continue
- `^` pointer (delta offset for dedup), `.` chain (string prefix sharing)

**Dedup optimizations:**
- Repeated strings → `^delta` pointer to first occurrence
- String prefix chaining → `.` tag with shared prefix segments
- Objects with same keys → schema pointer + values only
- Small identical subtrees → structural hash dedup

## Performance

On a 94.5 MB JSON file (Apple M-series):

| Operation | Output | Time |
|-----------|--------|------|
| `rex encode` (JSON → RX) | 5.3 MB (94% compression) | 265ms |
| `rex compile` (Rex → REXC) | 5.3 MB | 654ms |
| `rex decode` (RX → JSON) | 94.5 MB | 1036ms |
| Node.js `encode(value)` | 5.3 MB | 310ms |
| Node.js `compile(source)` | 5.3 MB | 280ms |
| LuaJIT `encode(table)` | 709 KB (10K users) | 15ms |

Parser throughput: 2.2 GB/s lexer, 543 MB/s full lex+parse on raw JSON.

## Tests

255 tests across unit tests, integration tests, and round-trip tests.

```sh
cargo test                          # all tests
cargo test -p rex-core --lib        # unit tests only
cargo test -p rex-core --test samples    # parser sample tests
cargo test -p rex-core --test roundtrip  # Value → RX → Rex → compile → compare
```

## Status / Known Issues

- **Pointer delta direction**: Pointers point forward (to higher byte positions) in the output. `delta = self.pos - target_left` where `self.pos` is the running byte count.

- **Schema sharing in decoder**: The decoder detects schema-shared objects by checking if the first value in a map body is a non-string. If it resolves to a map, its keys are used as the schema.

- **String chain direction**: The encoder writes chains as `[size].[prefix][suffix]`. The prefix may itself be a chain or pointer. The decoder concatenates segments in order.

- **No optimizer passes yet**: The `lower` module produces unoptimized IR. Constant folding, dead code elimination, and other passes can be added as IR → IR transforms on the `Value` tree.

- **No interpreter yet**: The CLI can compile and decompile but cannot execute Rex programs.

- **LuaJIT FFI path**: The FFI encoder is faster but doesn't have container dedup or full schema sharing. The C API path has full features but is slower.

- **Decimal round-trip**: Decimals decompile as scientific notation (`314e-2`) to preserve exact significand and exponent through round-trips without floating-point conversion.

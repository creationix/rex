# rex-rs

High-performance encoder for the [RX/REXC](https://github.com/creationix/rex) binary format. Compiles Rex source code and encodes JavaScript values into compact, random-access bytecode strings that embed directly in JSON.

Written in Rust with native bindings for Node.js via [napi-rs](https://napi.rs).

## Install

```sh
npm install rex-rs
```

## API

### `encode(value: unknown): string`

Encode a JavaScript value to RX bytecode. Supports all JSON types: objects, arrays, strings, numbers, booleans, and null.

The output is a UTF-8 string safe for embedding in JSON values. Repeated strings, object schemas, and small subtrees are automatically deduplicated using pointer references.

```js
import { encode } from 'rex-rs';

encode(42)                  // '1k+'
encode('hello')             // '5,hello'
encode(true)                // "t'"
encode([1, 2, 3])           // '6;2+4+6+'
encode({ name: 'Ada' })    // "9:4,name3,Ada"
```

### `compile(source: string): string`

Compile Rex source code to REXC bytecode. Supports the full Rex language including variables, operators, control flow, comprehensions, and mutations.

```js
import { compile } from 'rex-rs';

compile('1 + 2')                    // '(ad%2+4+)'
compile('x = 42')                   // '=x$1k+'
compile('when x do y end')          // '?(x$y$)'
compile('max = max or 100')         // '=max$|(max$38+)'
compile('[self * self in items]')   // '>[items$(ml%@@)]'
```

## Performance

Benchmarks on a 94.5 MB JSON file (Apple M-series):

| Function | Output | Compression | Time |
|----------|--------|-------------|------|
| `encode(value)` | 5.3 MB | 94% | ~300ms |
| `compile(source)` | 5.3 MB | 94% | ~640ms |

`encode` walks JS objects directly via the napi C API. `compile` goes through the full Rex pipeline: lexer, parser, syntax tree, IR lowering, and bytecode encoding.

## Bytecode format

RX is a compact, left-to-right binary format encoded as printable UTF-8. Every value starts with an optional base-64 varint, followed by a tag byte:

- `+` integer (zigzag encoded)
- `*` decimal exponent prefix
- `,` string (length-prefixed)
- `'` named reference (`t'`=true, `f'`=false, `n'`=null)
- `;` list, `:` map (sized, lazy)
- `(` `)` calls, `[` `]` arrays, `{` `}` blocks
- `^` pointer (delta offset to deduplicated value)

REXC extends RX with variables (`$`), opcodes (`%`), control flow (`?` when, `!` unless, `>` for-in, `#` while), and mutation (`=` set, `~` delete).

## License

MIT

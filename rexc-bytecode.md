# REXC Bytecode Format

REXC is a compact bytecode that serializes as printable UTF-8. It is a **superset of [RX](rx-format.md)**: every valid RX document is valid REXC. The RX layer handles data encoding (numbers, strings, arrays, objects, pointers, chains, indexes). REXC adds language constructs: variables, opcodes, control flow, mutation, and evaluation semantics.

REXC embeds directly in JSON string values with minimal escaping.

---

## Parsing Rule

Every value starts with zero or more base-64 digits (the varint), followed by a non-b64 tag byte. The tag determines how to interpret the varint and what (if any) body follows.

```
[b64 digits][tag][body]
```

The parser:
1. Scans b64 digits greedily -> varint bytes
2. Reads the next byte -> tag
3. Interprets the varint based on the tag
4. Reads body if the tag requires one

### Base-64 Digit Alphabet

```
0-9   values 0-9
a-z   values 10-35
A-Z   values 36-61
-     value 62
_     value 63
```

Digits form big-endian base-64 unsigned integers. Zero is an empty string (no digits).

### Zigzag Encoding

Signed integers use zigzag: `n >= 0 ? 2n : -2n - 1`

```
 0 -> 0    -1 -> 1     1 -> 2    -2 -> 3     2 -> 4
```

---

## RX Data Layer

See [rx-format.md](rx-format.md) for the full RX spec. Summary of RX tags:

| Tag     | Kind   | Description                              |
|---------|--------|------------------------------------------|
| `+`     | scalar | Integer (zigzag)                         |
| `*`     | prefix | Decimal exponent (followed by `[sig]+`)  |
| `,`     | sized  | String (varint = byte count)             |
| `'`     | scalar | Named reference (true, false, null, etc.)|
| `^`     | scalar | Pointer (delta offset)                   |
| `.`     | sized  | String chain (varint = byte count)       |
| `[` `]` | paired | Array                                    |
| `{` `}` | paired | Object                                   |
| `#`     | index  | Index header (inside container)          |

---

## REXC Additions

Everything in RX is valid REXC. REXC adds the following tags for language constructs.

### Scalars

| Tag | Name           | Varint meaning                              |
|-----|----------------|---------------------------------------------|
| `$` | Variable       | name (opaque b64 bytes)                     |
| `%` | Opcode         | mnemonic (opaque b64 bytes)                 |
| `@` | Self           | depth (0 = current)                         |
| `\` | Break/Continue | `(depth-1)*2 + kind` (0=break, 1=continue)  |

### Calls

`(callee arg0 arg1 ...)` -- paired delimiters, first child is callee, rest are arguments.

| Callee type | Meaning                                       |
|-------------|-----------------------------------------------|
| `%opcode`   | Operation call -- apply opcode to args        |
| `$variable` | Navigation -- look up args as keys on variable |
| `'ref`      | Domain navigation                              |
| other       | Navigation on expression result                |

```
(ad%2+4+)                -> add(1, 2) = 3
(user$4,name)            -> user.name
(user$7,address6,street) -> user.address.street
```

### Control Flow

Compound tags: modifier before the opening delimiter.

| Tags      | Name   | Children            |
|-----------|--------|---------------------|
| `?(` `)`  | When   | cond, then [, else] |
| `!(` `)`  | Unless | cond, then [, else] |
| `\|(` `)` | Or     | left, right         |
| `&(` `)`  | And    | left, right         |

**When**: evaluate cond. If defined -> evaluate then, skip else. If none -> skip then, evaluate else (or return none).

**Or**: evaluate left. If defined -> return left, skip right. If none -> evaluate right.

**And**: evaluate left. If none -> return none, skip right. If defined -> evaluate right.

### Length Prefixes (Skip Support)

In pure RX data, containers never need length prefixes. In REXC, conditional branches may be skipped at runtime. When a branch child is a container, the encoder adds a varint length prefix before the container opener so the interpreter can jump past it in O(1):

```
?(cond 4{2+4+} 2{6+})     <- branch blocks are length-prefixed
>(iterable bindings {body}) <- not conditional, no prefix
?(x$ 1k+)                  <- scalar branch, no prefix needed
```

The length prefix gives the byte count between the delimiters (excluding opener and closer). Since `[`, `{`, `(` are not b64 digits, the parser always distinguishes a length prefix from an unprefixed container.

**Return is transparent** -- when a return (`;`) appears in a skip position, it passes the skip flag through to its child. The `;` itself never gets a length prefix. Example: `return [1]` in a branch encodes as `;2[2+]` (child array gets the prefix).

### Loops

| Tags     | Name        | Children                    |
|----------|-------------|-----------------------------|
| `>(` `)` | For-in loop | iterable, [$bindings], body |
| `<(` `)` | For-of loop | iterable, [$bindings], body |
| `#(` `)` | While loop  | cond, body                  |

Bindings: 0-2 `$` variables between iterable and body.

| Tag          | 0 bindings | 1 binding  | 2 bindings    |
|--------------|------------|------------|---------------|
| `>` (for-in) | `for in`   | `for v in` | `for k, v in` |
| `<` (for-of) | `for of`   | `for k of` | --             |

### Comprehensions

| Tags     | Name                        | Children                                    |
|----------|-----------------------------|---------------------------------------------|
| `>[` `]` | For-in array comprehension  | iterable, [$bindings], value_expr           |
| `<[` `]` | For-of array comprehension  | iterable, [$bindings], value_expr           |
| `#[` `]` | While array comprehension   | cond, value_expr                            |
| `>{` `}` | For-in object comprehension | iterable, [$bindings], key_expr, value_expr |
| `<{` `}` | For-of object comprehension | iterable, [$bindings], key_expr, value_expr |
| `#{` `}` | While object comprehension  | cond, key_expr, value_expr                  |

### Mutation

Fixed-arity operators. No delimiters -- children are self-delimiting.

| Tag | Name     | Children                         |
|-----|----------|----------------------------------|
| `=` | Set      | place, value                     |
| `/` | Swap-set | place, value (returns old value) |
| `~` | Delete   | place                            |

```
=x$1k+          -> x = 42
=x$(ad%x$2+)    -> x += 1  (desugared: x = add(x, 1))
~x$              -> delete x
```

### Block

`{expr0 expr1 ... exprN}` -- evaluates all expressions sequentially, returns the last result. Uses `{}` delimiters (same as objects; the interpreter distinguishes by context).

### Return

`;[value]` -- the `;` tag consumes the next value as the return value. Halts execution and propagates through all enclosing blocks, loops, and conditionals.

```
;1k+             -> return 42
;no'             -> return none (bare return)
```

A bare `return` with no value compiles to `;no'` -- the compiler injects `none` as the child.

### Tagged Template Literals

Template literals compile to string chains (`.`) for untagged, or to calls for tagged.

**Untagged** -- `` `hello ${name}` `` compiles to a string chain:

```
.[5,hello name$]     -> chain("hello ", name) -> "hello Ada"
```

**Tagged** -- `` html`<a>${title}</a>` `` compiles to a call:

```
(html%[4,<a >5,</a>]title$)     -> html(["<a>", "</a>"], title)
```

---

## Semantics

### Existence, Not Truthiness

Rex uses **existence** instead of truthiness. Only `none` represents absence. `false`, `null`, `0`, and `""` are all defined values.

- Comparisons return the left-hand value on success, `none` on failure
- `when`/`unless` branch on whether a value is defined (not none)
- `and`/`or` short-circuit on existence
- Type predicates return the value if it matches, `none` otherwise

### Gas-Bounded Execution

Gas is charged per loop/comprehension iteration. The host sets the limit; 0 = unlimited.

---

## Tag Summary

### RX (data layer)

| Tag     | Kind   | Description                                                      |
|---------|--------|------------------------------------------------------------------|
| `+`     | scalar | Integer (zigzag)                                                 |
| `*`     | prefix | Decimal exponent (followed by `[sig]+`)                          |
| `,`     | sized  | String (varint = byte count)                                     |
| `'`     | scalar | Named reference                                                  |
| `^`     | scalar | Pointer (delta offset)                                           |
| `.`     | sized  | String chain (varint = byte count)                               |
| `[` `]` | paired | Array                                                            |
| `{` `}` | paired | Object                                                           |
| `#`     | index  | Index header: varint = (count << 3 \| width-1), then pointers   |

### REXC (language layer)

| Tag     | Kind     | Description                                    |
|---------|----------|------------------------------------------------|
| `$`     | scalar   | Variable                                       |
| `%`     | scalar   | Opcode                                         |
| `@`     | scalar   | Self (depth)                                   |
| `\`     | scalar   | Break/continue                                 |
| `(` `)` | paired   | Call (optional length prefix in skip position)  |
| `?`     | modifier | When (before `(`)                              |
| `!`     | modifier | Unless (before `(`)                            |
| `\|`    | modifier | Or (before `(`)                                |
| `&`     | modifier | And (before `(`)                               |
| `>`     | modifier | For-in (before `(`, `[`, or `{`)               |
| `<`     | modifier | For-of (before `(`, `[`, or `{`)               |
| `#`     | modifier | While (before `(`, `[`, or `{`)                |
| `=`     | fixed    | Set (2 children)                               |
| `/`     | fixed    | Swap-set (2 children)                          |
| `~`     | fixed    | Delete (1 child)                               |
| `;`     | prefix   | Return (consumes next value)                   |

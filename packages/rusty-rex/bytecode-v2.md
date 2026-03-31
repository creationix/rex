# RX / REXC Bytecode Format (v2)

A compact, left-to-right binary format encoded as printable UTF-8. Embeds directly in JSON string values with minimal escaping.

**RX** is the data layer — encodes JSON-compatible values (numbers, strings, arrays, objects, booleans, null).

**REXC** is a superset of RX — adds variables, opcodes, control flow, mutation, and evaluation semantics for the Rex language.

---

## Parsing Rule

Every value starts with zero or more base-64 digits (the varint), followed by a non-b64 tag byte. The tag determines how to interpret the varint and what (if any) body follows.

```
[b64 digits][tag][body]
```

The parser:
1. Scans b64 digits greedily → raw varint bytes
2. Reads the next byte → tag
3. Interprets the raw bytes based on the tag (as a number, or as an opaque name)
4. Reads body if the tag has one

### Base-64 Digit Alphabet

```
0-9   values 0–9
a-z   values 10–35
A-Z   values 36–61
-     value 62
_     value 63
```

Digits form big-endian base-64 unsigned integers. Zero is an empty string (no digits).

### Zigzag Encoding

Signed integers use zigzag: `n >= 0 ? 2n : -2n - 1`

```
 0 → 0    -1 → 1     1 → 2    -2 → 3     2 → 4
```

---

## RX Data Layer

### Scalars

Scalars have no body — just varint + tag.

| Tag | Name    | Varint interpretation                                           | Example                                              |
|-----|---------|-----------------------------------------------------------------|------------------------------------------------------|
| `+` | Integer | zigzag signed integer                                           | `+` = 0, `2+` = 1, `1+` = -1, `1k+` = 42             |
| `*` | Decimal | zigzag exponent, then reads next `[sig]+`                       | `3*1k+` = 42 × 10^(-2) = 0.42                        |
| `,` | String  | byte count of raw UTF-8 body                                    | `5,hello` = "hello", `,` = ""                        |
| `'` | Ref     | name (opaque b64 bytes)                                         | `t'` = true, `f'` = false, `n'` = null, `no'` = none |
| `^` | Pointer | delta offset (forward, from right of pointer to left of target) | `^` = delta 0, `3^` = delta 3                        |

#### Decimals

`[exp]*` is a prefix that consumes the next integer as the significand:

```
[zigzag_exp]*[zigzag_sig]+    →    sig × 10^exp
```

#### Built-in References

| Encoding | Value          |
|----------|----------------|
| `t'`     | true           |
| `f'`     | false          |
| `n'`     | null           |
| `no'`    | none (absence) |
| `nan'`   | NaN            |
| `inf'`   | +Infinity      |
| `nif'`   | -Infinity      |

#### Pointers

`[delta]^` — a relative offset from the byte after `^` to the start of the target value. Used for deduplication: repeated values become pointers to a duplicate deeper in the document.

### Containers

Containers use paired delimiters. The body is zero or more values between the delimiters.

| Open | Close | Name   |
|------|-------|--------|
| `[`  | `]`   | Array  |
| `{`  | `}`   | Object |

#### Optional length prefix

Any paired container can optionally be preceded by a varint giving the byte count of the body (everything between the delimiters, excluding the delimiters themselves):

```
[body]           → no prefix, not skippable — consumer must parse all children
[size][body]     → length-prefixed, skippable — consumer can jump size bytes past opener
```

The producer decides which containers to length-prefix. Typical uses:
- **Control flow branches** — the interpreter skips the untaken branch by jumping `size` bytes past the opener, then consuming the closer
- **Large data containers** — skip past a container without parsing its contents

The length prefix is a varint of b64 digits immediately before the opening delimiter. Since `[` and `{` are not b64 digits, the parser can always tell whether a varint precedes the opener.

**Arrays:**

Arrays are semantically an ordered list of values of any type.

| Encoding    | Value       | Skippable?    |
|-------------|-------------|---------------|
| `[]`        | `[]`        | no            |
| `[2+4+6+]`  | `[1, 2, 3]` | no            |
| `6[2+4+6+]` | `[1, 2, 3]` | yes (6 bytes) |

**Objects:**

Objects are semantically an ordered mapping from strings to values of any type.

They are either encoded as alternating key/value pairs or the first entry resolves to a shared schema and is followed by just values in matching order.

| Encoding                                    | Value                                                    |
|---------------------------------------------|----------------------------------------------------------|
| `{}`                                        | `{}`                                                     |
| `{4,name3,Ada5,score2-+}`                   | `{name: "Ada", score: 95}`                               |
| `{9^3,Bob1k+}`<br>`{4,name3,Ada5,score2-+}` | `{name: "Bob", score: 42}`<br>`{name: "Ada", score: 95}` |

Since keys are always strings semantically, the first node in key position disambiguates: if it resolves to a string, parse key-value pairs normally. If it resolves to an array or object, it's a schema defining the keys — the remaining nodes are values only.

### Indexed Containers

A container can have an index for random access. The `#` tag appears inside the container, just before the element data. It follows the standard `[varint][tag]` parsing rule — the varint before `#` encodes both the element count and pointer width.

#### Index format

```
[ <packed>#<ptr0><ptr1>...<ptrN> <elem0><elem1>...<elemN> ]
```

- `packed` — varint before the `#` tag, encoding two values:
  - Lower 3 bits: pointer width (1–8 b64 digits per pointer)
  - Upper bits: element count (`packed >> 3`)
- `ptr0..ptrN` — fixed-width pointers, one per element. Each is a byte offset from the end of the pointer table to the start of that element. All pointers use the same width (zero-padded on the left).
- Elements follow immediately after the pointer table, encoded normally.

The consumer reads `packed`, computes `count = packed >> 3` and `width = (packed & 7) + 1`, then reads `count * width` b64 digits as the pointer table. Elements follow.

```
[4#0123 2+4+]    → packed=4: count=0 (4>>3), width=5 (4&7+1) — wrong example
[a#02 2+4+]      → packed=a (10): count=1 (10>>3), width=3 (10&7+1) — also wrong
[8#0 2+]         → packed=8: count=1, width=1. ptr0=0. one element: int 1
```

Concretely, for a 2-element array `[1, 2]` with pointer width 1:
- count=2, width=1 → packed = (2 << 3) | (1-1) = 16 → varint `g`
- ptr0=`0` (offset 0 from end of table), ptr1=`2` (offset 2)
- Result: `[g#022+4+]`

#### Skipping

Skipping an indexed container uses the **length prefix** (before the opener), not an internal end pointer. The `#` index is purely for random access.

**No index = eager.** The consumer reads all elements sequentially.

**Index present = random access.** The consumer can jump to individual elements via the pointer table.

#### Indexed objects

Schema and index are mutually exclusive on the same object. Schema is for compression of many small same-shape objects; index is for random access into large flat objects. They serve different levels of the data hierarchy.

An indexed object has one pointer per key-value pair pointing to the start of the key. Pointers are sorted by key (byte-order comparison of the encoded key) for O(log n) binary search lookup.

#### Placement

```
[ <packed>#<pointers> <elements> ]          ← indexed array
{ <packed>#<pointers> <key0><val0>... }     ← indexed object (sorted by key)
{ <schema-ptr> <val0><val1>... }            ← schema-shared object
```

### String Chains

`[size].[seg1 seg2 ...]` — a string built from concatenated segments. Each segment can be a string, a pointer, or another chain.

```
5.[3^4,/baz]     → chain: pointer resolves to "/foo/bar", suffix "/baz" → "/foo/bar/baz"
```

Used for prefix deduplication of strings with common prefixes (URL paths, header names, etc.).

---

## REXC Additions

Everything in RX is valid REXC. REXC adds the following tags for language constructs.

### Scalars

| Tag | Name           | Varint interpretation                      |
|-----|----------------|--------------------------------------------|
| `$` | Variable       | name (opaque b64 bytes)                    |
| `%` | Opcode         | mnemonic (opaque b64 bytes)                |
| `@` | Self           | depth (0 = current)                        |
| `\` | Break/Continue | `(depth-1)*2 + kind` (0=break, 1=continue) |

### Calls

`(callee arg0 arg1 ...)` — paired delimiters, first child is callee, rest are arguments.

The callee determines the call type:

| Callee type | Meaning                                       |
|-------------|-----------------------------------------------|
| `%opcode`   | Operation call — apply opcode to args         |
| `$variable` | Navigation — look up args as keys on variable |
| `'ref`      | Domain navigation                             |
| other       | Navigation on expression result               |

```
(ad%2+4+)                → add(1, 2) = 3
(user$4,name)            → user.name
(user$7,address6,street) → user.address.street
(table$key$)             → table.(key)
```

### Control Flow

Compound tags: the modifier comes first, then the opening delimiter.

| Tags      | Name   | Children            |
|-----------|--------|---------------------|
| `?(` `)`  | When   | cond, then [, else] |
| `!(` `)`  | Unless | cond, then [, else] |
| `\|(` `)` | Or     | left, right         |
| `&(` `)`  | And    | left, right         |

**When**: evaluate cond. If defined → evaluate then, skip else. If none → skip then, evaluate else (or return none).

**Or**: evaluate left. If defined → return left, skip right. If none → evaluate right.

**And**: evaluate left. If none → return none, skip right. If defined → evaluate right.

**Skipping branches**: if a branch is a container, the producer should make it indexed so the interpreter can skip it via the end pointer. If it's a scalar, scanning past it is trivial.

### Loops

| Tags     | Name        | Children                    |
|----------|-------------|-----------------------------|
| `>(` `)` | For-in loop | iterable, [$bindings], body |
| `<(` `)` | For-of loop | iterable, [$bindings], body |
| `#(` `)` | While loop  | cond, body                  |

Bindings: 0–2 `$` variables between iterable and body.

| Tag          | 0 bindings | 1 binding  | 2 bindings    |
|--------------|------------|------------|---------------|
| `>` (for-in) | `for in`   | `for v in` | `for k, v in` |
| `<` (for-of) | `for of`   | `for k of` | —             |

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

Fixed-arity operators. No delimiters — children are self-delimiting.

| Tag | Name     | Children                         |
|-----|----------|----------------------------------|
| `=` | Set      | place, value                     |
| `/` | Swap-set | place, value (returns old value) |
| `~` | Delete   | place                            |

```
=x$1k+          → x = 42
=x$(ad%x$2+)    → x += 1  (desugared: x = add(x, 1))
~x$              → delete x
```

### Block

`{expr0 expr1 ... exprN}` — evaluates all expressions sequentially, returns the last result. Uses `{}` delimiters (same as objects, but in REXC context where expressions have side effects).

The interpreter distinguishes objects from blocks by context: inside data position → object. Inside code position → block.

### Return

`[optional size];[value]` — a compound tag like decimal (`*`). The `;` tag always consumes the next value as the return value. Halts execution and propagates through all enclosing blocks, loops, and conditionals.

The varint before `;` is the byte count of `[value]` (for skipping). If the varint is empty (no b64 digits before `;`), the return is not skippable — the consumer must parse the child to advance past it. If a size is present, the consumer can skip `size` bytes to jump past the return value without parsing it.

```
;1k+             → return 42 (not skippable)
3;1k+            → return 42 (skippable — value is 3 bytes)
;no'             → return none (bare return)
```

A bare `return` with no value compiles to `;no'` — the compiler injects `none` as the child.

Rex source:
```rex
when method == "GET" do
  return {ok: true, data: items}
end
when method == "POST" do
  return {ok: true, created: id}
end
res.status = 405
{ok: false, error: "method_not_allowed"}
```

### Tagged Template Literals

Template literals compile to string chains (`.`) for untagged, or to calls for tagged. No new bytecode tag needed.

**Untagged** — `` `hello ${name}` `` compiles to a string chain:

```
.[5,hello name$]     → chain("hello ", name) → "hello Ada"
```

The chain segments are string literals and expressions interleaved. The interpreter concatenates them.

**Tagged** — `` html`<a>${title}</a>` `` compiles to a call where the first argument is an array of the static string parts:

```
(html%[4,<a >5,</a>]title$)     → html(["<a>", "</a>"], title)
```

The tag function (domain opcode) receives the constant string parts array and the interpolated values as separate arguments. This enables safe-by-construction patterns: SQL parameterization, HTML escaping, URL encoding.

---

## Semantics

### Existence, Not Truthiness

Rex uses **existence** instead of truthiness. Only `none` represents absence. `false`, `null`, `0`, and `""` are all defined values.

- Comparisons return the left-hand value on success, `none` on failure
- `when`/`unless` branch on whether a value is defined (not none)
- `and`/`or`/`nor` short-circuit on existence
- Type predicates return the value if it matches, `none` otherwise

### Eager vs Indexed

| Has index? | Random access           | Skippable               |
|------------|-------------------------|-------------------------|
| No         | No — sequential only    | Only with length prefix |
| Yes (`#`)  | Yes — via pointer table | Only with length prefix |

The producer decides which containers to index. The index enables random access; skipping is always via the length prefix (before the opener), independent of whether an index is present.

### Gas-Bounded Execution

Gas is charged per loop/comprehension iteration. The host sets the limit; 0 = unlimited.

---

## Tag Summary

### RX (data layer)

| Tag     | Kind   | Description                                                                                 |
|---------|--------|---------------------------------------------------------------------------------------------|
| `+`     | scalar | Integer (zigzag)                                                                            |
| `*`     | prefix | Decimal exponent (followed by `[sig]+`)                                                     |
| `,`     | sized  | String (varint = byte count)                                                                |
| `'`     | scalar | Named reference                                                                             |
| `^`     | scalar | Pointer (delta offset)                                                                      |
| `.`     | sized  | String chain (varint = byte count)                                                          |
| `[` `]` | paired | Array (optional varint length prefix before `[`)                                            |
| `{` `}` | paired | Object (optional varint length prefix before `{`)                                           |
| `#`     | index  | Index header: varint encodes (count << 3 \| width-1), followed by fixed-width pointer table |

### REXC (language layer)

| Tag     | Kind     | Description                                               |
|---------|----------|-----------------------------------------------------------|
| `$`     | scalar   | Variable                                                  |
| `%`     | scalar   | Opcode                                                    |
| `@`     | scalar   | Self (depth)                                              |
| `\`     | scalar   | Break/continue                                            |
| `(` `)` | paired   | Call (optional length prefix)                             |
| `?`     | modifier | When (before `(`)                                         |
| `!`     | modifier | Unless (before `(`)                                       |
| `\|`    | modifier | Or (before `(`)                                           |
| `&`     | modifier | And (before `(`)                                          |
| `>`     | modifier | For-in (before `(`, `[`, or `{`)                          |
| `<`     | modifier | For-of (before `(`, `[`, or `{`)                          |
| `#`     | modifier | While (before `(`, `[`, or `{`)                           |
| `=`     | fixed    | Set (2 children)                                          |
| `/`     | fixed    | Swap-set (2 children)                                     |
| `~`     | fixed    | Delete (1 child)                                          |
| `;`     | compound | Return (optional size varint, always consumes next value) |

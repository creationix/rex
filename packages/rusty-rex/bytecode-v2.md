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

**Arrays:**

Arrays are semantically an ordered list of values of any type.

They are encoded as the values directly.

| Encoding     | Value               |
|--------------|---------------------|
| `[]`         | `[]`                |
| `[2+4+6+]`   | `[1, 2, 3]`         |
| `[[t'][f']]` | `[[true], [false]]` |

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

When a container has a `#` immediately after the opening delimiter, it is **indexed**. Indexed containers support:

- **Random access** — jump directly to any element by index
- **Lazy evaluation** — elements are only parsed/executed when accessed
- **Skippable** — the end pointer allows jumping past the entire container

#### Index format

```
[#<end_ptr><count><ptr0><ptr1>...<ptrN><elem0><elem1>...<elemN>]
```

- `end_ptr` — fixed-width pointer to the byte just past the last element (before `]`). Used for skipping the entire container.
- `count` — varint giving the number of elements
- `ptr0..ptrN` — fixed-width pointers, one per element. Each is a relative delta from the end of the index to the start of that element.
- Pointer width — minimum number of b64 digits needed to reach the farthest element. All pointers use this same width.

**No index = eager.** The consumer reads all elements sequentially.

**Index present = lazy.** The consumer can jump to individual elements or skip the entire container.

#### Indexed objects

For objects, the index contains one pointer per key-value pair, pointing to the start of the key. Pointers are sorted by key (byte-order comparison of the encoded key) for O(log n) binary search lookup.

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

`[value][varint];` — the preceding value is the return value. The `;` tag halts execution and propagates through all enclosing blocks, loops, and conditionals.

The varint encodes the number of return values minus one (reserved for future multi-return). Currently always 0 (single return).

```
1k+;             → return 42
(ad%x$2+);       → return x + 1
no';             → return none (bare return)
```

A bare `return` with no value compiles to `no';` — the compiler injects `none` before the return tag.

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

### Eager vs Lazy

| Has index? | Evaluation                            | Random access            | Skippable             |
|------------|---------------------------------------|--------------------------|-----------------------|
| No         | Eager — all children parsed/evaluated | No — sequential only     | No                    |
| Yes (`#`)  | Lazy — children parsed on access      | Yes — via index pointers | Yes — via end pointer |

The producer decides which containers to index based on the use case. Typical pattern: root container indexed (lazy), inner data eager.

### Gas-Bounded Execution

Gas is charged per loop/comprehension iteration. The host sets the limit; 0 = unlimited.

---

## Tag Summary

### RX (data layer)

| Tag     | Kind   | Description                             |
|---------|--------|-----------------------------------------|
| `+`     | scalar | Integer (zigzag)                        |
| `*`     | prefix | Decimal exponent (followed by `[sig]+`) |
| `,`     | sized  | String (varint = byte count)            |
| `'`     | scalar | Named reference                         |
| `^`     | scalar | Pointer (delta offset)                  |
| `.`     | sized  | String chain (varint = byte count)      |
| `[` `]` | paired | Array                                   |
| `{` `}` | paired | Object                                  |
| `#`     | index  | Index header (inside container)         |

### REXC (language layer)

| Tag     | Kind     | Description                      |
|---------|----------|----------------------------------|
| `$`     | scalar   | Variable                         |
| `%`     | scalar   | Opcode                           |
| `@`     | scalar   | Self (depth)                     |
| `\`     | scalar   | Break/continue                   |
| `(` `)` | paired   | Call                             |
| `?`     | modifier | When (before `(`)                |
| `!`     | modifier | Unless (before `(`)              |
| `\|`    | modifier | Or (before `(`)                  |
| `&`     | modifier | And (before `(`)                 |
| `>`     | modifier | For-in (before `(`, `[`, or `{`) |
| `<`     | modifier | For-of (before `(`, `[`, or `{`) |
| `#`     | modifier | While (before `(`, `[`, or `{`)  |
| `=`     | fixed    | Set (2 children)                 |
| `/`     | fixed    | Swap-set (2 children)            |
| `~`     | fixed    | Delete (1 child)                 |
| `;`     | postfix  | Return (follows its value, varint = count - 1) |

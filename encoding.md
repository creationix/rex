# Rex Encoding Format

A compact encoding for Rex bytecode that serializes as a UTF-8 string. Designed for random-access interpretation — containers are length-prefixed so values can be skipped in O(1). The format embeds directly in JSON string values with minimal escaping.

## Format Basics

Every encoded value is a **prefix** of base-64 digits followed by a **type tag**:

```
<digits><tag>                     scalar (digits are the value)
<digits><open><body><close>       paired container (digits are byte-length of body)
<digits><tag><body>               unpaired container (digits are byte-length of body)
```

The type tag is the first non-digit character. It determines how to interpret the digit prefix and whether a body follows.

## Digit Alphabet

64 characters in a readable order:

```
0  1  2  3  4  5  6  7  8  9      values 0-9
a  b  c  d  e  f  g  h  i  j      values 10-19
k  l  m  n  o  p  q  r  s  t      values 20-29
u  v  w  x  y  z  A  B  C  D      values 30-39
E  F  G  H  I  J  K  L  M  N      values 40-49
O  P  Q  R  S  T  U  V  W  X      values 50-59
Y  Z  -  _                        values 60-63
```

Digits form a **big-endian base-64 integer**. Zero is **no digits** (zero-length prefix). Canonical encoding uses the minimum number of digits (no leading `0`).

## Type Reference

### Scalars

| Tag | Type                | Digits encode                                            |
|-----|---------------------|----------------------------------------------------------|
| `+` | Positive integer    | Value                                                    |
| `~` | Negative integer    | -1 - Value                                               |
| `*` | Decimal             | Zigzag-encoded power of 10 (consumes next integer value) |
| `:` | Bare string         | The string content itself                                |
| `?` | Query opcode        | Opcode ID                                                |
| `!` | Action opcode       | Opcode ID                                                |
| `@` | Reference           | Reference ID                                             |
| `$` | Variable            | The variable name itself                                 |
| `.` | Navigated variable  | The variable name (consumes next value as key)           |
| `/` | Navigated reference | Reference ID (consumes next value as key)                |
| `^` | Pointer             | Byte offset to another value                             |

### Containers

| Delimiters     | Type   | Body contains                    |
|----------------|--------|----------------------------------|
| `[` `]`        | Array  | Concatenated values              |
| `{` `}`        | Object | Alternating key, value pairs     |
| `(` `)`        | Call   | First value determines call type |
| `<` `>`        | Binary | Base64url-encoded bytes          |
| `,` (no close) | String | Raw UTF-8 bytes                  |
| `;` (no close) | Do     | Sequenced expressions            |
| `=` (no close) | Set    | Place, then value                |

Paired containers use closing delimiters for visual coherence and error checking. Unpaired containers (`,`, `;`, `=`) rely on the length prefix — no closing delimiter needed.

### Modifiers

| Tag  | Type  | Digits encode                                           |
|------|-------|---------------------------------------------------------|
| `#`  | Count | Item count (wraps next value)                           |
| `\|` | Index | Pointer width − 1 (wraps next value with pointer array) |

## Integers

Positive integers use `+`, negative use `~`. The two ranges don't overlap: `+` encodes 0 and up, `~` encodes -1 and down.

`+` encodes the value directly. `~` encodes **-1 - digits**, so `~` with no digits = -1.

```rexc
+   │ 0
1+  │ 1
9+  │ 9
a+  │ 10
G+  │ 42
1A+ │ 100

~   │ -1
1~  │ -2
9~  │ -10
a~  │ -11
G~  │ -43
1A~ │ -101
```

## Decimals

The `*` tag encodes a decimal number in two parts — think "multiply by power of 10". The digit prefix is a **zigzag-encoded power of 10**. The tag then **consumes the next value**, which must be a positive or negative integer, as the significand.

Zigzag mapping: 0 &rarr; 0, -1 &rarr; 1, 1 &rarr; 2, -2 &rarr; 3, 2 &rarr; 4, ...

The decoded value is: **significand &times; 10<sup>power</sup>**

```rexc
*1+   │ 1 × 10^0   = 1         │ power: zigzag(0) = 0
1*5+  │ 5 × 10^-1  = 0.5       │ power: zigzag(1) = -1
3*4W+ │ 314 × 10^-2 = 3.14     │ power: zigzag(3) = -2
c*1+  │ 1 × 10^6   = 1000000   │ power: zigzag(12) = 6
b*~   │ -1 × 10^-6 = -0.000001 │ power: zigzag(11) = -6
3*4V~ │ -314 × 10^-2 = -3.14   │ power: zigzag(3) = -2, significand: -1-313 = -314
```

The sign lives on the significand integer (`~` encodes -1-digits, so `4V~` = -1-313 = -314). This preserves exact decimal representation with no floating-point rounding.

## Bare Strings

The `:` tag interprets the digit characters as literal string content instead of a number. The digit alphabet (`0-9a-zA-Z-_`) covers all Rex identifiers, so bare strings handle most keys and names with zero overhead.

```rexc
:         │ ""
a:        │ "a"
hello:    │ "hello"
x-action: │ "x-action"
foo_bar:  │ "foo_bar"
42:       │ "42" (string, not integer — the tag disambiguates)
```

For strings with spaces, punctuation, or unicode, use the length-prefixed `,` container:

```rexc
,             │ ""
b,hello world │ "hello world"
```

## Opcodes

Two opcode families split by semantics. `do` and `set` have their own dedicated containers (`;` and `=`) and are not in either family.

### Query opcodes (`?`)

Tests, conditions, and lookups — things that check and return value or `undefined`. Used inside `()` calls. Domain query opcodes extend from 16+.

| ID | Opcode      | Enc. |  | ID | Opcode   | Enc. |
|----|-------------|------|--|----|----------|------|
| 0  | `when`      | `?`  |  | 8  | `gt`     | `8?` |
| 1  | `unless`    | `1?` |  | 9  | `gte`    | `9?` |
| 2  | `alt`       | `2?` |  | 10 | `lt`     | `a?` |
| 3  | `all`       | `3?` |  | 11 | `lte`    | `b?` |
| 4  | `eq`        | `4?` |  | 12 | `string` | `c?` |
| 5  | `neq`     m | `5?` |  | 13 | `number` | `d?` |
| 6  | `boolean`   | `6?` |  | 14 | `object` | `e?` |
| 7  | `array`     | `7?` |  | 15 | `bytes`  | `f?` |

### Action opcodes (`!`)

Computation and mutation — things that do work. Used inside `()` calls. Domain action opcodes extend from 12+.

| ID | Opcode | Enc. |  | ID | Opcode    | Enc. |
|----|--------|------|--|----|-----------|------|
| 0  | `add`  | `!`  |  | 6  | `and`     | `6!` |
| 1  | `sub`  | `1!` |  | 7  | `or`      | `7!` |
| 2  | `mul`  | `2!` |  | 8  | `xor`     | `8!` |
| 3  | `div`  | `3!` |  | 9  | `not`     | `9!` |
| 4  | `mod`  | `4!` |  | 10 | `delete`  | `a!` |
| 5  | `neg`  | `5!` |  | 11 | `literal` | `b!` |

The most common query (`when` = `?`) and action (`add` = `!`) each encode as a single byte.

## References

Pre-assigned constants. IDs 5+ are domain-defined.

| ID | Value       | Encoding |
|----|-------------|----------|
| 0  | `true`      | `@`      |
| 1  | `false`     | `1@`     |
| 2  | `null`      | `2@`     |
| 3  | `undefined` | `3@`     |
| 4  | `self`      | `4@`     |
| 5+ | Domain-defined | `5@`, `6@`, ... |

For single-key navigation into a domain reference, use `/` — it works like `@` but consumes the next value as a key:

```rexc
5/host:     │ ["$headers", "host"]     │ headers.host
5/x-action: │ ["$headers", "x-action"] │ headers['x-action']
6/page:     │ ["$query", "page"]       │ query.page
```

This is a shortcut for the common case. For multi-key paths or dynamic keys, use a call:

```rexc
e(5@x-forwarded-for:origin:) │ ["$headers", "x-forwarded-for", "origin"]
8(5@key$)                    │ ["$headers", ["$$key"]]
```

## Variables

The `$` tag works like `:` — digit characters are the variable name. Rex identifiers always fit within the digit alphabet, so no length-prefixed variant is needed.

A bare variable is a simple read:

```rexc
x$      │ ["$$x"]      │ read variable x
age$    │ ["$$age"]    │ read variable age
my-var$ │ ["$$my-var"] │ read variable my-var
```

For single-key navigation, use `.` — it works like `$` but consumes the next value as a key:

```rexc
user.name:    │ ["$$user", "name"]    │ user.name
config.debug: │ ["$$config", "debug"] │ config.debug
my-obj.key:   │ ["$$my-obj", "key"]   │ my-obj.key
```

This is a shortcut for the common case. For multi-key paths or dynamic keys, use a call:

```rexc
e(user$address:street:) │ ["$$user","address","street"] │ user.address.street
8(table$key$)           │ ["$$table",["$$key"]]         │ table[key]
```

## Do

The `;` container sequences multiple expressions and returns the last. Unpaired — length prefix only, no closing delimiter.

```rexc
4;1+2+                 │ (do 1 2) → 2
e;4=x$a+4=y$k+5(!x$y$) │ (do (set x 10) (set y 20) (add x y))
```

## Set

The `=` container binds a value to a place. Body is place followed by value. Returns the value. Unpaired.

```rexc
4=x$G+                 │ x = 42
k=5/x-handler:handler$ │ headers.x-handler = handler
```

A bare variable place is the common case (`x$`), but any navigable place works — `.` for variable paths, `/` for domain builtins, or a call for deeper navigation.

## Calls

The first value in a call body determines the call type:

| First value type  | Meaning                           |
|-------------------|-----------------------------------|
| Query opcode `?`  | Test/condition operation          |
| Action opcode `!` | Compute/mutate operation          |
| Variable `$`      | Navigable expression (place read) |
| Reference `@`     | Domain builtin navigation         |

```rexc
5(!1+2+)                     │ (add 1 2)                      │ action call
6(8?x$a+)                    │ (gt x 10)                      │ query call
e(user$address:street:)      │ user.address.street            │ multi-key navigation
e(5@x-forwarded-for:origin:) │ headers.x-forwarded-for.origin │ deep navigation
```

Single-key navigations don't need a call — use `.` for variables and `/` for references:

```rexc
user.name: │ user.name    │ instead of a(user$name:))
5/host:    │ headers.host │ instead of 9(5@host:))
```

## Objects

Body alternates key, value. Keys are typically bare strings, which makes the `:` tag pull double duty as a visual key-value separator:

```rexc
{}                   │ {}
h{color:red:size:G+} │ {color: "red", size: 42}
```

Breakdown of `h{color:red:size:G+}`:

```rexc
// A simple object
h{color:red:size:G+}
├╯╰─┬──╯╰┬─╯╰─┬─╯├╯╰─ object closer
│   │    │    │  ╰─── val 42
│   │    │    ╰────── key "size"
│   │    ╰─────────── val "red"
│   ╰──────────────── key "color"
╰──────────────────── object prefix(17) and opener
```

## Arrays

```rexc
[]        │ []
6[1+2+3+] │ [1, 2, 3]
```

Breakdown of `6[1+2+3+]`

```rexc
6[1+2+3+]
├╯├╯├╯├╯╰─ array closer
│ │ │ ╰─── positive integer 1
│ │ ╰───── positive integer 2
│ ╰─────── positive integer 3
╰───────── array prefix(6) and opener
```

## Binary

Body is base64url-encoded bytes. Matches the `<>` syntax in Rex source.

```rexc
<>         │ empty bytes
7<SGVsbG8> │ <48 65 6c 6c 6f>
```

Breakdown of `7<SGVsbG8>`

```rexc
7<SGVsbG8>
├╯╰──┬──╯╰─ bytes closer
│    ╰───── base64 encoded bytes <48 65 6c 6c 6f>
╰────────── bytes prefix(7) and opener
```

## Pointers

Deduplicate repeated values. The offset counts **forward** from the end of the pointer to the canonical value. The encoder places canonical values after their pointers so offsets always point forward.

```rexc
^  │ the value immediately after this pointer (offset 0)
1^ │ the value 1 byte after this pointer
a^ │ the value 10 bytes after this pointer
```

Avoid pointer chains — always point directly to the final value, not to another pointer.

**`[1, 1]`** — second element is a pointer to the first:

```rexc
3[^1+]
├╯│├╯╰─ array closer
│ │╰─── integer 1 (canonical value)
│ ╰──── pointer, offset 0 → resolves to 1+ immediately after
╰────── array prefix(3) and opener
```

**`[65, 65, 65]`** — two pointers, one canonical value at the end:

```rexc
6[1^^11+]
├╯├╯│╰┬╯╰─ array closer
│ │ │ ╰─── integer 65 (canonical value)
│ │ ╰───── pointer, offset 0 → 11+ (immediately after)
│ ╰─────── pointer, offset 1 → 11+ (1 byte after, skipping the ^)
╰───────── array prefix(6) and opener
```

## Indexes

Containers can be prefixed with count and index modifiers for fast access.

**`#` — Count.** Digit prefix is the item count. For arrays, count = number of elements. For objects, count = number of key-value pairs. Consumes the next value.

**`|` — Index.** Digit prefix is pointer width minus 1 (1-biased: `|` = width 1, `1|` = width 2, `2|` = width 3). Reads count × width bytes as a fixed-width pointer array, then consumes the next value. Pointers are byte offsets into the container body.

Width 1 covers offsets 0–63, width 2 covers 0–4,095, width 3 covers 0–262,143.

Count alone annotates a container without indexing. Count + index enables fast access:

```rexc
3#6[1+2+3+]               │ count only (3 items)
3#|0246[1+2+3+]           │ indexed: O(1) array access
2#|0ah{color:red:size:G+} │ indexed: O(log2 n) object lookup
```

### Indexed Array

For arrays, pointer *i* points to element *i*. Access is O(1) — multiply index by pointer width, read the offset, jump into the body.

**`[1, 2, 3]`** with index:

```rexc
3#|0246[1+2+3+]
├╯│╰┬╯╰───┬───╯
│ │ │     ╰──── the array
│ │ ╰────────── index: 0→1+, 2→2+, 4→3+
│ ╰──────────── pointer width 1
╰────────────── 3 items
```

### Indexed Object

For objects, pointers point to keys, sorted by key value for binary search. The value follows immediately after each key in the body.

**`{size: 42, color: "red"}`** with index:

```rexc
2#|70h{size:G+color:red:}
├╯│├╯╰────────┬────────╯
│ ││          ╰──── the object
│ │╰─────────────── sorted index: 7→color:, 0→size:
│ ╰──────────────── pointer width 1
╰────────────────── 2 pairs
```

To look up `size`: binary search the 2 sorted pointers, compare the key at each offset. One comparison finds `size:` at body offset 10.

## Worked Examples

### `(add 1 2)`

```rex
(add 1 2)
```

JSON bytecode: `["$add", 1, 2]` (14 bytes)

```rexc
5(!1+2+)       (8 bytes)
├╯│├╯├╯╰─ call closer
│ ││ ╰─── integer 2
│ │╰───── integer 1
│ ╰────── add (action 0)
╰──────── call prefix(5) and opener
```

### `x = 42`

```rex
x = 42
```

```json
["$set", ["$$x"], 42]
```

```rexc
4=x$G+
├╯├╯├╯
│ │ ╰── integer 42
│ ╰──── variable x
╰────── set prefix(4)
```

### `(when (gt x 10) (add x 1))`

```rex
(when (gt x 10)
  (add x 1))
```

```json
["$when",["$gt",["$$x"],10],["$add",["$$x"],1]]
``` 

```rexc
i(?6(8?x$a+)5(!x$1+))
├╯│╰───┬───╯╰──┬───╯╰─ call closer
│ │    │       ╰────── (add x 1)
│ │    ╰────────────── (gt x 10)
│ ╰─────────────────── when (query 0)
╰───────────────────── call prefix(18) and opener
```

### `{color: "red", size: 42}`

```rex
{ color: "red" size: 42 }
```

JSON is 25 bytes

```json
{"color":"red","size":42}
```

Rex-C is 20 bytes

```rexc
h{color:red:size:G+}
├╯╰─┬──╯╰┬─╯╰─┬─╯├╯╰─ object closer
│   │    │    │  ╰─── 42
│   │    │    ╰────── "size"
│   │    ╰─────────── "red"
│   ╰──────────────── "color"
╰──────────────────── object prefix(17) and opener
```

### `(alt user.name "anonymous")`

Rex is 28 bytes

```rex
(alt user.name "anonymous")
```

JSON is 38 bytes

```json
["$alt",["$$user","name"],"anonymous"]
```

Rex-C is 25 bytes

```rexc
m(2?user.name:anonymous:)
├╯├╯╰───┬────╯╰───┬────╯╰─ call closer
│ │     │         ╰─────── "anonymous"
│ │     ╰───────────────── user.name (navigated variable)
│ ╰─────────────────────── alt (query 2)
╰───────────────────────── call prefix(22) and opener
```

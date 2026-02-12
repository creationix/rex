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

| Tag | Type | Digits encode |
|-----|------|---------------|
| `+` | Positive integer | Value |
| `~` | Negative integer | -1 - Value |
| `.` | Decimal | Zigzag-encoded power of 10 (consumes next integer value) |
| `:` | Bare string | The string content itself |
| `!` | Opcode | Opcode ID |
| `@` | Reference | Reference ID |
| `$` | Variable | The variable name itself |
| `^` | Pointer | Byte offset to another value |

### Containers

| Delimiters | Type | Body contains |
|------------|------|---------------|
| `[` `]` | Array | Concatenated values |
| `{` `}` | Object | Alternating key, value pairs |
| `(` `)` | Call | First value determines call type |
| `<` `>` | Binary | Base64url-encoded bytes |
| `,` (no close) | String | Raw UTF-8 bytes |

Paired containers use closing delimiters for visual coherence and error checking. The `,` string has no closing delimiter — strings are common and the byte savings add up.

## Integers

Positive integers use `+`, negative use `~`. The two ranges don't overlap: `+` encodes 0 and up, `~` encodes -1 and down.

`+` encodes the value directly. `~` encodes **-1 - digits**, so `~` with no digits = -1.

```
+       0
1+      1
9+      9
a+      10
G+      42
1A+     100

~       -1
1~      -2
9~      -10
a~      -11
G~      -43
1A~     -101
```

## Decimals

The `.` tag encodes a decimal number in two parts. The digit prefix is a **zigzag-encoded power of 10**. The tag then **consumes the next value**, which must be a positive or negative integer, as the significand.

Zigzag mapping: 0 &rarr; 0, -1 &rarr; 1, 1 &rarr; 2, -2 &rarr; 3, 2 &rarr; 4, ...

The decoded value is: **significand &times; 10<sup>power</sup>**

```
.1+         1 × 10^0   = 1            power: zigzag(0) = 0
1.5+        5 × 10^-1  = 0.5          power: zigzag(1) = -1
3.4W+       314 × 10^-2 = 3.14        power: zigzag(3) = -2
c.1+        1 × 10^6   = 1000000      power: zigzag(12) = 6
b.~         -1 × 10^-6 = -0.000001    power: zigzag(11) = -6
3.4V~       -314 × 10^-2 = -3.14      power: zigzag(3) = -2, significand: -1-313 = -314
```

The sign lives on the significand integer (`~` encodes -1-digits, so `4V~` = -1-313 = -314). This preserves exact decimal representation with no floating-point rounding.

## Bare Strings

The `:` tag interprets the digit characters as literal string content instead of a number. The digit alphabet (`0-9a-zA-Z-_`) covers all Rex identifiers, so bare strings handle most keys and names with zero overhead.

```
:             ""
a:            "a"
hello:        "hello"
x-action:     "x-action"
foo_bar:      "foo_bar"
42:           "42" (string, not integer — the tag disambiguates)
```

For strings with spaces, punctuation, or unicode, use the length-prefixed `,` container:

```
,                   ""
b,hello world       "hello world"
```

## Opcodes

Core language opcodes are fixed IDs 0-29. Domain/framework opcodes use 30+.

| ID | Opcode | Enc. | | ID | Opcode | Enc. |
|----|--------|------|-|----|--------|------|
| 0 | `when` | `!` | | 15 | `not` | `f!` |
| 1 | `unless` | `1!` | | 16 | `xor` | `g!` |
| 2 | `do` | `2!` | | 17 | `add` | `h!` |
| 3 | `alt` | `3!` | | 18 | `sub` | `i!` |
| 4 | `all` | `4!` | | 19 | `mul` | `j!` |
| 5 | `set` | `5!` | | 20 | `div` | `k!` |
| 6 | `delete` | `6!` | | 21 | `mod` | `l!` |
| 7 | `eq` | `7!` | | 22 | `neg` | `m!` |
| 8 | `neq` | `8!` | | 23 | `string` | `n!` |
| 9 | `gt` | `9!` | | 24 | `number` | `o!` |
| 10 | `gte` | `a!` | | 25 | `object` | `p!` |
| 11 | `lt` | `b!` | | 26 | `array` | `q!` |
| 12 | `lte` | `c!` | | 27 | `boolean` | `r!` |
| 13 | `and` | `d!` | | 28 | `bytes` | `s!` |
| 14 | `or` | `e!` | | 29 | `literal` | `t!` |

The most common opcode (`when` = 0) encodes as a single byte: `!`.

## References

Pre-assigned constants. IDs 5+ are domain-defined.

| ID | Value | Encoding |
|----|-------|----------|
| 0 | `true` | `@` |
| 1 | `false` | `1@` |
| 2 | `null` | `2@` |
| 3 | `undefined` | `3@` |
| 4 | `self` | `4@` |
| 5+ | Domain-defined | `5@`, `6@`, ... |

## Variables

The `$` tag works like `:` — digit characters are the variable name. Rex identifiers always fit within the digit alphabet, so no length-prefixed variant is needed.

A bare variable is a simple read. For path navigation, wrap in a call:

```
x$                  ["$$x"]              read variable x
age$                ["$$age"]            read variable age
my-var$             ["$$my-var"]         read variable my-var
8(foo$bar:)         ["$$foo", "bar"]     navigate foo.bar
```

## Calls

The first value in a call body determines the call type:

| First value type | Meaning |
|------------------|---------|
| Opcode `!` | Core/domain operation |
| Variable `$` | Navigable expression (place read) |
| Reference `@` | Domain builtin navigation |

```
6(h!1+2+)               (add 1 2)         opcode call
8(foo$bar:)              foo.bar            variable navigation
9(5@host:)               (headers 'host')   builtin navigation (headers = ref 5)
```

## Objects

Body alternates key, value. Keys are typically bare strings, which makes the `:` tag pull double duty as a visual key-value separator:

```
{}                             {}
h{color:red:size:G+}           {color: "red", size: 42}
```

Breakdown of `h{color:red:size:G+}`:

```
h{color:red:size:G+}
  ╰────╯╰──╯╰───╯╰╯
  key    val key   val
```

## Arrays

```
[]                   []
6[1+2+3+]           [1, 2, 3]
```

## Binary

Body is base64url-encoded bytes. Matches the `<>` syntax in Rex source.

```
<>                   empty bytes
7<SGVsbG8>           <48 65 6c 6c 6f>
```

## Pointers

Deduplicate repeated values. The offset counts **forward** from the end of the pointer to the canonical value. The encoder places canonical values after their pointers so offsets always point forward.

```
^       the value immediately after this pointer (offset 0)
1^      the value 1 byte after this pointer
a^      the value 10 bytes after this pointer
```

Avoid pointer chains — always point directly to the final value, not to another pointer.

**`[1, 1]`** — second element is a pointer to the first:

```
3[^1+]
  │├╯
  │╰─── 1+   integer 1 (canonical value)
  ╰──── ^    pointer, offset 0 → resolves to 1+ immediately after
```

**`[65, 65, 65]`** — two pointers, one canonical value at the end:

```
6[1^^11+]
  ├╯│╰┬╯
  │ │ ╰── 11+  integer 65 (canonical value)
  │ ╰──── ^    pointer, offset 0 → 11+ (immediately after)
  ╰────── 1^   pointer, offset 1 → 11+ (1 byte after, skipping the ^)
```

## Worked Examples

### `(add 1 2)`

```rex
(add 1 2)
```

JSON bytecode: `["$add", 1, 2]` (14 bytes)

```
6(h!1+2+)       (8 bytes)
  ├╯├╯├╯
  │ │ ╰── 2+   integer 2
  │ ╰──── 1+   integer 1
  ╰────── h!   opcode 17 (add)
```

### `x = 42`

```rex
x = 42
```

JSON bytecode: `["$set", ["$$x"], 42]` (21 bytes)

```
6(5!x$G+)       (8 bytes)
  ├╯├╯├╯
  │ │ ╰── G+   integer 42
  │ ╰──── x$   variable x
  ╰────── 5!   opcode 5 (set)
```

### `(when (gt x 10) (add x 1))`

```rex
(when (gt x 10)
  (add x 1))
```

JSON bytecode: `["$when", ["$gt", ["$$x"], 10], ["$add", ["$$x"], 1]]` (49 bytes)

```
j(!6(9!x$a+)6(h!x$1+))       (22 bytes)
  │╰───┬───╯╰───┬───╯
  │    │        ╰─── 6(h!x$1+)  (add x 1)
  │    ╰──────────── 6(9!x$a+)  (gt x 10)
  ╰───────────────── !          when (opcode 0)
```

### `{color: "red", size: 42}`

```rex
{color: "red", size: 42}
```

JSON bytecode: `{"color":"red","size":42}` (25 bytes)

```
h{color:red:size:G+}       (20 bytes)
  ╰─┬──╯╰┬─╯╰─┬─╯├╯
    │    │    │  ╰── G+      42
    │    │    ╰───── size:   "size"
    │    ╰────────── red:    "red"
    ╰─────────────── color:  "color"
```

### `(alt user.name "anonymous")`

```rex
(alt user.name "anonymous")
```

JSON bytecode: `["$alt", ["$$user", "name"], "anonymous"]` (40 bytes)

```
p(3!a(user$name:)anonymous:)       (28 bytes)
  ├╯╰─────┬─────╯╰───┬────╯
  │       │          ╰───── anonymous:         "anonymous"
  │       ╰──────────────── a(user$name:)      user.name
  ╰──────────────────────── 3!                 alt (opcode 3)
```

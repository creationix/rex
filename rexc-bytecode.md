# Rex Bytecode Format (`rexc`)

Rex compiles to `rexc` — a compact bytecode that serializes as a UTF-8 string. This is the format that Rex interpreters execute. It embeds directly in JSON string values with minimal escaping.

## Format Basics

Every encoded value is a **prefix** of base-64 digits followed by a **type tag**:

```
<digits><tag>                     scalar (digits are the value)
<digits><open><body><close>       paired container (digits are byte-length of body)
<digits><tag><body>               fixed-arity operator (digits are byte-length of body)
```

The type tag is the first non-digit character. It determines how to interpret the digit prefix and whether a body follows.

**Byte-length prefixes are optional on paired containers.** The encoder adds them only when a value needs to be skippable in O(1) — for example, values inside non-indexed arrays and objects. At top level, inside indexed containers, and in other positions where skipping isn't needed, the prefix is omitted.

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

| Tag | Type        | Digits encode                                            |
|-----|-------------|----------------------------------------------------------|
| `+` | Integer     | Zigzag-encoded value (0→0, -1→1, 1→2, -2→3, 2→4, ...)    |
| `*` | Decimal     | Zigzag-encoded power of 10 (consumes next integer value) |
| `:` | Bare string | The string content itself                                |
| `%` | Opcode      | Opcode ID                                                |
| `@` | Self        | Depth (empty=`self`, `1@`=one level up, etc.)          |
| `'` | Reference   | Reference ID                                             |
| `$` | Variable    | The variable name itself                                 |
| `^` | Pointer     | Byte offset to another value                             |
| `;` | Loop control| Encodes break/continue kind and depth                    |

### Paired Containers (optional byte-length prefix)

| Delimiters | Type   | Body contains                    |
|------------|--------|----------------------------------|
| `[` `]`    | Array  | Concatenated values              |
| `{` `}`    | Object | Alternating key, value pairs     |
| `(` `)`    | Call   | First value determines call type |

### Unpaired Containers (required byte-length prefix)

| Tag | Type   | Body contains   |
|-----|--------|-----------------|
| `,` | String | Raw UTF-8 bytes |

### Mutation Operators (optional byte-length prefix, fixed arity)

| Tag | Type     | Body contains     |
|-----|----------|-------------------|
| `=` | Set      | Place, then value |
| `/` | Swap-set | Place, then value |
| `~` | Delete   | Place             |

### Control-Flow Containers (optional byte-length prefix)

| Opener | Type   | Body contains                                |
|--------|--------|----------------------------------------------|
| `?(`   | When   | cond, then-expr, else-expr?                  |
| `!(`   | Unless | cond, then-expr, else-expr?                  |
| `\|(`  | Alt    | expr, expr, ... (first non-undefined wins)   |
| `&(`   | All    | expr, expr, ... (first undefined short-circuits) |
| `>(`   | For-in | iterable, body OR iterable, value-var, body OR iterable, key-var, value-var, body |
| `<(`   | For-of | iterable, body OR iterable, key-var, body    |
| `#(`   | While  | condition, body                               |
| `>[`,`<[` | Array comprehension | iteration clause, body expression |
| `>{`,`<{` | Object comprehension | iteration clause, key expression, value expression |

### Structural

| Char | Role                                     |
|------|------------------------------------------|
| `#`  | Index marker inside `[]` and `{}` bodies |

## Integers

The `+` tag uses **zigzag encoding** for all integers, matching the decimal power encoding. Zigzag interleaves positive and negative values so small magnitudes get short encodings regardless of sign:

| Value | Zigzag | Encoding |
|-------|--------|----------|
| 0     | 0      | `+`      |
| -1    | 1      | `1+`     |
| 1     | 2      | `2+`     |
| -2    | 3      | `3+`     |
| 2     | 4      | `4+`     |
| 10    | 20     | `k+`     |
| -10   | 19     | `j+`     |
| 42    | 84     | `1k+`    |
| -42   | 83     | `1j+`    |
| 100   | 200    | `38+`    |
| -100  | 199    | `37+`    |

Zigzag formula: `encode(n) = n >= 0 ? 2n : -2n - 1`, `decode(z) = z even ? z/2 : -(z+1)/2`

## Decimals

The `*` tag encodes a decimal number in two parts — "significand times power of 10". The digit prefix is a **zigzag-encoded power of 10**. The tag then **consumes the next value**, which must be an integer, as the significand.

The decoded value is: **significand &times; 10<sup>power</sup>**

```rexc
*2+    │ 1 × 10^0   = 1         │ power: zigzag(0) = 0
1*a+   │ 5 × 10^-1  = 0.5       │ power: zigzag(1) = -1
3*9Q+  │ 314 × 10^-2 = 3.14     │ power: zigzag(3) = -2
c*2+   │ 1 × 10^6   = 1000000   │ power: zigzag(12) = 6
b*1+   │ -1 × 10^-6 = -0.000001 │ power: zigzag(11) = -6
3*9P+  │ -314 × 10^-2 = -3.14   │ power: zigzag(3) = -2, significand: zigzag(627) = -314
```

Both the power and significand use zigzag encoding, so the sign of any decimal is determined by the significand's zigzag value being odd (negative) or even (positive).

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

Opcodes use short mnemonic string keys. The digit prefix is the raw key string (not a base-64 integer). Control flow (`when`, `unless`, `alt`, `all`, loops, loop control) has dedicated syntax and is not in this table.

| Opcode | Enc.  |  | Opcode    | Enc.  |
|--------|-------|--|-----------|-------|
| `do`   | `%`   |  | `and`     | `an%` |
| `add`  | `ad%` |  | `or`      | `or%` |
| `sub`  | `sb%` |  | `xor`     | `xr%` |
| `mul`  | `ml%` |  | `not`     | `nt%` |
| `div`  | `dv%` |  | `boolean` | `bt%` |
| `eq`   | `eq%` |  | `number`  | `nm%` |
| `neq`  | `nq%` |  | `string`  | `st%` |
| `lt`   | `lt%` |  | `array`   | `ar%` |
| `lte`  | `le%` |  | `object`  | `ob%` |
| `gt`   | `gt%` |  | `mod`     | `md%` |
| `gte`  | `ge%` |  | `neg`     | `ng%` |
| `range`| `rn%` |  |           |       |

Domain functions also compile as opcodes with their own short codes (e.g., `jp%` for `json.parse`).

Opcodes are used as the first value inside `()` calls:

```rexc
(ad%2+4+)   │ 1 + 2
(gt%x$k+)   │ x > 10
(%=x$k+E+)  │ do x = 10 20 end
```

## References

References use short mnemonic string keys. The digit prefix is the raw key string (not a base-64 integer).

**Built-in constants:**

| Value       | Encoding |
|-------------|----------|
| `true`      | `tr'`    |
| `false`     | `fl'`    |
| `null`      | `nl'`    |
| `undefined` | `un'`    |
| `NaN`       | `nan'`   |
| `Infinity`  | `inf'`   |
| `-Infinity` | `nif'`   |

**Domain data** also compiles as references with short codes defined in the domain config (e.g., `H'` for `headers`, `M'` for `method`).  By convention, domain references use uppercase letters to distinguish them from opcode references.

For navigation into a domain reference, use a call:

```rexc
(H'host:)                   │ headers.host
(H'x-forwarded-for:origin:) │ headers.x-forwarded-for.origin
(H'key$)                    │ headers[key]
```

## Self Depth

`@` reads `self` from a dynamic depth stack:

```rexc
@   │ self (depth 1)
1@  │ parent self (depth 2)
2@  │ grandparent self (depth 3)
```

Depth decode rule: `depth = prefix + 1`.

## Variables

The `$` tag works like `:` — digit characters are the variable name. Rex identifiers always fit within the digit alphabet, so no length-prefixed variant is needed.

```rexc
x$      │ read variable x
age$    │ read variable age
my-var$ │ read variable my-var
```

For navigation, use a call:

```rexc
(user$name:)           │ user.name
(user$address:street:) │ user.address.street
(table$key$)           │ table[key]
```

## Set, Swap-Set, and Delete

The `=` operator binds a value to a place. Fixed arity: place then value. The `/` operator binds a value to a place but returns the **old** value (swap-set). Fixed arity: place then value. The `~` operator removes a place. Fixed arity: place only.

All three have an optional byte-length prefix for when the operation itself needs to be skippable.

```rexc
=x$1k+                  │ x = 42
/x$1k+                  │ x := 42 (returns old value of x)
=(H'x-handler:)handler$ │ headers['x-handler'] = handler
~x$                     │ delete x
~(user$temp:)           │ delete user.temp
```

## Calls

The `(` `)` container groups a function-like expression. The first value determines the call type:

| First value type | Meaning                   |
|------------------|---------------------------|
| Opcode `%`       | Operation call            |
| Variable `$`     | Navigation (place read)   |
| Reference `'`    | Domain builtin navigation |
| Any other value  | Navigation from expression result |

```rexc
(ad%2+4+)                   │ 1 + 2
(gt%x$k+)                   │ x > 10
(user$address:street:)      │ user.address.street
(H'x-forwarded-for:origin:) │ headers.x-forwarded-for.origin
({a:2+}a:)                  │ {a:1}.a
```

## Control Flow

Control-flow operations have dedicated container syntax with compound openers. `?(`, `!(`, `|(`, `&(`, `>(`, and `<(` close with `)`. `>[` closes with `]`, and `>{` closes with `}`. The encoder adds byte-length prefixes to container values in skip positions.

### When / Unless

```rexc
?(cond then-expr)            │ when: evaluate then if cond is defined
?(cond then-expr else-expr)  │ when: evaluate then or else
!(cond then-expr else-expr)  │ unless: evaluate then if cond is undefined
```

The condition is always evaluated. Then-expr and else-expr are in skip positions — the interpreter jumps past whichever branch isn't taken. Container values in these positions get byte-length prefixes.

```rexc
?((gt%x$k+)7(ad%x$2+)7(sb%x$4+))
├╯╰───┬───╯╰────┬───╯╰────┬───╯╰─ closer
│     │         │         ╰────── else: sub(x, 2) — prefixed, skip position
│     │         ╰──────────────── then: add(x, 1) — prefixed, skip position
│     ╰────────────────────────── cond: gt(x, 10) — bare, always evaluated
╰──────────────────────────────── when opener
```

### Logical Or / And / Not / Nor

```rexc
|(expr1 expr2 expr3)  │ or: first non-undefined result or return undefined
&(expr1 expr2 expr3)  │ and: short-circuit on first undefined or return last value
!(expr)               │ not: return undefined if expr is defined, otherwise return true
!(expr1 expr2)        │ nor: return expr2 if expr1 is undefined, otherwise undefined
```

The first expression is always evaluated. Remaining expressions are in skip positions (the operation may short-circuit past them).

`nor` reuses the `!(` unless container with exactly 2 arguments. This is equivalent to `unless expr1 do expr2 end`.

```rexc
|((user$name:)anonymous:)
├╯╰────┬─────╯╰───┬────╯╰─ closer
│      │          ╰─────── "anonymous" — scalar, self-delimiting
│      ╰────────────────── user.name — bare, always evaluated
╰───────────────────────── or opener
```

### Loops and Comprehensions

`for` and `while` forms are dedicated control-flow containers, not opcodes.

```rexc
>(iter body)        │ for in iter do ... end
>(iter v$ body)     │ for v in iter do ... end
>(iter k$ v$ body)  │ for k, v in iter do ... end
<(iter body)        │ for of iter do ... end
<(iter k$ body)     │ for k of iter do ... end
#(cond body)        │ while cond do ... end
```

Comprehensions put the body expression first, then `for`/`in`/`of` and the iterator:

```rexc
>[iter val]            │ [val in iter] array comprehension (self is value)
>[iter v$ val]         │ [val for v in iter]
>[iter k$ v$ val]      │ [val for k, v in iter]
>{iter key val}        │ {key:val in iter} object comprehension (self is value)
>{iter v$ key val}     │ {key:val for v in iter}
>{iter k$ v$ key val}  │ {key:val for k, v in iter}
<[iter val]            │ [val of iter] array comprehension (self is key)
<[iter k$ val]         │ [val for k of iter]
<{iter key val}        │ {key:val of iter} object comprehension  (self is key)
<{iter k$ key val}     │ {key:val for k of iter}
#[conf val]            │ [val while conf] while loop (self is value)
#{cond key val}        │ {key:val while cond} while loop (self is key)
```

`>[...]` collects defined body results into a new array (undefined results are skipped). `>{...}` evaluates key/value expressions and writes entries only when the value is defined.

`break` and `continue` use scalar `;` with a compact digit payload:

```rexc
;    │ break depth 1
1;   │ continue depth 1
2;   │ break depth 2
3;   │ continue depth 2
```

Decode rule: `kind = n % 2` (`0=break`, `1=continue`), `depth = floor(n / 2) + 1`.

`;` is valid only inside loop bodies; otherwise decoding/validation must fail.

## Objects

Body alternates key, value pairs. Keys are typically bare strings, which makes the `:` tag pull double duty as a visual separator.

```rexc
{color:red:size:1k+}
│╰─┬──╯╰┬─╯╰─┬─╯╰┬╯╰─ closer
│  │    │    │   ╰─── val 42
│  │    │    ╰─────── key "size"
│  │    ╰──────────── val "red"
│  ╰───────────────── key "color"
╰──────────────────── opener
```

When an object is in a skip position (e.g., value in a non-indexed array), the encoder adds a byte-length prefix:

```rexc
j{color:red:size:1k+}   │ prefixed — body is 19 bytes
```

## Arrays

Arrays hold values with or without a length prefix depending on needs.

```rexc
[2+4+6+]
│╰─┬──╯╰─ closer
│  ╰───── elements: 1, 2, 3 (no prefixes needed)
╰──────── opener
6[2+4+6+] 
├╯╰─┬──╯╰─ closer
│   ╰───── elements: 1, 2, 3 (no prefixes needed)
╰───────── opener with length prefix
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
[^2+]
││├╯╰─ closer
││╰─── integer 1 (canonical value)
│╰──── pointer, offset 0 → resolves to 2+ immediately after
╰───── opener
```

## Indexes

The `#` marker appears inside `[]` and `{}` containers, immediately after the opening delimiter. It provides an index for O(1) element access (arrays) or O(log n) key lookup (objects).

Values inside indexed containers don't need byte-length prefixes — the index provides O(1) access.

### Indexed Arrays

The index entries point to the values

```rexc
[3#10242+4+6+]
│╰┬╯╰┬╯╰─┬──╯╰─ closer
│ │  │   ╰───── elements: 1, 2, 3 (no prefixes needed)
│ │  ╰───────── index: offset 0→2+, offset 2→4+, offset 4→6+
│ ╰──────────── index metadata (3x1)
╰────────────── opener
```

### Indexed Objects

The index entries point to the keys, but are sorted to enable fast binary search.

```rexc
{2#180size:1k+color:red:}
│╰┬╯├╯╰────────┬───────╯╰─ closer
│ │ │          ╰────────── key-value pairs (unsorted iteration order)
│ │ ╰───────────────────── sorted index: offset 8→color:, offset 0→size:
│ ╰─────────────────────── index metadata (2x1)
╰───────────────────────── opener
```

## Skip Rules

The encoder adds byte-length prefixes to container values only where O(1) skipping is needed. Scalars and strings are already self-delimiting.

**No prefix needed:**
- Top-level value
- Inside indexed containers (index provides direct access)
- Condition in `?(` / `!(`  (always evaluated)
- First expression in `|(` / `&(` (always evaluated)
- Iterable and binding slots in `>(` / `<(` (always evaluated)
- Iterable and binding slots in `>[` / `>{` (always evaluated)
- All arguments in regular `()` calls (all evaluated)
- Body of `=` / `/` / `~` (fixed arity, all parts evaluated)
- `;` loop-control scalar (self-delimiting)

**Prefix added to container values in:**
- Non-indexed array elements
- Non-indexed object values
- Then-expr and else-expr in `?(` / `!(`
- Second and later expressions in `|(` / `&(`
- Loop body in `>(` / `<(` (skip/jump target)
- Body expression in `>[` (skip/jump target)
- Key/value expressions in `>{` (skip/jump targets)

## Worked Examples

### `1 + 2`

```rexc
(ad%2+4+)
│╰┬╯├╯├╯╰─ call closer
│ │ │ ╰─── integer 2 (zigzag)
│ │ ╰───── integer 1 (zigzag)
│ ╰─────── add opcode
╰───────── call opener
```

### `x = 42`

```rexc
=x$1k+
│├╯╰┬╯
││  ╰─ integer 42
│╰──── variable x
╰───── set operator
```

### `when x > 10 do x + 1 end`

```rexc
?((gt%x$k+)7(ad%x$2+))
├╯╰───┬───╯╰────┬───╯╰─ closer
│     │         ╰────── then: (add x 1) — prefixed(7), skip position
│     ╰─────────────── cond: x > 10 — bare, always evaluated
╰───────────────────── when opener
```

### `{color: "red", size: 42}`

```rexc
{color:red:size:1k+}
│╰─┬──╯╰┬─╯╰─┬─╯╰┬╯╰─ closer
│  │    │    │   ╰─── val 42
│  │    │    ╰─────── key "size"
│  │    ╰──────────── val "red"
│  ╰───────────────── key "color"
╰──────────────────── opener
```

### `user.name or "anonymous"`

```rexc
|((user$name:)anonymous:)
├╯╰────┬─────╯╰───┬────╯╰─ alt closer
│      │          ╰─────── "anonymous" — scalar, self-delimiting
│      ╰────────────────── user.name — bare, first expr always evaluated
╰───────────────────────── alt opener
```

### `[when self % 3 > 0 do x * 3 end for x in 1..10]`

```rexc
>[(rn%2+k+)x$o?((gt%(md%@6+)+)7(ml%x$6+))]
├╯╰┬─╯╰┬╯╰─────┬──────╯╰───┬────╯│╰─ array comprehension closer
│  │   │       │           │     ╰── when closer
│  │   │       │           ╰──────── then: x * 3 — prefixed(7), skip position
│  │   │       ╰──────────────────── condition: self % 3 > 0
│  │   ╰──────────────────────────── when body opener (length prefixed)
│  ╰──────────────────────────────── x in 1..10 (range opcode produces [1..10])
╰─────────────────────────────────── array comprehension opener
```

This yields `[3, 6, 12, 15, 21, 24, 30]`.

### HTTP Server Action Annotations

This is a larger example using domain ref `H'` for `headers`.

```rex
map = {
  abc: "/letters"
  123: "/numbers"
}
when act = map.(headers.x-action) do
  headers.x-handler = act
end
```

This compiles down to 86 bytes:

```rexc
(%=map$r{abc:8,/letters3S+8,/numbers}?(=act$h(map$(H'x-action:))i=(H'x-handler:)act$))
```

This can be optimized using inline data and `self` instead of two local variables.

```rex
when {
  "abc": "/letters"
  "123": "/numbers"
}.(headers.x-action) do
  headers.x-handler = self
end
```

Which compiles down to 65 bytes:

```rexc
?(({abc:8,/letters123:8,/numbers}(H'x-action:))f=(H'x-handler:)@)
```

### Repeated Value with Pointers

Consider this array with a repeated large string value:

```rex
[
  { cache-key: "tenant:public:route:GET:/v1/search" }
  { cache-key: "tenant:public:route:GET:/v1/search" }
  { cache-key: "tenant:public:route:GET:/v1/search" }
]
```

Using pointers, this encodes to 81 bytes:

```rexc
[K{cache-key:-^}K{cache-key:L^}K{cache-key:y,tenant:public:route:GET:/v1/search}]
```

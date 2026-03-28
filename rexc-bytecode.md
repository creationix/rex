# Rex Bytecode Format (`.rexc`)

Rex compiles to REXC — a compact bytecode that serializes as a UTF-8 string. REXC is a **superset of [RX](rx-format.md)**: every valid RX document is valid REXC. The RX layer handles data encoding (numbers, strings, lists, maps, pointers, chains, indexes). REXC adds language constructs: variables, opcodes, control flow, mutation, and evaluation semantics.

REXC embeds directly in JSON string values with minimal escaping.

---

## Semantics

### Existence, Not Truthiness

Rex uses **existence** instead of truthiness. There is no concept of "falsy" — `false`, `null`, `0`, and `""` are all ordinary defined values. Only `undefined` represents absence.

This drives all control flow:

- **Comparisons** return the left-hand value on success, `undefined` on failure
- **`when`/`unless`** branch on whether a value is defined
- **`and`/`or`/`nor`** short-circuit on existence
- **Type predicates** return the value if it matches, `undefined` otherwise

### Lazy vs Eager Evaluation

| Container | Evaluation            | Returns             |
|-----------|-----------------------|---------------------|
| `;` `:`   | **lazy** — on access  | data structure      |
| `(` `)`   | **eager**             | call result         |
| `[` `]`   | **eager**, sequential | list of all results |
| `{` `}`   | **eager**, sequential | last result only    |

RX data containers (`;` lists, `:` maps) evaluate lazily — REXC expressions embedded inside are only executed when accessed. Reading element 5 of a `;` list does not evaluate elements 0–4.

The paired-delimiter containers (`()`, `[]`, `{}`) evaluate eagerly — all expressions are executed in order.

### Gas-Bounded Execution

Evaluation is gas-bounded. Gas is charged per loop/comprehension iteration. The host runtime sets the gas limit; a limit of 0 disables it.

---

## Reading Direction and Value Order

Like RX, REXC is parsed **right-to-left**. The parser starts at the rightmost byte, scans left past b64 digits to collect the varint, then reads the tag. See [rx-format.md](rx-format.md) for the full parsing algorithm.

Values within containers are stored in **reverse page order** so that right-to-left reading yields them in natural (source-code) order.

For example, `a < b` compiles to `lt(a, b)`. On the page the bytes are:

```rexc
(+b+a%lt)
```

Reading right-to-left: `)` → `%lt` (opcode) → `+a` (arg 1) → `+b` (arg 2) → `(`. The parser encounters the opcode first, then the arguments in source order.

Throughout this document, **reading order** means right-to-left. When describing what the parser encounters, earlier means further right on the page.

## Digit Alphabet

Same as RX — 64 URL-safe characters:

```
0  1  2  3  4  5  6  7  8  9      values 0-9
a  b  c  d  e  f  g  h  i  j      values 10-19
k  l  m  n  o  p  q  r  s  t      values 20-29
u  v  w  x  y  z  A  B  C  D      values 30-39
E  F  G  H  I  J  K  L  M  N      values 40-49
O  P  Q  R  S  T  U  V  W  X      values 50-59
Y  Z  -  _                        values 60-63
```

Digits form **big-endian base-64 integers**. Zero is an empty string (no digits). Canonical encoding uses the minimum number of digits.

---

## Tag Reference

### RX Data Layer (inherited)

See [rx-format.md](rx-format.md) for complete documentation.

| Tag | Name    | Kind          | Layout                 |
|-----|---------|---------------|------------------------|
| `+` | Number  | scalar        | `+[zigzag]`            |
| `*` | Decimal | scalar prefix | `+[base]*[exp]`        |
| `,` | String  | sized body    | `[bytes],[length]`     |
| `'` | Ref     | scalar        | `'[name]`              |
| `;` | List    | sized body    | `[children];[size]`    |
| `:` | Map     | sized body    | `[pairs]:[size]`       |
| `^` | Pointer | scalar        | `^[delta]`             |
| `.` | Chain   | sized body    | `[segments].[size]`    |
| `#` | Index   | sized body    | `[entries]#[compound]` |

### REXC Scalars

Scalars have no body — just a tag and varint. The tag is the rightmost non-b64 byte; the varint extends to its right.

| Tag | Name           | Varint encodes         |
|-----|----------------|------------------------|
| `$` | Variable       | name (string)          |
| `%` | Opcode         | mnemonic (string)      |
| `@` | Self           | depth offset (integer) |
| `\` | Break/Continue | kind + depth (integer) |

For `$` and `%`, the varint is a **string identifier** — the b64 characters are the name itself, not a numeric value. Same convention as `'` refs in RX.

### REXC Paired Containers

Paired containers use matching delimiters. On the page, the left delimiter is leftmost and the right delimiter is rightmost, with body bytes between them. The parser reads right-to-left: it encounters the right delimiter first, parses body values, then hits the left delimiter.

An optional size varint may appear to the right of the right delimiter when the container is in a skip position (see [Skip Rules](#skip-rules)).

| Left | Right | Name       | Evaluation        | Returns             |
|------|-------|------------|-------------------|---------------------|
| `(`  | `)`   | Call       | eager             | call result         |
| `[`  | `]`   | Eager list | eager, sequential | list of all results |
| `{`  | `}`   | Do block   | eager, sequential | last result         |

### REXC Compound Containers

A compound container is a paired container with a **type tag** next to the right delimiter: `( body )?`. The type tag is read first and identifies the container kind.

| Left | Right | Name                        |
|------|-------|-----------------------------|
| `(`  | `)?`  | When                        |
| `(`  | `)!`  | Unless / not / nor          |
| `(`  | `)\|` | Or (alt)                    |
| `(`  | `)&`  | And (all)                   |
| `(`  | `)>`  | For-in loop                 |
| `(`  | `)<`  | For-of loop                 |
| `(`  | `)#`  | While loop                  |
| `[`  | `]>`  | For-in list comprehension   |
| `[`  | `]<`  | For-of list comprehension   |
| `[`  | `]#`  | While list comprehension    |
| `{`  | `}>`  | For-in object comprehension |
| `{`  | `}<`  | For-of object comprehension |
| `{`  | `}#`  | While object comprehension  |

### REXC Mutation Operators

Fixed-arity operators. On the page: `[body][tag]`. The tag is rightmost; the body extends to its left. An optional size varint may appear to the right of the tag in skip positions.

| Tag | Name     | Body (in reading order)    |
|-----|----------|----------------------------|
| `=` | Set      | place, value               |
| `/` | Swap-set | place, value (returns old) |
| `~` | Delete   | place                      |

---

## Variables

The `$` tag encodes a variable name using b64 characters.

```rexc
$x       │ variable x
$age     │ variable age
$my-var  │ variable my-var
```

For navigation, wrap in a call. Arguments after the variable are key lookups:

```rexc
(name,4 $user)           │ user.name
(street,6 address,7 $user)  │ user.address.street
($key $table)            │ table[key]
```

## Opcodes

The `%` tag encodes an opcode mnemonic.

| Opcode | Encoding |  | Opcode    | Encoding |
|--------|----------|--|-----------|----------|
| `add`  | `%ad`    |  | `and`     | `%an`    |
| `sub`  | `%sb`    |  | `or`      | `%or`    |
| `mul`  | `%ml`    |  | `xor`     | `%xr`    |
| `div`  | `%dv`    |  | `not`     | `%nt`    |
| `eq`   | `%eq`    |  | `boolean` | `%bt`    |
| `neq`  | `%nq`    |  | `number`  | `%nm`    |
| `lt`   | `%lt`    |  | `string`  | `%st`    |
| `lte`  | `%le`    |  | `array`   | `%ar`    |
| `gt`   | `%gt`    |  | `object`  | `%ob`    |
| `gte`  | `%ge`    |  | `mod`     | `%md`    |
| `neg`  | `%ng`    |  | `range`   | `%rn`    |

Domain functions also compile as opcodes with their own mnemonics (e.g., `%jp` for `json.parse`).

## Self

`@` reads `self` from a dynamic depth stack. Depth = varint + 1.

```rexc
@    │ self (depth 1)
@1   │ parent self (depth 2)
@2   │ grandparent self (depth 3)
```

## References

Refs use the `'` tag from RX. The varint is a name, not a number.

**Built-in constants:**

| Value       | Encoding |
|-------------|----------|
| `true`      | `'t`     |
| `false`     | `'f`     |
| `null`      | `'n`     |
| `undefined` | `'u`     |
| `NaN`       | `'nan`   |
| `+Infinity` | `'inf`   |
| `-Infinity` | `'nif`   |

**Domain references** use short codes defined in the domain config (e.g., `'H` for `headers`, `'M` for `method`). By convention, domain refs use uppercase letters.

```rexc
(host,4 'H)                      │ headers.host
(origin,6 x-forwarded-for,f 'H)  │ headers.x-forwarded-for.origin
($key 'H)                        │ headers[key]
```

---

## Calls

The `(` `)` container groups a function-like expression. Reading right-to-left, the parser encounters `)`, then reads body values. The first body value read (rightmost on the page) determines the call type:

| First value type | Meaning                    |
|------------------|----------------------------|
| Opcode `%`       | Operation call             |
| Variable `$`     | Navigation (place read)    |
| Reference `'`    | Domain builtin navigation  |
| Any other value  | Navigation from expression |

```rexc
(+4 +2 %ad)          │ 1 + 2 → add(1, 2)
(+k $x %gt)          │ x > 10 → gt(x, 10)
(name,4 $user)       │ user.name
(host,4 'H)          │ headers.host
(a,1 +2a,1+4b,1:a)   │ {a:1, b:2}.a
```

## Eager List

The `[` `]` container evaluates all expressions in order and returns their results as a list. This differs from `;` (RX list), which evaluates lazily on access.

```rexc
[+6 +4 +2]                     │ → [1, 2, 3]
[(+4 $x %ml) (+2 $x %ad)]      │ → [x+1, x*2] (both evaluated in order)
```

Use `;` for inert data. Use `[]` when expressions have side effects or ordering dependencies.

## Do Block

The `{` `}` container evaluates all expressions in order and returns only the last result.

```rexc
{$y +4 $y= +1k $x=}     │ x = 42; y = 2; return y → 2
```

---

## Mutation

### Set

`=` binds a value to a place and returns the value. On the page: `[value][place]=`.

```rexc
+1k $x=                     │ x = 42
$handler ('H x-handler,9)=  │ headers['x-handler'] = handler
```

### Swap-Set

`/` binds a value and returns the **old** value. On the page: `[value][place]/`.

```rexc
+1k $x/              │ x := 42 (returns previous x)
```

### Delete

`~` removes a place. On the page: `[place]~`.

```rexc
$x~                  │ delete x
($user temp,4)~      │ delete user.temp
```

---

## Control Flow

### When / Unless

`(?…)?` evaluates its then-branch if the condition is defined. `(!…)!` evaluates if the condition is undefined.

In reading order (right-to-left), the parser encounters:
1. `)?` — compound right delimiter
2. Condition — always evaluated
3. Then-expr — in a skip position
4. Else-expr (optional) — in a skip position
5. `(` — left delimiter

```rexc
// when x do 42 end
(+1k$x)?

// when x > 10 do x + 1 end
((+2$x%ad)(+k$x%gt))?

// when x > 10 do x + 1 else x - 2 end
((+4$x%sb)(+2$x%ad)(+k$x%gt))?
├─────┬──╯╰───┬───╯╰───┬───╯╰── )? when
│     │       │        ╰─────── cond: gt(x, 10) — read first
│     │       ╰──────────────── then: add(x, 1) — skip position
│     ╰──────────────────────── else: sub(x, 2) — skip position
╰────────────────────────────── ( left delimiter
```

### Unless / Not / Nor

The `(!…)!` container has three forms depending on how many body values are present:

| Count | Semantics                                                    |
|-------|--------------------------------------------------------------|
| 1     | **not** — `undefined` if defined, `true` if undefined        |
| 2     | **nor** — expr2 if expr1 is undefined, else `undefined`      |
| 3     | **unless** — then-expr if cond undefined, with optional else |

### Or / And

`(|…)|` returns the first defined value. `(&…)&` short-circuits on the first undefined.

In reading order: the first expression is always evaluated. Remaining expressions are in skip positions.

```rexc
│ user.name or "anonymous"
(anonymous,9 ($user name,4))|
├────┬──────╯╰──────┬─────╯╰── )| or
│    │              ╰────────── read first: user.name — always evaluated
│    ╰───────────────────────── read second: "anonymous" — skip position
╰────────────────────────────── ( left delimiter
```

---

## Iteration

### For-In / For-Of Loops

`(>…)>` iterates values (`in`). `(<…)<` iterates keys (`of`).

In reading order:
1. Iterable — always evaluated
2. Bindings — zero or more `$` variables
3. Body expressions — remaining values

The number of `$` bindings determines the form:

| Compound | 0 bindings | 1 binding  | 2 bindings    |
|----------|------------|------------|---------------|
| `(>…)>`  | `for in`   | `for v in` | `for k, v in` |
| `(<…)<`  | `for of`   | `for k of` | —             |

In `in` forms, `self` is the current value. In `of` forms, `self` is the current key.

```rexc
│ for v in [1,2,3] do v + 1 end
((+2 $v %ad) $v +6+4+2;6)>
├──────┬────╯╰╯╰───┬───╯╰── )> for-in
│      │    │      ╰──────── read first: iterable [1,2,3]
│      │    ╰─────────────── read next: binding v
│      ╰──────────────────── read last: body add(v, 1)
╰─────────────────────────── ( left delimiter
```

### While Loops

`(#…)#` repeats while the condition is defined.

In reading order:
1. Condition — evaluated each iteration
2. Body expressions — remaining values

### Comprehensions

Comprehensions use `[…]` or `{…}` delimiters with iteration suffixes:

| Right | Iterates | Returns | Body after iterable/bindings |
|-------|----------|---------|------------------------------|
| `]>`  | for-in   | list    | value-expr                   |
| `]<`  | for-of   | list    | value-expr                   |
| `]#`  | while    | list    | value-expr                   |
| `}>`  | for-in   | map     | key-expr, value-expr         |
| `}<`  | for-of   | map     | key-expr, value-expr         |
| `}#`  | while    | map     | key-expr, value-expr         |

List comprehensions collect defined values (undefined results are skipped). Map comprehensions collect key-value pairs where the value is defined.

```rexc
│ [v * 2 for v in [1,2,3]] → [2,4,6]
[(+4 $v %ml) $v +6+4+2;6]>
```

### Loop Control

`\` encodes `break` and `continue`:

```
varint = (depth - 1) * 2 + kind
kind: 0 = break, 1 = continue
```

```rexc
\    │ break depth 1       (varint empty = 0)
\1   │ continue depth 1    (varint 1)
\2   │ break depth 2       (varint 2)
\3   │ continue depth 2    (varint 3)
```

`\` is valid only inside loop bodies. Requires `\\` escaping in JSON strings.

---

## Skip Rules

Paired containers in **skip positions** carry an optional size varint to the right of their right delimiter, enabling O(1) skipping. The size varint gives the byte count of the body (between delimiters). Scalars and RX sized-body containers (`,` `;` `:` `.`) are already self-delimiting and never need a skip varint.

**No skip varint needed:**
- Top-level value
- Inside indexed containers (index provides direct access)
- Condition in `(?…)?` / `(!…)!` (always evaluated)
- First expression in `(|…)|` / `(&…)&` (always evaluated)
- Iterable and bindings in loops/comprehensions (always evaluated)
- All arguments in `(…)` calls (all evaluated)
- All parts of `=` / `/` / `~` (all evaluated)

**Skip varint added to paired containers in:**
- Non-indexed `;` list elements
- Non-indexed `:` map values
- Then-expr and else-expr in `(?…)?` / `(!…)!`
- Later expressions in `(|…)|` / `(&…)&`
- Loop body in `(>…)>` / `(<…)<` / `(#…)#`
- Body/key/value expressions in comprehensions

---

## Worked Examples

### `1 + 2`

```rexc
(+4 +2 %ad)
├┬╯╰┬╯╰┬╯╰── ) right delimiter
││  │   ╰──── read first: opcode add
││  ╰──────── read next: integer 1 (zigzag 2)
│╰────────── read next: integer 2 (zigzag 4)
╰──────────── ( left delimiter
```

### `x = 42`

```rexc
+1k $x=
├┬╯╰┬╯╰── = set
│   ╰──── read first: place — variable x
╰──────── read next: value — integer 42
```

### `when x > 10 do x + 1 end`

```rexc
((+2 $x %ad) (+k $x %gt))?
├─────┬─────╯╰─────┬────╯╰── )? when
│     │            ╰──────── read first: cond gt(x, 10)
│     ╰───────────────────── read next: then add(x, 1) — skip position
╰─────────────────────────── ( left delimiter
```

### `user.name or "anonymous"`

```rexc
(anonymous,9 ($user name,4))|
├─────┬─────╯╰──────┬─────╯╰── )| or
│     │             ╰────────── read first: user.name
│     ╰──────────────────────── read next: "anonymous" — skip position
╰────────────────────────────── ( left delimiter
```

### `{color: "red", size: 42}`

Pure data — uses the RX `:` map, no REXC tags:

```rexc
+1ksize,4red,3color,5:k
```

### `[v * 2 for v in 1..10]`

```rexc
[(+4 $v %ml) $v (+k +2 %rn)]>
├─────┬─────╯╰╯╰──────┬────╯╰── ]> for-in list comprehension
│     │       │        ╰──────── read first: iterable range(1, 10)
│     │       ╰────────────────── read next: binding v
│     ╰────────────────────────── read next: value-expr mul(v, 2)
╰──────────────────────────────── [ left delimiter
```

### Lazy vs Eager

A `;` list with computed elements — each evaluated only on access:

```rexc
(+4 $x %ml)(+2 $x %ad);8
```

Reading element 0 evaluates `add(x, 1)` without touching element 1. Reading element 1 evaluates `mul(x, 2)` without touching element 0.

The same expressions in an eager list — both evaluate immediately, in order:

```rexc
[(+2 $x %ad)(+4 $x %ml)]
```

Both `add(x, 1)` and `mul(x, 2)` execute in that order. The result is a two-element list.

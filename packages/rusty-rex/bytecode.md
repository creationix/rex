# Rex Bytecode (v2)

Compact text format that encodes as a UTF-8 string and embeds in JSON values. Parsed and executed left-to-right, in place, with minimal allocations. A superset of the data layer: all JSON values have a direct encoding.

## Design Goals

- **Left-to-right** parsing and execution
- **Execute in place** — the interpreter works directly on the bytecode string, no deserialization
- **Random access** — O(1) for indexed arrays, O(log n) for indexed objects via sorted index + binary search
- **JSON-string safe** — prefers ASCII that needs no escaping; `\` is the only char requiring escaping (`\\`)

---

## Encoding Primitives

### Digit Alphabet (b64)

```
0-9   values 0–9
a-z   values 10–35
A-Z   values 36–61
-     value 62
_     value 63
```

### Varint

A sequence of b64 digits, big-endian base-64. Terminated by the next non-b64 byte or end of input. Empty sequence = 0.

```
(empty)  →  0
1        →  1
a        →  10
_        →  63
10       →  64
11       →  65
```

### Zigzag

Maps signed integers to unsigned: `zigzag(n) = n >= 0 ? 2n : -2n - 1`

```
 0 → 0    -1 → 1     1 → 2    -2 → 3     2 → 4
```

---

## Tag Reference

Every value starts with a **tag** (a non-b64 character). The tag determines what follows.

### Scalars

| Tag | Name | Encoding | Example |
|-----|------|----------|---------|
| `+` | Number | `+<zigzag>[*<zigzag_exp>]` | `+` = 0, `+4` = 2, `+9Q*3` = 3.14 |
| `,` | String | `,<byte_count><raw_bytes>` | `,5hello` = "hello" |
| `'` | Ref | `'<name>` | `'t` = true, `'n` = null |
| `$` | Variable | `$<name>` | `$x`, `$my-var` |
| `%` | Opcode | `%<mnemonic>` | `%ad` = add, `%lt` = lt |
| `@` | Self | `@[<varint>]` | `@` = self (depth 1), `@1` = depth 2 |
| `\` | Break/Cont | `\[<varint>]` | `\` = break, `\1` = continue |

#### Number Encoding

Integers use zigzag encoding. Decimals append `*<zigzag_exponent>` to represent `significand * 10^exponent`.

```
+        0
+2       1
+1       -1
+1k      42          zigzag(42) = 84 = 1×64 + 20 → "1k"
+1k*1    -420        42 × 10^(-1)... wait, that's 4.2
```

Actually let me re-express: the significand is the zigzag-decoded integer, the exponent shifts the decimal point.

```
+4*3     1 × 10^(-2) = 0.01       sig=zigzag(4)=2...
```

Hmm, let me simplify. The significand is the full integer (zigzag decoded). The exponent is zigzag-decoded and gives the power of 10.

```
3.14  →  sig = 314, exp = -2  →  +<zigzag(314)>*<zigzag(-2)>  →  +9s*3
0.5   →  sig = 5, exp = -1    →  +a*1
100   →  just +<zigzag(100)>   →  +3c  (no exponent needed)
```

#### Built-in References

| Encoding | Value |
|----------|-------|
| `'t` | true |
| `'f` | false |
| `'n` | null |
| `'u` | undefined |
| `'N` | NaN |
| `'I` | +Infinity |
| `'i` | -Infinity |

#### Opcodes

| Op | Enc | | Op | Enc | | Op | Enc |
|----|-----|--|----|-----|--|----|-----|
| add | `%ad` | | eq | `%eq` | | neg | `%ng` |
| sub | `%sb` | | neq | `%nq` | | range | `%rn` |
| mul | `%ml` | | lt | `%lt` | | string | `%st` |
| div | `%dv` | | lte | `%le` | | number | `%nm` |
| mod | `%md` | | gt | `%gt` | | object | `%ob` |
| b-and | `%an` | | gte | `%ge` | | array | `%ar` |
| b-or | `%or` | | not | `%nt` | | boolean | `%bt` |
| b-xor | `%xr` | | | | | | |

### Sized Containers

Tag followed by a **size varint** giving the byte count of the body. The body spans the next `size` bytes after the varint.

| Tag | Name | Body |
|-----|------|------|
| `;` | List (lazy) | elements — evaluated on access |
| `:` | Map (lazy) | key-value pairs — evaluated on access |
| `[` | Array (eager) | elements — all evaluated in order |
| `{` | Block (eager) | expressions — all evaluated, returns last |
| `(` | Call | callee, then args |

#### Container Index

Large containers (>16 elements) include an index as the first item in the body, starting with `#`:

```
#<count><off0><off1>...<offN>
```

Each offset is a varint giving the byte position of the element relative to the body start. The interpreter reads `#`, reads count, reads offsets, then can jump directly to any element.

For maps, the index entries are sorted by key (byte-order comparison of the encoded key), enabling binary search.

If the first body byte is not `#`, there is no index — the interpreter scans sequentially.

#### Calls

`(<size><callee><arg1><arg2>...`

The callee (first body value) determines the call type:

| Callee | Meaning |
|--------|---------|
| `%op` | Operation — apply opcode to args |
| `$var` | Navigation — look up args as keys on variable |
| `'ref` | Domain navigation — look up args on domain ref |
| other | Navigation — look up args on evaluated expression |

Navigation args are applied left-to-right. String args do static lookup; expression args do dynamic lookup.

```
($user,4name                user.name
($user,7address,6street     user.address.street
($table$key                 table.(key)
(%ad+2+4                    add(1, 2)
($f$x$y                     f(x, y)  — zero-arg: ($f
```

### Control Flow

| Tag | Name | Body | Children |
|-----|------|------|----------|
| `?` | When | `?<size><cond><then>[<else>]` | 2–3 |
| `!` | Unless | `!<size><expr>[<then>[<else>]]` | 1–3 |
| `&` | And | `&<size><left><right>` | 2 |
| `\|` | Or | `\|<size><left><right>` | 2 |

**When** (`?`): evaluate cond. If defined → evaluate and return then. If undefined → evaluate and return else (or undefined if no else).

**Unless** (`!`): child count determines semantics:
- 1 child: **not** — undefined if defined, true if undefined
- 2 children: **unless/nor** — if cond is undefined, evaluate then
- 3 children: **unless-else** — if cond is undefined, evaluate then; else evaluate else-branch

**And** (`&`): evaluate left. If defined → evaluate and return right. If undefined → return undefined. Right is skippable.

**Or** (`|`): evaluate left. If defined → return left. If undefined → evaluate and return right. Right is skippable.

### Loops

| Tag | Name | Body |
|-----|------|------|
| `>` | For-in | `><size><iterable>[<$bindings>]<body>` |
| `<` | For-of | `<<size><iterable>[<$bindings>]<body>` |
| `^` | While | `^<size><cond><body>` |

**Bindings**: 0–2 `$` variables between the iterable and the body.

| Tag | 0 bindings | 1 binding | 2 bindings |
|-----|-----------|-----------|------------|
| `>` | `for in` | `for v in` | `for k, v in` |
| `<` | `for of` | `for k of` | — |

### Comprehensions

| Tag | Name | Body |
|-----|------|------|
| `]` | List compr. | `]<size><kind><iterable>[<$bindings>]<value_expr>` |
| `}` | Map compr. | `}<size><kind><iterable>[<$bindings>]<key_expr><value_expr>` |

`kind` is a single byte: `>` (for-in), `<` (for-of), or `^` (while).

### Mutation

Fixed-arity operators. No size prefix — children are self-delimiting.

| Tag | Name | Body | Arity |
|-----|------|------|-------|
| `=` | Set | `=<place><value>` | 2 |
| `/` | Swap-set | `/<place><value>` | 2 (returns old value) |
| `~` | Delete | `~<place>` | 1 |

---

## Worked Examples

### `42`

```
+1k
```
zigzag(42) = 84. 84 = 1×64 + 20. Digit 1 = `1`, digit 20 = `k`. → `+1k`

### `"hello"`

```
,5hello
```

### `true`

```
't
```

### `[1, 2, 3]`

```
;6+2+4+6
```
Pure data → lazy list. zigzag: 1→2, 2→4, 3→6. Body = `+2+4+6` (6 bytes).

### `{name: "Ada", score: 95}`

```
:l,4name,3Ada,5score+2y
```
Lazy map. Pairs are key, value, key, value... Body size = `l` (21 in b64).

### `1 + 2`

```
(6%ad+2+4
```
Call with opcode `add`, args 1 and 2.

### `x = 42`

```
=$x+1k
```
Set: place = `$x`, value = `+1k`.

### `x += 1`

Desugars to `x = add(x, 1)`:
```
=$x(6%ad$x+2
```

### `user.name`

```
(9$user,4name
```
Navigation call: callee = `$user`, arg = string `"name"`.

### `routes.(route-key)`

```
(g$routes$route-key
```
Dynamic navigation: callee = `$routes`, arg = variable `$route-key`.

### `max = max or 100`

```
=$max|8$max+3c
```
Set max to (max or 100). Or-container: left = `$max`, right = `+3c` (zigzag(100)=200=3×64+8=`3c`). Wait, 200 = 3×64+8. Digit 3 = `3`, digit 8 = `8`. → `+38`. Hmm let me recalculate. 200 in base 64: 200/64 = 3 remainder 8. So "38". And `3` = `3`, `8` = `8`. → `+38`.

```
=$max|7$max+38
```

### `when x > 10 do x + 1 end`

```
?g(6%gt$x+k(6%ad$x+2
```
When-container (2 children): cond = `(6%gt$x+k` (gt(x, 5)... wait, zigzag(10)=20=`k`), then = `(6%ad$x+2` (add(x, 1)).

### `when x do y else z end`

```
?6$x$y$z
```
When-container (3 children): cond = `$x`, then = `$y`, else = `$z`.

### `for x in items do x + 1 end`

```
>e$items$x(6%ad$x+2
```
For-in: iterable = `$items`, binding = `$x`, body = `(6%ad$x+2` (add(x, 1)).

### `[self * self in items]`

```
]d>$items(6%ml@@
```
List comprehension, for-in kind (`>`): iterable = `$items`, no bindings, value = `(6%ml@@` (mul(self, self)).

### `{(u.name): u.score for u in users}`

```
}q>$users$u(9$u,4name(a$u,5score
```
Map comprehension, for-in: iterable = `$users`, binding = `$u`, key = `(9$u,4name` (u.name), value = `(a$u,5score` (u.score).

### Fibonacci program

```rex
max = max or 100
fibs = []
i = 0
a = 1
b = 1
while a <= max do
  fibs.(i) = a
  i += 1
  c = a + b
  a = b
  b = c
end
fibs
```

```
{...
  =$max|7$max+38
  =$fibs[
  =$i+
  =$a+2
  =$b+2
  ^...
    (%le$a$max
    {...
      =($fibs$i$a
      =$i(6%ad$i+2
      =$c(6%ad$a$b
      =$a$b
      =$b$c
    }
  $fibs
}
```
(sizes omitted for clarity)

---

## JSON Compatibility

Every JSON value maps directly to the data layer:

| JSON | Bytecode |
|------|----------|
| `number` | `+<zigzag>[*<exp>]` |
| `"string"` | `,<len><bytes>` |
| `true` | `'t` |
| `false` | `'f` |
| `null` | `'n` |
| `[array]` | `;<size><elements>` (lazy list) |
| `{object}` | `:<size><pairs>` (lazy map) |

Pure JSON data uses lazy containers (`;` `:`) — no evaluation needed. The interpreter returns values by reference directly into the bytecode string. Large JSON arrays/objects include a `#` index for random access.

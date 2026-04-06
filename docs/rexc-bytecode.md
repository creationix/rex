# REXC Bytecode Format

REXC is the bytecode for Rex, serialized as printable UTF-8. It is a strict superset of [RX](rx-format.md) — every valid RX document is valid REXC. This document covers only the extensions; see [rx-format.md](rx-format.md) for the base data format.

For the generic C ABI lazy-decoder interface used by LuaJIT FFI and other hosts,
see [rex-ffi-decoder.md](rex-ffi-decoder.md).

---

## Scalars

Same `[varint][tag]` rule as RX. `$` and `%` use the varint bytes as a name (like `'`), not as a number.

| Tag | Name | Varint |
|-----|------|--------|
| `$` | Variable | name |
| `%` | Opcode | mnemonic |
| `\` | Break/Continue | `(depth-1)*2 + kind` (0=break, 1=continue) |

---

## Calls

`(callee arg0 arg1 ...)` — first child is the callee, rest are arguments.

```
(ad%2+4+)                 add(1, 2) = 3
(user$4,name)             user.name
(user$7,address6,street)  user.address.street
```

---

## Control Flow

Modifier tags appear before a paired container's opener. The modifier + opener + closer form a compound container.

### Cond (`?`)

`?(c1 t1 [c2 t2 ...] [else])` — condition-body pairs, left to right. First defined condition's body is evaluated and returned. If no match, evaluate else (or return `none`).

```
?(c t)              when c do t end
?(c t e)            when c do t else e end
?(c1 t1 c2 t2 e)   when c1 do t1 else when c2 do t2 else e end
```

`unless` compiles to `?` with swapped branches:

```
?(c no' t)          unless c do t end
?(c e t)            unless c do t else e end
```

### And / Or (`&`, `|`)

Variadic, short-circuit on existence:

- **`&(a b c ...)`** — evaluate left to right. Return last value if all defined, first `none` otherwise.
- **`|(a b c ...)`** — return first defined value. All `none` returns `none`.

### Loops (`>`, `<`, `#`)

| Modifier | Name | Children |
|---|---|---|
| `>` `()` | For-in | iterable, [$bindings], body |
| `<` `()` | For-of | iterable, [$bindings], body |
| `#` `()` | While | cond, body |

Bindings: 1-2 `$` variables. `>` with 1 = `for v in`, with 2 = `for k, v in`. `<` with 1 = `for k of`.

### Comprehensions

Same modifiers with `[]` or `{}` instead of `()`:

| Tags | Name | Children |
|---|---|---|
| `>[]` | Array comp (for-in) | iterable, [$bindings], value_expr |
| `>{}` | Object comp (for-in) | iterable, [$bindings], key_expr, value_expr |
| `<[]` `<{}` | Same (for-of) | same |
| `#[]` `#{}` | Same (while) | cond, value/key_expr |

---

## Fixed-Arity Operators

Children follow the tag directly (no delimiters):

| Tag | Name | Children |
|-----|------|----------|
| `=` | Set | place, value |
| `/` | Swap-set | place, value (returns old) |
| `~` | Delete | place |

```
=x$1k+           x = 42
=x$(ad%x$2+)     x += 1  (desugared)
~x$               delete x
```

---

## Block

`(%expr0 expr1 ... exprN)` — the empty opcode (`%` with no mnemonic) followed by expressions. Evaluates sequentially, returns last.

`do...end` in Rex source compiles to a call with the empty opcode:

```
(%=x$2+x$)        do x = 1; x end
```

`{}` is **always** an object — never a code block. Code blocks use `(%...)`.

### Object Disambiguation

The first child after `{` determines the object variant. If it's a pointer or chain, resolve it recursively until you reach a concrete value. Then:

- **`#`** → indexed object (see [RX: Indexed Containers](rx-format.md#indexed-containers))
- **String** → key-value object (alternating key-value pairs)
- **Object or array** → schema-shared object (pointer to schema, then values only)
- **Anything else** → error (not a valid object)

Do not peek at the raw tag byte — the first child may be a pointer that resolves to a string or object.

---

## Return

`;[value]` — consumes the next value. Halts execution, propagates through all enclosing scopes.

```
;1k+       return 42
;no'       return none
```

---

## Template Literals

Compile to existing constructs — no new tags.

**Untagged** → string chain (`.`):
```
.[5,hello name$]              `hello ${name}`
```

**Tagged** → call with string parts array:
```
(html%[4,<a >5,</a>]title$)   html`<a>${title}</a>`
```

---

## Length Prefixes

Containers in skip positions get a varint length prefix before the opener — the byte count of the body (between delimiters). This lets the interpreter jump past skipped branches in constant time.

```
?(cond 4{2+4+} 2{6+})   branches are length-prefixed
=x$ 4[2+4+]             set value is length-prefixed
```

Scalars don't need prefixes (self-delimiting). No prefix (empty varint) falls back to scanning for the closer.

Since `[`, `{`, `(` are not b64 digits, a length prefix is always unambiguous.

---

## Semantics

### Existence

Only `none` is absence. `false`, `null`, `0`, `""` are defined values. Comparisons return value or `none`. Control flow branches on defined vs `none`.

### Variables

| Declaration | Visibility | Mutable | Optimizable |
|---|---|---|---|
| `extern x` | host → script | no | no |
| `extern mut x` | host ↔ script | yes | no |
| `x = ...` | local | yes | yes |

Host interface = `extern` bindings (inputs) + `extern mut` bindings (bidirectional) + return value (output).

### Gas

Charged per loop/comprehension iteration. Host sets limit; 0 = unlimited.

---

## Core Opcodes

Opcodes (`%`) are called with `(opcode arg0 arg1 ...)`. The mnemonic is encoded as b64 bytes in the varint position. These are the built-in opcodes that every Rex interpreter must support:

### Arithmetic

| Mnemonic | Rex syntax | Description |
|----------|-----------|-------------|
| `ad` | `a + b` | Add (numbers: arithmetic; strings: concatenate; arrays: concatenate) |
| `sb` | `a - b` | Subtract |
| `ml` | `a * b` | Multiply |
| `dv` | `a / b` | Divide (division by zero → `nan`) |
| `md` | `a % b` | Modulo (division by zero → `nan`) |
| `ng` | `-a` | Negate (unary minus) |

### Comparison

All comparisons return the left operand if the condition holds, `none` otherwise.

| Mnemonic | Rex syntax | Description |
|----------|-----------|-------------|
| `eq` | `a == b` | Equal |
| `nq` | `a != b` | Not equal |
| `gt` | `a > b` | Greater than |
| `ge` | `a >= b` | Greater than or equal |
| `lt` | `a < b` | Less than |
| `le` | `a <= b` | Less than or equal |

### Bitwise

| Mnemonic | Rex syntax | Description |
|----------|-----------|-------------|
| `an` | `a & b` | Bitwise AND |
| `or` | `a \| b` | Bitwise OR |
| `xr` | `a ^ b` | Bitwise XOR |
| `nt` | `~a` | Bitwise NOT (ints) / logical NOT (bools) |

### Type Predicates

Return the argument if it matches the type, `none` otherwise.

| Mnemonic | Rex syntax | Description |
|----------|-----------|-------------|
| `st` | `isString(x)` | Is string |
| `nm` | `isNumber(x)` | Is number (integer or decimal) |
| `ig` | `isInteger(x)` | Is integer |
| `ob` | `isObject(x)` | Is object |
| `ar` | `isArray(x)` | Is array |
| `bt` | `isBoolean(x)` | Is boolean |

### Other

| Mnemonic | Rex syntax | Description |
|----------|-----------|-------------|
| `rn` | `a..b` | Range (generates ascending or descending integer sequence) |
| `no` | `none` | The `none` value (encoded as ref, not opcode) |

### Host Opcodes

Hosts may define additional opcodes for domain-specific operations (e.g., `json.parse` → `jp`, `db.list` → `dl`). These are generated by `compile_with_domain()` from `.rexd` extern declarations. Host opcodes use the same `%` tag and call syntax — they cannot conflict with core opcodes because core mnemonics are fixed two-letter codes and host mnemonics are derived from namespace+method initials.

---

## Tag Summary

All [RX tags](rx-format.md) plus:

| Tag | Kind | Description |
|-----|------|-------------|
| `$` | scalar | Variable (name in varint) |
| `%` | scalar | Opcode (mnemonic in varint) |
| `\` | scalar | Break/continue |
| `(` `)` | paired | Call |
| `?` | modifier | Cond (before `()`) |
| `&` | modifier | And (before `()`) |
| `\|` | modifier | Or (before `()`) |
| `>` | modifier | For-in (before `()` `[]` `{}`) |
| `<` | modifier | For-of (before `()` `[]` `{}`) |
| `#` | modifier | While (before `()` `[]` `{}`) |
| `=` | fixed | Set (2 children) |
| `/` | fixed | Swap-set (2 children) |
| `~` | fixed | Delete (1 child) |
| `;` | prefix | Return (1 child) |

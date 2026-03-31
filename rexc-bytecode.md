# REXC Bytecode Format

REXC is a compact bytecode for the Rex language, serialized as printable UTF-8. It is a **strict superset of [RX](rx-format.md)** — every valid RX document is valid REXC.

REXC extends RX with: variables, opcodes, calls, control flow, loops, comprehensions, mutation, blocks, and return. This document covers only the extensions. See [rx-format.md](rx-format.md) for the base data format (parsing rule, b64 alphabet, zigzag, scalars, containers, pointers, chains, indexes).

---

## Additional Paired Container

RX uses `[]` and `{}`. REXC adds one more paired container:

| Open | Close | Name |
|------|-------|------|
| `(`  | `)`   | Call |

Calls evaluate: the first child is the callee, the rest are arguments.

```
(ad%2+4+)                -> add(1, 2) = 3
(user$4,name)            -> user.name
(user$7,address6,street) -> user.address.street
```

---

## Additional Scalars

| Tag | Name           | Varint meaning                             |
|-----|----------------|--------------------------------------------|
| `$` | Variable       | name (opaque b64 bytes, like `'` for refs) |
| `%` | Opcode         | mnemonic (opaque b64 bytes)                |
| `\` | Break/Continue | `(depth-1)*2 + kind` (0=break, 1=continue) |

These follow the same `[varint][tag]` parsing rule as RX scalars. `$` and `%` use the varint bytes as a name (like `'`), not as a number.

---

## Compound Modifiers

REXC adds modifier tags that appear **before** a paired container's opening delimiter. The modifier + opener + closer form a compound container.

### Control Flow

| Modifier | Container | Name | Children             |
|----------|-----------|------|----------------------|
| `?`      | `(` `)`   | Cond | variadic (see below) |
| `&`      | `(` `)`   | And  | variadic             |
| `\|`     | `(` `)`   | Or   | variadic             |

**And** `&(a b c ...)`: evaluate left to right. If all children are defined, return the last value. At the first `none`, stop and return `none`. Zero children returns `none`.

**Or** `|(a b c ...)`: evaluate left to right. Return the first defined value. If all children are `none`, return `none`. Zero children returns `none`.

**Cond** `?(c1 t1 [c2 t2 ...] [else])`: evaluate condition-body pairs left to right. For each pair, evaluate the condition; if defined, evaluate the corresponding body and return it (skip all remaining pairs and else). If no condition matches, evaluate and return the else expression. If no else (even number of children), return `none`.

```
?(c t)              // when c do t end
?(c t e)            // when c do t else e end
?(c1 t1 c2 t2 e)   // when c1 do t1 else when c2 do t2 else e end
```

The source-level `unless` keyword compiles to `?` with a `none` placeholder in the then position:

```
unless c do t end          ->  ?(c no' t)
unless c do t else e end   ->  ?(c e t)
```

### Loops

| Modifier | Container | Name   | Children                    |
|----------|-----------|--------|-----------------------------|
| `>`      | `(` `)`   | For-in | iterable, [$bindings], body |
| `<`      | `(` `)`   | For-of | iterable, [$bindings], body |
| `#`      | `(` `)`   | While  | cond, body                  |

Bindings: 1-2 `$` variables between iterable and body.

| Modifier     | 1 binding  | 2 bindings    |
|--------------|------------|---------------|
| `>` (for-in) | `for v in` | `for k, v in` |
| `<` (for-of) | `for k of` | --            |

### Comprehensions

Comprehensions use the same modifiers but with `[]` or `{}` instead of `()`:

| Tags     | Name                        | Children                                    |
|----------|-----------------------------|---------------------------------------------|
| `>[` `]` | For-in array comprehension  | iterable, [$bindings], value_expr           |
| `<[` `]` | For-of array comprehension  | iterable, [$bindings], value_expr           |
| `#[` `]` | While array comprehension   | cond, value_expr                            |
| `>{` `}` | For-in object comprehension | iterable, [$bindings], key_expr, value_expr |
| `<{` `}` | For-of object comprehension | iterable, [$bindings], key_expr, value_expr |
| `#{` `}` | While object comprehension  | cond, key_expr, value_expr                  |

### Modifier Parsing

The decoder encounters a modifier as a tag byte (after reading the varint, which is always empty for modifiers). It then reads the next byte as the opener and parses children until the matching closer:

```
// In read_value, after reading raw varint and tag:
case '?', '|', '&', '>', '<', '#':
    opener = read_byte(input, pos)          // '(' or '[' or '{'
    closer = matching_closer(opener)        // ')' or ']' or '}'
    children = []
    while input[pos] != closer:
        children.push(read_value(input, pos))
    pos += 1   // consume closer
    return CompoundNode(tag, opener, children)
```

**Disambiguation of `#`**: as an index, `#` is always followed by b64 digits (the index is a non-empty array of b64 pointer values). As a modifier, `#` is always followed by `(`, `[`, or `{`. Since opener bytes are not b64 digits, the parser distinguishes the two cases by inspecting the byte after `#`.

---

## Length Prefixes (Skip Support)

In pure RX, containers never need length prefixes.  Random access in RX is enabled via indexes on select values. In REXC, the interpreter sometimes needs to skip values without fully parsing them. Delimited containers (anything with matching brackets, including compound modifiers) can always be skipped by scanning for the closing delimiter — but this requires parsing each nested child. A length prefix allows jumping past the entire body in constant time.

**Rule**: when a container appears in a skip position, the encoder always adds a length prefix — a varint before the opening delimiter giving the byte count of the body (between delimiters, excluding the delimiters themselves). This applies uniformly: inside conditional modifiers (`?`, `&`, `|`), fixed-arity operators (`=`, `/`), return (`;`), or anywhere else a value might be skipped.

Scalars never need length prefixes — they are self-delimiting. The decimal tag (`.`) is a special case: skip the `.` tag, then skip the embedded child value.

```
?(cond 4{2+4+} 2{6+})      // container branches are length-prefixed
&(a$ 4{2+4+} 2{6+})        // and: skippable children are prefixed
|(a$ 2{6+})                 // or: skippable children are prefixed
=x$ 4[2+4+]                // set: value child is length-prefixed
?(x$ 1k+)                  // scalar branch, no prefix needed
```

When the length prefix is zero (no b64 digits before the opener), the interpreter falls back to scanning for the matching closer. This is fine because an empty container is trivially fast to skip.

```
skip_value(input, pos):
    raw = read_b64_digits(input, pos)
    if raw is non-empty and input[pos] is '[' or '{' or '(':
        // length-prefixed container: jump in constant time
        size = b64_to_uint(raw)
        pos += 1       // skip opener
        pos += size    // jump past body
        pos += 1       // skip closer
    else:
        // scalar, zero-prefix container, or fixed-arity: skip by parsing
        pos = saved
        parse_and_discard(input, pos)
```

Since `[`, `{`, `(` are not b64 digits, the parser always distinguishes a length prefix from an unprefixed container.

---

## Fixed-Arity Operators

No delimiters -- children are self-delimiting values that follow the tag.

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

---

## Block

`{expr0 expr1 ... exprN}` -- evaluates all expressions sequentially, returns the last result. Uses the same `{}` delimiters as objects.

The interpreter distinguishes objects from blocks by peeking at the first child:
- First child is a string literal (varint + `,`) -> object (key-value pairs)
- First child resolves to an object/array -> schema-shared object
- First child is an index (varint + `#`) -> indexed object
- Otherwise -> code block

---

## Return

`;[value]` -- the `;` tag consumes the next value as the return expression. Halts execution and propagates through all enclosing blocks, loops, and conditionals.

```
;1k+             -> return 42
;no'             -> return none (bare return)
```

A bare `return` compiles to `;no'` -- the compiler injects `none` as the child.

---

## Tagged Template Literals

Template literals compile to string chains (`.`) for untagged, or to calls for tagged.

**Untagged** -- `` `hello ${name}` `` compiles to a string chain:
```
.[5,hello name$]     -> "hello " + name -> "hello Ada"
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
- `and`/`or` short-circuit on existence (variadic)
- Type predicates return the value if it matches, `none` otherwise

### Variable Scoping and Visibility

Variables (`$`) fall into two categories based on `extern` declarations (from `.rexd` or source):

| Declaration    | Visibility    | Mutable | Optimizable |
|----------------|---------------|---------|-------------|
| `extern x`     | host → script | no      | no (pinned) |
| `extern mut x` | host ↔ script | yes     | no (pinned) |
| `x = ...`      | script-local  | yes     | yes         |

The host interface is the `extern` contract plus the return value:

- **Inputs**: `extern` bindings (read-only) and `extern mut` bindings (initial values)
- **Outputs**: `extern mut` bindings (modified values) and the evaluation's return value (last expression)

All non-extern variables are local. The optimizer may replace them with stack slots, inline them, or eliminate them entirely.

### Gas-Bounded Execution

Gas is charged per loop/comprehension iteration. The host sets the limit; 0 = unlimited.

---

## Tag Summary

All RX tags (see [rx-format.md](rx-format.md)) plus:

| Tag  | Kind     | Varint meaning              | Body / children                             |
|------|----------|-----------------------------|---------------------------------------------|
| `$`  | scalar   | name (opaque)               | none                                        |
| `%`  | scalar   | mnemonic (opaque)           | none                                        |
| `\`  | scalar   | break/continue code         | none                                        |
| `(`  | opener   | (length prefix in skip pos) | values until `)`                            |
| `)`  | closer   | --                          | --                                          |
| `?`  | modifier | (empty)                     | compound: `?(` children `)` (variadic cond) |
| `\|` | modifier | (empty)                     | compound: `\|(` children `)` (variadic or)  |
| `&`  | modifier | (empty)                     | compound: `&(` children `)` (variadic and)  |
| `>`  | modifier | (empty)                     | compound with `(`, `[`, `{`                 |
| `<`  | modifier | (empty)                     | compound with `(`, `[`, `{`                 |
| `#`  | modifier | (empty)                     | compound with `(`, `[`, `{`                 |
| `=`  | fixed    | (empty)                     | 2 children                                  |
| `/`  | fixed    | (empty)                     | 2 children                                  |
| `~`  | fixed    | (empty)                     | 1 child                                     |
| `;`  | prefix   | (empty)                     | 1 child (return value)                      |

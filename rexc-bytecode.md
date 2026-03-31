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

| Tag | Name           | Varint meaning                              |
|-----|----------------|---------------------------------------------|
| `$` | Variable       | name (opaque b64 bytes, like `'` for refs)  |
| `%` | Opcode         | mnemonic (opaque b64 bytes)                 |
| `@` | Self           | depth (unsigned integer, 0 = current)       |
| `\` | Break/Continue | `(depth-1)*2 + kind` (0=break, 1=continue)  |

These follow the same `[varint][tag]` parsing rule as RX scalars. `$` and `%` use the varint bytes as a name (like `'`), not as a number.

---

## Compound Modifiers

REXC adds modifier tags that appear **before** a paired container's opening delimiter. The modifier + opener + closer form a compound container.

### Control Flow

| Modifier | Container | Name   | Children            |
|----------|-----------|--------|---------------------|
| `?`      | `(` `)`   | When   | cond, then [, else] |
| `!`      | `(` `)`   | Unless | cond, then [, else] |
| `\|`     | `(` `)`   | Or     | left, right         |
| `&`      | `(` `)`   | And    | left, right         |

**When** `?(cond then)` or `?(cond then else)`: evaluate cond. If defined, evaluate then and skip else. If none, skip then and evaluate else (or return none).

**Unless** `!(cond then)` or `!(cond then else)`: evaluate cond. If none, evaluate then and skip else. If defined, skip then and evaluate else (or return none).

**Or** `|(left right)`: evaluate left. If defined, return it and skip right. If none, evaluate right.

**And** `&(left right)`: evaluate left. If none, return none and skip right. If defined, evaluate right.

### Loops

| Modifier | Container     | Name    | Children                    |
|----------|---------------|---------|-----------------------------|
| `>`      | `(` `)`       | For-in  | iterable, [$bindings], body |
| `<`      | `(` `)`       | For-of  | iterable, [$bindings], body |
| `#`      | `(` `)`       | While   | cond, body                  |

Bindings: 0-2 `$` variables between iterable and body.

| Modifier     | 0 bindings | 1 binding  | 2 bindings    |
|--------------|------------|------------|---------------|
| `>` (for-in) | `for in`   | `for v in` | `for k, v in` |
| `<` (for-of) | `for of`   | `for k of` | --             |

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
case '?', '!', '|', '&', '>', '<', '#':
    opener = read_byte(input, pos)          // '(' or '[' or '{'
    closer = matching_closer(opener)        // ')' or ']' or '}'
    children = []
    while input[pos] != closer:
        children.push(read_value(input, pos))
    pos += 1   // consume closer
    return CompoundNode(tag, opener, children)
```

**Disambiguation of `#`**: as a modifier, `#` always has an empty varint (no b64 digits before it) and is followed by `(`, `[`, or `{`. As an index inside a container, `#` always has a non-empty varint. This is why the index `peek_is_index` check requires at least one b64 digit before `#`.

---

## Length Prefixes (Skip Support)

In pure RX, containers never need length prefixes. In REXC, conditional branches (`?`, `!`, `|`, `&`) may need to skip untaken branches at runtime. When a branch child is a container, the encoder adds a varint before the container's opening delimiter giving the byte count of the body (between delimiters, excluding the delimiters themselves):

```
?(cond 4{2+4+} 2{6+})     // branch blocks are length-prefixed
>(iterable bindings {body}) // not conditional, no prefix needed
?(x$ 1k+)                  // scalar branch, no prefix needed
```

The length prefix is only added to container-valued children at index > 0 (the condition at index 0 is always evaluated). The interpreter reads the prefix, jumps past the opener + body + closer without parsing:

```
skip_with_prefix(input, pos):
    raw = read_b64_digits(input, pos)
    if raw is non-empty and input[pos] is '[' or '{' or '(':
        size = b64_to_uint(raw)
        pos += 1       // skip opener
        pos += size    // jump past body
        pos += 1       // skip closer
    else:
        // no prefix, skip by recursive parsing
        pos = saved
        skip_value(input, pos)
```

Since `[`, `{`, `(` are not b64 digits, the parser always distinguishes a length prefix from an unprefixed container.

**Return is transparent**: when `;` (return) appears in a skip position, the encoder passes the skip flag through to the return's child. The `;` itself never gets a length prefix. Example: `return [1]` in a branch encodes as `;2[2+]` (the child array gets the prefix).

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
- `and`/`or` short-circuit on existence
- Type predicates return the value if it matches, `none` otherwise

### Gas-Bounded Execution

Gas is charged per loop/comprehension iteration. The host sets the limit; 0 = unlimited.

---

## Tag Summary

All RX tags (see [rx-format.md](rx-format.md)) plus:

| Tag     | Kind     | Varint meaning                | Body / children              |
|---------|----------|-------------------------------|------------------------------|
| `$`     | scalar   | name (opaque)                 | none                         |
| `%`     | scalar   | mnemonic (opaque)             | none                         |
| `@`     | scalar   | depth (unsigned)              | none                         |
| `\`     | scalar   | break/continue code           | none                         |
| `(`     | opener   | (length prefix in skip pos)   | values until `)`             |
| `)`     | closer   | --                            | --                           |
| `?`     | modifier | (empty)                       | compound: `?(` children `)`  |
| `!`     | modifier | (empty)                       | compound: `!(` children `)`  |
| `\|`    | modifier | (empty)                       | compound: `\|(` children `)` |
| `&`     | modifier | (empty)                       | compound: `&(` children `)`  |
| `>`     | modifier | (empty)                       | compound with `(`, `[`, `{`  |
| `<`     | modifier | (empty)                       | compound with `(`, `[`, `{`  |
| `#`     | modifier | (empty)                       | compound with `(`, `[`, `{`  |
| `=`     | fixed    | (empty)                       | 2 children                   |
| `/`     | fixed    | (empty)                       | 2 children                   |
| `~`     | fixed    | (empty)                       | 1 child                      |
| `;`     | prefix   | (empty)                       | 1 child (return value)       |

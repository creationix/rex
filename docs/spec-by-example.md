# Rex Language Spec by Example

A guided tour of every Rex language feature, doubling as the golden test
suite. Starts with the simplest values and builds toward full programs.

## Test Format

The test runner (`crates/rex-core/tests/spec.rs`) parses this file:

- `rex` — compile and run in a shared VM (state carries across tests)
- `json` — structural match against the last expression result
- `json vars` — structural match against all current variables
- `rext` — exact match against bytecode of previous rex block

Prose is ignored by the runner. Multiple blocks per section, interleaved freely.

---

# Data

## Integers

| rex     | rext   | json    |
|---------|--------|---------|
| `0`     | `+`    | `0`     |
| `-1`    | `1+`   | `-1`    |
| `1`     | `2+`   | `1`     |
| `42`    | `1k+`  | `42`    |
| `1234`  | `CA+`  | `1234`  |
| `-4321` | `271+` | `-4321` |

Hex and binary literals compile to plain integers:

| rex          | rext      | json         |
|--------------|-----------|--------------|
| `0xFF`       | `7-+`     | `255`        |
| `0b1010`     | `k+`      | `10`         |
| `0xdeadbeef` | `6ZmTTu+` | `3735928559` |

## Decimals

| rex      | rext    | json      |
|----------|---------|-----------|
| `3.14`   | `3*9Q+` | `3.14`    |
| `12e3`   | `6*o+`  | `12e3`    |
| `12e-34` | `13*o+` | `1.2e-33` |


## Strings

Single and double quotes produce identical values:

| rex             | rext          | json            |
|-----------------|---------------|-----------------|
| `"hello"`       | `5,hello`     | `"hello"`       |
| `'world'`       | `5,world`     | `"world"`       |
| `""`            | `,`           | `""`            |
| `"\"escaped\""` | `9,"escaped"` | `"\"escaped\""` |

## Booleans, Null, None

| rex     | rext  | result  |
|---------|-------|---------|
| `true`  | `t'`  | `true`  |
| `false` | `f'`  | `false` |
| `null`  | `n'`  | `null`  |
| `none`  | `no'` | `none`  |

## Special Numbers

| rex    | rext   |
|--------|--------|
| `inf`  | `inf'` |
| `-inf` | `nif'` |
| `nan`  | `nan'` |

## Comments

Comments are stripped during compilation — they produce no bytecode.

```rex
// line comment
42 /* block comment */
```

```json
42
```

---

# Containers

## Arrays

Commas are optional. Trailing commas allowed.

| rex         | rext       | json        |
|-------------|------------|-------------|
| `[]`        | `[]`       | `[]`        |
| `[1, 2, 3]` | `[2+4+6+]` | `[1, 2, 3]` |
| `[ 1 2 3 ]` | `[2+4+6+]` | `[1, 2, 3]` |

```rex
[1 [2] 3 [5] 5]
```

```rext
[2+[4+]6+[a+]a+]
```

```json
[1,[2],3,[5],5]
```

## Objects

Bare keys are strings. Commas optional. Trailing commas allowed.

| rex            | rext           | json            |
|----------------|----------------|-----------------|
| `{}`           | `{}`           | `{}`            |
| `{a: 1, b: 2}` | `{1,a2+1,b4+}` | `{"a":1,"b":2}` |

```rex
{name: "Rex", age: 65}
```

```json
{"name": "Rex", "age": 65}
```

## Computed Keys

Parentheses make the key an expression:

| rex         | rext         |
|-------------|--------------|
| `{name: 1}` | `{4,name2+}` |
| `{(x): 1}`  | `{x$2+}`     |

## Nested Containers

```rex
a = [1]
a = [2 a 2]
a = [3 a 3]
```

```json
[3,[2,[1],2],3]
```

## Template Literals

Backtick strings with `${expr}` interpolation. Compile to string chains:

```rex
name = "Rex"
```

```rex
`hello ${name}`
```

```rext
d.6,hello name$
```

```json
"hello Rex"
```

Tagged templates pass parts and values to a function:

| rex                     | rext                     |
|-------------------------|--------------------------|
| `` html`<p>${x}</p>` `` | `(html$[3,<p>4,</p>]x$)` |

## Ranges

Inclusive. Auto-descending when start > end.

```rex
1..5
```

```rext
(rn%2+a+)
```

```json
[1, 2, 3, 4, 5]
```

---

# Navigation

## Static Keys

Dots read nested values. Compiles to a call with string arguments:

| rex                   | rext                       |
|-----------------------|----------------------------|
| `user.name`           | `(user$4,name)`            |
| `user.address.street` | `(user$7,address6,street)` |

## Dynamic Keys

`.()` navigates with an expression:

| rex           | rext              |
|---------------|-------------------|
| `map.(x + 1)` | `(map$(ad%x$2+))` |

---

# Variables and Assignment

## Assignment

`=` binds a value and returns it:

```rex
x = 42
```

```rext
=x$1k+
```

```json
42
```

```json vars
{"x": 42}
```

## Compound Assignment

Desugars to `x = op(x, value)`:

| rex      | rext           |
|----------|----------------|
| `x += 1` | `=x$(ad%x$2+)` |

```rex
x = 10
x += 5
```

```json
15
```

## Compound Expressions

Semicolons group expressions. Evaluates left to right, returns last:

```rex
a = 1; b = 2; a + b
```

```rext
(%=a$2+=b$4+(ad%a$b$))
```

```json
3
```

---

# Arithmetic

| rex      | rext        | json  |
|----------|-------------|-------|
| `1 + 2`  | `(ad%2+4+)` | `3`   |
| `10 - 3` | `(sb%k+6+)` | `7`   |
| `4 * 5`  | `(ml%8+a+)` | `20`  |
| `7 / 2`  | `(dv%e+4+)` | `3.5` |
| `10 % 3` | `(md%k+6+)` | `1`   |
| `-x`     | `(ng%x$)`   |       |

```rex
[1 + 2, 10 - 3, 4 * 5, 7 / 2, 10 % 3]
```

```json
[3, 7, 20, 3.5, 1]
```

String concatenation uses `+`:

```rex
"hello" + " world"
```

```rext
(ad%5,hello6, world)
```

```json
"hello world"
```

---

# Comparison

Comparisons return the **left-hand value** on success, `none` on failure:

| rex      | json   |
|----------|--------|
| `3 > 2`  | `3`    |
| `3 > 5`  | `none` |
| `3 == 3` | `3`    |
| `3 != 3` | `none` |

```rex
[3 > 2, 3 > 5]
```

```json
[3, null]
```

All comparison opcodes:

| rex      | rext        |
|----------|-------------|
| `x == 1` | `(eq%x$2+)` |
| `x != 1` | `(nq%x$2+)` |
| `x > 1`  | `(gt%x$2+)` |
| `x >= 1` | `(ge%x$2+)` |
| `x < 1`  | `(lt%x$2+)` |
| `x <= 1` | `(le%x$2+)` |

---

# Bitwise and Boolean Operators

Symbol operators (`&`, `|`, `^`, `~`) operate on **values** — bitwise for numbers, logical for booleans:

| rex     | rext        | result  |
|---------|-------------|---------|
| `5 & 3` | `(an%a+6+)` | `1`     |
| `~5`    | `(nt%a+)`   | `-6`    |
| `~true` | `(nt%t')`   | `false` |

```rex
[5 & 3, ~5, ~true]
```

```json
[1, -6, false]
```

---

# Existence Logic

`and` and `or` short-circuit on **existence**, not truthiness. Only `none` is absence — `false`, `null`, `0`, `""` are all real values.

## `or` — first defined value

| rex                   | rext                | result       |
|-----------------------|---------------------|--------------|
| `none or "fallback"`  | `\|(no'8,fallback)` | `"fallback"` |
| `0 or "fallback"`     | `\|(+8,fallback)`   | `0`          |
| `false or "fallback"` | `\|(f'8,fallback)`  | `false`      |

```rex
[none or "fallback", 0 or "fallback", false or "fallback"]
```

```json
["fallback", 0, false]
```

## `and` — last value if all defined

| rex          | rext       | json   |
|--------------|------------|--------|
| `1 and 2`    | `&(2+4+)`  | `2`    |
| `none and 2` | `&(no'4+)` | `none` |

```rex
[1 and 2, none and 2]
```

```json
[2, null]
```

---

# Control Flow

## `when` / `else`

Branch on existence:

```rex
x = 10
when x > 5 do "big" else "small" end
```

```json
"big"
```

Chained conditions:

```rex
when x > 100 do "huge" else when x > 5 do "big" else "small" end
```

```rext
?((gt%x$38+)4,huge7(gt%x$a+)3,big5,small)
```

```json
"big"
```

## `unless`

Compiles to `when` with swapped branches:

```rex
y = none
```

```rex
unless y do "absent" end
```

```rext
?(y$no'6,absent)
```

```json
"absent"
```

## Binding in Conditions

`=` in a condition binds the value and tests existence:

```rex
when val = 10 do val + 1 end
```

```rext
?(=val$k+9(ad%val$2+))
```

```json
11
```

## `return`

Halts execution and produces a value:

| rex         | rext   |
|-------------|--------|
| `return 42` | `;1k+` |

## `delete`

Removes a key from an object:

| rex              | rext           |
|------------------|----------------|
| `delete obj.key` | `~(obj$3,key)` |

---

# Iteration

## `for` Loops

Values, key-value pairs, or keys only:

```rex
for v in [10, 20, 30] do v end
```

```rext
>([k+E+Y+]v$v$)
```

```json
30
```

```rex
for k of {a: 1, b: 2} do k end
```

```rext
<({1,a2+1,b4+}k$k$)
```

```json
"b"
```

## `while` Loops

```rex
x = 0
while x < 3 do x += 1 end
```

```json
3
```

## `break` / `continue`

| rex        | rext |
|------------|------|
| `break`    | `\`  |
| `continue` | `1\` |

```rex
for v in 1..10 do when v == 3 do break end; v end
```

```json
2
```

---

# Comprehensions

Body first, then iteration. `none` results are automatically excluded.

## Array Comprehensions

```rex
[v * 2 for v in 1..3]
```

```rext
>[(rn%2+6+)v$(ml%v$4+)]
```

```json
[2, 4, 6]
```

Filtering with `and` — `none` results are excluded:

```rex
[v >= 10 and v for v in [5, 15, 3, 20]]
```

```json
[15, 20]
```

## Object Comprehensions

```rex
{(k): v * 10 for k, v in {a: 1, b: 2}}
```

```json
{"a": 10, "b": 20}
```

## `while` Comprehensions

Collect values until condition fails:

```rex
x = 1
[x = x * 2 while x < 100]
```

```json
[2, 4, 8, 16, 32, 64, 128]
```

---

# Type Predicates

Return the value if it matches the type, `none` otherwise:

| rex               | rext         | json   |
|-------------------|--------------|--------|
| `isString("hi")`  | `(st%2,hi)`  | `"hi"` |
| `isString(42)`    | `(st%1k+)`   | `none` |
| `isNumber(3.14)`  | `(nm%3*9Q+)` | `3.14` |
| `isInteger(42)`   | `(ig%1k+)`   |        |
| `isBoolean(true)` | `(bt%t')`    | `true` |
| `isArray([])`     | `(ar%[])`    | `[]`   |
| `isObject({})`    | `(ob%{})`    | `{}`   |

```rex
[isString("hi"), isString(42)]
```

```json
["hi", null]
```

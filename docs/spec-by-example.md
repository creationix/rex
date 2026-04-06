# Rex Language Spec by Example

A guided tour of every Rex language feature, doubling as the golden test
suite. Starts with the simplest values and builds toward full programs.

## How To Run

Run this spec suite from the repo root:

```sh
cargo test -p rex-core --test spec
```

```sh
cargo test -p rex-cli hover_ -- --nocapture
```

## Test Format

The test runner (`crates/rex-core/tests/spec.rs`) parses this file:

- `rex` — compile and run in a shared VM (state carries across tests)
- `json` — structural match against the last expression result
- `json vars` — structural match against all current variables
- `json types` — structural match against inferred type spans from the last `rex` block
- `csv types` — exact CSV snapshot of inferred type spans from the last `rex` block
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

```rext
1k+
```

```json
42
```

---

# Containers

## Arrays

Commas are optional. Trailing commas allowed.

| rex           | rext       | json        |
|---------------|------------|-------------|
| `[]`          | `[]`       | `[]`        |
| `[ 1, 2, 3 ]` | `[2+4+6+]` | `[1, 2, 3]` |
| `[ 1 2 3 ]`   | `[2+4+6+]` | `[1, 2, 3]` |

```rex
[ 1 [ 2 ] 3 [ 5 ] 5 ]
```

```rext
[2+[4+]6+[a+]a+]
```

```json
[1,[2],3,[5],5]
```

## Objects

Bare keys are strings. Commas optional. Trailing commas allowed.

| rex           | rext           | json            |
|---------------|----------------|-----------------|
| `{}`          | `{}`           | `{}`            |
| `{ a:1 b:2 }` | `{1,a2+1,b4+}` | `{"a":1,"b":2}` |

```rex
{ name:"Rex" age:65 }
```

```rext
{4,name3,Rex3,age22+}
```

```json
{"name": "Rex", "age": 65}
```

## Computed Keys

Parentheses make the key an expression:

| rex          | rext         |
|--------------|--------------|
| `{ name:1 }` | `{4,name2+}` |
| `{ (x):1 }`  | `{x$2+}`     |

```rex
extern db: { *: { *: str } }
extern key: str
res = db.(key + ".html").prop
```

```csv types
text                   , type             , line, col
db                     , { *: { *: str } }, 1   , 8
str                    , str              , 1   , 22
key                    , str              , 2   , 8
str                    , str              , 2   , 13
res                    , str | none       , 3   , 1
db                     , { *: { *: str } }, 3   , 7
"db.(key + "".html"").prop", str | none       , 3   , 7
key                    , str              , 3   , 11
""".html"""            , str              , 3   , 17
```

```rext
=res$(db$(ad%key$5,.html)4,prop)
```

## Spread

`...expr` inside arrays and objects splices the contents of `expr` into the
container. Compiles to a chain (`.`) of segments:

| rex             | rext        |
|-----------------|-------------|
| `[ ...a 42 ]`   | `7.a$[1k+]` |
| `[ ...a ...b ]` | `4.a$b$`    |

In objects, `key: none` removes a key — combined with spread, this lets you
clone-and-modify:

| rex                      | rext                   |
|--------------------------|------------------------|
| `{ ...v id:none }`       | `b.v${2,idno'}`        |
| `{ ...base name:"new" }` | `i.base${4,name3,new}` |

## Indexed Containers

`#` after the opener adds a pointer table for O(1) array access or O(log n)
object key lookup. Same values, different bytecode encoding:

| rex            | rext               |
|----------------|--------------------|
| `[# 1 2 3 ]`   | `[o#0242+4+6+]`    |
| `{# a:1 b:2 }` | `{g#051,a2+1,b4+}` |
| `{# b:1 a:2 }` | `{g#501,b2+1,a4+}` |

Indexed form also meand that any code inside that has side-effects will be lazily evaluated (otherwise we couldn't support random-access and lazy parsing)

Without `#`, arrays and objects have no index — access is O(n):

| rex           | rext           |
|---------------|----------------|
| `[ 1 2 3 ]`   | `[2+4+6+]`     |
| `{ a:1 b:2 }` | `{1,a2+1,b4+}` |

## Forming Nested Containers

```rex
a = [ 1 ]
a = [ 2 a 2 ]
a = [ 3 a 3 ]
```

```csv types
text, type                 , line, col
a   , [int]                , 1   , 1
1   , int                  , 1   , 7
a   , [int | [int]]        , 2   , 1
2   , int                  , 2   , 7
a   , [int]                , 2   , 9
2   , int                  , 2   , 11
a   , [int | [int | [int]]], 3   , 1
3   , int                  , 3   , 7
a   , [int | [int]]        , 3   , 9
3   , int                  , 3   , 11
```

```rext
(%=a$[2+]=a$[4+a$4+]=a$[6+a$6+])
```

```json
[3,[2,[1],2],3]
```

## Inline Expanded

```rex
a = [ 1 ]
a = [ 2 ...a 2 ]
a = [ 3 ...a 3 ]
```

```csv types
text, type , line, col
a   , [int], 1   , 1
1   , int  , 1   , 7
a   , [int], 2   , 1
2   , int  , 2   , 7
a   , [int], 2   , 12
2   , int  , 2   , 14
a   , [int], 3   , 1
3   , int  , 3   , 7
a   , [int], 3   , 12
3   , int  , 3   , 14
```

```rext
(%=a$[2+]=a$8.2^a$[4+]=a$8.2^a$[6+])
```

```json
[3,2,1,2,3]
```

## Object Nesting

```rex
a = { c:3 x:4 }
a = { b:1 _:a y:5 }
a = { a:1 _:a z:6 }
```

```csv types
text, type                                                       , line, col
a   , { c: int x: int }                                          , 1   , 1
c   , int                                                        , 1   , 7
3   , int                                                        , 1   , 9
x   , int                                                        , 1   , 11
4   , int                                                        , 1   , 13
a   , { b: int _: { c: int x: int } y: int }                     , 2   , 1
b   , int                                                        , 2   , 7
1   , int                                                        , 2   , 9
_   , { c: int x: int }                                          , 2   , 11
a   , { c: int x: int }                                          , 2   , 13
y   , int                                                        , 2   , 15
5   , int                                                        , 2   , 17
a   , { a: int _: { b: int _: { c: int x: int } y: int } z: int }, 3   , 1
a   , int                                                        , 3   , 7
1   , int                                                        , 3   , 9
_   , { b: int _: { c: int x: int } y: int }                     , 3   , 11
a   , { b: int _: { c: int x: int } y: int }                     , 3   , 13
z   , int                                                        , 3   , 15
6   , int                                                        , 3   , 17
```

```rext
(%=a${1,c6+1,x8+}=a${1,b2+1,_a$1,ya+}=a${1,a2+1,_a$1,zc+})
```

```json
{"a":1,"_":{"b":1,"_":{"c":3,"x":4},"y":5},"z":6}
```

## Spread Objects

```rex
a = { c:3 x:4 }
a = { b:2 ...a y:5 }
a = { a:1 ...a z:6 }
```

```csv types
text, type                                         , line, col
a   , { c: int x: int }                            , 1   , 1
c   , int                                          , 1   , 7
3   , int                                          , 1   , 9
x   , int                                          , 1   , 11
4   , int                                          , 1   , 13
a   , { b: int c: int x: int y: int }              , 2   , 1
b   , int                                          , 2   , 7
2   , int                                          , 2   , 9
a   , { c: int x: int }                            , 2   , 14
y   , int                                          , 2   , 16
5   , int                                          , 2   , 18
a   , { a: int b: int c: int x: int y: int z: int }, 3   , 1
a   , int                                          , 3   , 7
1   , int                                          , 3   , 9
a   , { b: int c: int x: int y: int }              , 3   , 14
z   , int                                          , 3   , 16
6   , int                                          , 3   , 18
```

```rext
(%=a${1,c6+1,x8+}=a$g.{1,b4+}a${1,ya+}=a$g.{1,a2+}a${1,zc+})
```

```json
{"a":1,"b":2,"c":3,"x":4,"y":5,"z":6}
```


## Template Literals

Backtick strings with `${expr}` interpolation. Without a tag, they compile to
string chains (`.`) that concatenate segments:

| rex                   | rext              |
|-----------------------|-------------------|
| `` `hello` ``         | `5,hello`         |
| `` `hello ${name}` `` | `d.6,hello name$` |
| `` `${x}` ``          | `2.x$`            |
| `` `${a} and ${b}` `` | `b.a$5, and b$`   |

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

### Tagged Templates

A tag before the backtick turns the template into a function call. The tag
receives a string array and the interpolated values as separate arguments.

The string array follows the **sandwich rule**: it always has one more element
than the number of interpolations, with empty strings where no static text
appears. This lets the tag function zip strings and values without bounds checks.

```rex
html`<p>${x}</p>`
```

```rext
(html$[3,<p>4,</p>]x$)
```

```rex
html`${a}${b}`
```

```rext
(html$[,,,]a$b$)
```

```rex
html`${x}`
```

```rext
(html$[,,]x$)
```

### Shortcodes

Domain files (`.rexd`) can assign explicit shortcodes to extern declarations
with a string after `extern`. By convention, host shortcodes use initial caps
to avoid conflicts with built-in opcodes (which are lowercase).

Functions compile to opcodes (`%`), bindings compile to refs (`'`):

| rex                     | rexd                                            | rext                  |
|-------------------------|-------------------------------------------------|-----------------------|
| `json.parse(text)`      | `extern "Jp" json.parse(t: string) -> some`     | `(Jp%text$)`          |
| `json.stringify(v)`     | `extern "Js" json.stringify(v: some) -> string` | `(Js%v$)`             |
| `math.floor(n)`         | `extern "Mf" math.floor(n: number) -> integer`  | `(Mf%n$)`             |
| `env.name`              | `extern "E" env = {name: string}`               | `(E'4,name)`          |
| `` html`<p>${x}</p>` `` | `extern "H" html(p: [string]) -> string`        | `(H%[3,<p>4,</p>]x$)` |

Without a shortcode, identifiers stay as variables:

| rex                | rext                    |
|--------------------|-------------------------|
| `json.parse(text)` | `((json$5,parse)text$)` |
| `env.name`         | `(env$4,name)`          |

## Ranges

Inclusive. Auto-descending when start > end.

```rex
1 .. 5
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

Dots read nested values. Compiles to a navigation/call with string arguments:

| rex                   | rext                       |
|-----------------------|----------------------------|
| `user.name`           | `(user$4,name)`            |
| `user.address.street` | `(user$7,address6,street)` |

## Dynamic Keys

`.()` navigates with an expression:

| rex           | rext              |
|---------------|-------------------|
| `map.(x + 1)` | `(map$(ad%x$2+))` |

### Exact Automatic Types

```rex
db = {
  bob:{ name:"Bob" color:0x44ff44 }
  tim:{ name:"Tim" color:0x0088ff }
}

first-name = db.bob
tim-color = db.tim.color
```

```csv types
text        , type                                                           , line, col
db          , { bob: { name: str color: int } tim: { name: str color: int } }, 1   , 1
bob         , { name: str color: int }                                       , 2   , 3
name        , str                                                            , 2   , 9
"""Bob"""   , str                                                            , 2   , 14
color       , int                                                            , 2   , 20
0x44ff44    , int                                                            , 2   , 26
tim         , { name: str color: int }                                       , 3   , 3
name        , str                                                            , 3   , 9
"""Tim"""   , str                                                            , 3   , 14
color       , int                                                            , 3   , 20
0x0088ff    , int                                                            , 3   , 26
first-name  , { name: str color: int }                                       , 6   , 1
db          , { bob: { name: str color: int } tim: { name: str color: int } }, 6   , 14
db.bob      , { name: str color: int }                                       , 6   , 14
tim-color   , int                                                            , 7   , 1
db          , { bob: { name: str color: int } tim: { name: str color: int } }, 7   , 13
db.tim      , { name: str color: int }                                       , 7   , 13
db.tim.color, int                                                            , 7   , 13
```

```rext
(%=db${Q^{d^3,BobyvW8+}V^{4,name3,TimM^h7-+}}=first-name$(db$3,bob)=tim-color$(db$3,tim5,color))
```

```json
35071
```

### More Generic types

```rex
db: { *: { name: str color: int } } = {
  bob:{ name:"Bob" color:0x44ff44 }
  tim:{ name:"Tim" color:0x0088ff }
}

first-name = db.bob
tim-color = db.tim.color
```

```csv types
text        , type                           , line, col
db          , { *: { name: str color: int } }, 1   , 1
name        , str                            , 1   , 12
str         , str                            , 1   , 18
color       , int                            , 1   , 22
int         , int                            , 1   , 29
bob         , { name: str color: int }       , 2   , 3
name        , str                            , 2   , 9
"""Bob"""   , str                            , 2   , 14
color       , int                            , 2   , 20
0x44ff44    , int                            , 2   , 26
tim         , { name: str color: int }       , 3   , 3
name        , str                            , 3   , 9
"""Tim"""   , str                            , 3   , 14
color       , int                            , 3   , 20
0x0088ff    , int                            , 3   , 26
first-name  , { name: str color: int } | none, 6   , 1
db          , { *: { name: str color: int } }, 6   , 14
db.bob      , { name: str color: int } | none, 6   , 14
tim-color   , int | none                     , 7   , 1
db          , { *: { name: str color: int } }, 7   , 13
db.tim      , { name: str color: int } | none, 7   , 13
db.tim.color, int | none                     , 7   , 13
```

```rext
(%=db${Q^{d^3,BobyvW8+}V^{4,name3,TimM^h7-+}}=first-name$(db$3,bob)=tim-color$(db$3,tim5,color))
```

```json
35071
```

### Heper Types

```rex
type Person = { name: str color: int }

db: { bob: Person tim: Person } = {
  bob:{ name:"Bob" color:0x44ff44 }
  tim:{ name:"Tim" color:0x0088ff }
}

first-name = db.bob

tim-color = db.tim.color

[ first-name tim-color ]
```

```csv types
text        , type                                                           , line, col
Person      , { name: str color: int }                                       , 1   , 6
name        , str                                                            , 1   , 17
str         , str                                                            , 1   , 23
color       , int                                                            , 1   , 27
int         , int                                                            , 1   , 34
db          , { bob: { name: str color: int } tim: { name: str color: int } }, 3   , 1
bob         , { name: str color: int }                                       , 3   , 7
Person      , { name: str color: int }                                       , 3   , 12
tim         , { name: str color: int }                                       , 3   , 19
Person      , { name: str color: int }                                       , 3   , 24
bob         , { name: str color: int }                                       , 4   , 3
name        , str                                                            , 4   , 9
"""Bob"""   , str                                                            , 4   , 14
color       , int                                                            , 4   , 20
0x44ff44    , int                                                            , 4   , 26
tim         , { name: str color: int }                                       , 5   , 3
name        , str                                                            , 5   , 9
"""Tim"""   , str                                                            , 5   , 14
color       , int                                                            , 5   , 20
0x0088ff    , int                                                            , 5   , 26
first-name  , { name: str color: int }                                       , 8   , 1
db          , { bob: { name: str color: int } tim: { name: str color: int } }, 8   , 14
db.bob      , { name: str color: int }                                       , 8   , 14
tim-color   , int                                                            , 10  , 1
db          , { bob: { name: str color: int } tim: { name: str color: int } }, 10  , 13
db.tim      , { name: str color: int }                                       , 10  , 13
db.tim.color, int                                                            , 10  , 13
first-name  , { name: str color: int }                                       , 12  , 3
tim-color   , int                                                            , 12  , 14
```

```rext
(%=db${Q^{d^3,BobyvW8+}V^{4,name3,TimM^h7-+}}=first-name$(db$3,bob)=tim-color$(db$3,tim5,color)[first-name$tim-color$])
```

```json
[{"name":"Bob","color":4521796},35071]
```

---

# Variables and Assignment

## Assignment

`=` binds a value and returns it:

```rex
x = 42
```

```csv types
text, type, line, col
x   , int , 1   , 1
42  , int , 1   , 5
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

Type annotations are used by the typechecker but don't affect the bytecode:

```rex
scores: { *: int } = { alice:95 bob:42 }
```

```json
{"alice": 95, "bob": 42}
```

## Swap Assignment

`:=` assigns a new value and returns the previous one:

| rex       | rext     |
|-----------|----------|
| `x := 99` | `/x$36+` |

```rex
x = 42
x := 99
```

```csv types
text, type, line, col
x   , int , 1   , 1
42  , int , 1   , 5
x   , int , 2   , 1
99  , int , 2   , 6
```

```json
42
```

```json vars
{"x": 99}
```

C-style `i++` — swap-set in a while comprehension collects pre-increment values:

```rex
i = 0
[ i := i + 1 while i < 5 ]
```

```csv types
text, type, line, col
i   , int , 1   , 1
0   , int , 1   , 5
i   , int , 2   , 3
i   , int , 2   , 8
1   , int , 2   , 12
i   , int , 2   , 20
5   , int , 2   , 24
```

```rext
(%=i$+#[(lt%i$a+)/i$(ad%i$2+)])
```

```json
[0, 1, 2, 3, 4]
```

## Compound Assignment

Desugars to `x = op(x, value)`:

| rex       | rext           |
|-----------|----------------|
| `x += 1`  | `=x$(ad%x$2+)` |
| `x -= 1`  | `=x$(sb%x$2+)` |
| `x *= 2`  | `=x$(ml%x$4+)` |
| `x /= 2`  | `=x$(dv%x$4+)` |
| `x %= 3`  | `=x$(md%x$6+)` |
| `x &= 3`  | `=x$(an%x$6+)` |
| `x \|= 3` | `=x$(or%x$6+)` |
| `x ^= 3`  | `=x$(xr%x$6+)` |

```rex
x = 10
x += 5
```

```csv types
text, type, line, col
x   , int , 1   , 1
10  , int , 1   , 5
x   , int , 2   , 1
5   , int , 2   , 6
```

```rext
(%=x$k+=x$(ad%x$a+))
```

```json
15
```

## Compound Expressions

Some positions are **blocks** — they hold multiple expressions naturally (program
level, `do...end` bodies, comprehension bodies). Other positions are
**expression slots** — conditions, array elements, assignment right-hand sides.
Semicolons let you pack multiple expressions into a single expression slot,
like C's comma operator. Evaluates left to right, returns last:

```rex
// multiple expressions as a single expression (uses last)
all = (a = 1; b = 2; a + b)
```

```csv types
text, type, line, col
all , int , 2   , 1
a   , int , 2   , 8
1   , int , 2   , 12
b   , int , 2   , 15
2   , int , 2   , 19
a   , int , 2   , 22
b   , int , 2   , 26
```

```rext
=all$(%=a$2+=b$4+(ad%a$b$))
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
[ 1 + 2, 10 - 3, 4 * 5, 7 / 2, 10 % 3 ]
```

```csv types
text, type, line, col
1   , int , 1   , 3
2   , int , 1   , 7
10  , int , 1   , 10
3   , int , 1   , 15
4   , int , 1   , 18
5   , int , 1   , 22
7   , int , 1   , 25
2   , int , 1   , 29
10  , int , 1   , 32
3   , int , 1   , 37
```

```rext
[(ad%2+4+)(sb%k+6+)(ml%8+a+)(dv%e+4+)(md%k+6+)]
```

```json
[3, 7, 20, 3.5, 1]
```

String concatenation uses `+`:

```rex
"hello" + " world"
```

```json types
[
  {"text":"\"hello\"", "type":"str","line":1,"col":1},
  {"text":"\" world\"","type":"str","line":1,"col":11}
]
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
c = {
  a:3 > 2
  b:3 > 5
}
```

```csv types
text, type                           , line, col
c   , { a: int | none b: int | none }, 1   , 1
a   , int | none                     , 2   , 3
3   , int                            , 2   , 5
2   , int                            , 2   , 9
b   , int | none                     , 3   , 3
3   , int                            , 3   , 5
5   , int                            , 3   , 9
```

```rext
=c${1,a(gt%6+4+)1,b(gt%6+a+)}
```

```json
{"a":3}
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
d = [ a = 5 & 3, b = ~5, c = ~true ]
```

```csv types
text, type        , line, col
d   , [int | bool], 1   , 1
a   , int         , 1   , 7
5   , int         , 1   , 11
3   , int         , 1   , 15
b   , int         , 1   , 18
5   , int         , 1   , 23
c   , bool        , 1   , 26
true, bool        , 1   , 31
```

```rext
=d$[=a$(an%a+6+)=b$(nt%a+)=c$(nt%t')]
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
[ none or "fallback", 0 or "fallback", false or "fallback" ]
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
[ 1 and 2, none and 2 ]
```

```json
[2, null]
```

```rex
b = true
n = null
ban = b and n
i = 42
d = 1.23
iad = i and d
banaiad = ban and iad
```

```csv types
text   , type       , line, col
b      , bool       , 1   , 1  
true   , bool       , 1   , 5  
n      , null       , 2   , 1  
null   , null       , 2   , 5  
ban    , null       , 3   , 1  
b      , bool       , 3   , 7  
n      , null       , 3   , 13 
i      , int        , 4   , 1  
42     , int        , 4   , 5  
d      , num        , 5   , 1  
1.23   , num        , 5   , 5  
iad    , num        , 6   , 1  
i      , int        , 6   , 7  
d      , num        , 6   , 13 
banaiad, num        , 7   , 1  
ban    , null       , 7   , 11 
iad    , num        , 7   , 19 
```

```rext
(%=b$t'=n$n'=ban$&(b$n$)=i$1k+=d$3*3S+=iad$&(i$d$)=banaiad$&(ban$iad$))
```

```json
1.23
```

---

# Control Flow

## `when` / `else`

Branch on existence:

```rex
x = 10
```

```rex
extern x: int
a = when c = x > 5 do
  b = "big"
else
  c = "small"
end
```

```csv types
text   , type      , line, col
x      , int       , 1   , 8
int    , int       , 1   , 11
a      , str       , 2   , 1
c      , int | none, 2   , 10
x      , int       , 2   , 14
5      , int       , 2   , 18
b      , str       , 3   , 3
"""big""", str       , 3   , 7
c      , str       , 5   , 3
"""small""", str       , 5   , 7
```

```rext
=a$?(=c$(gt%x$a+)=b$3,big=c$5,small)
```

```json
"big"
```

Chained conditions:

```rex
when x > 100 do
  "huge"
else when x > 5 do
  "big"
else
  "small"
end
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

```rex
res = when c = 1 > 2 do
  return "impossible"
  42
else
  return "likely"
  56
end
```

```csv types
text        , type      , line, col
res         , never     , 1   , 1
c           , int | none, 1   , 12
1           , int       , 1   , 16
2           , int       , 1   , 20
"""impossible""", str       , 2   , 10
42          , never     , 3   , 3
"""likely""", str       , 5   , 10
56          , never     , 6   , 3
```

```rext
=res$?(=c$(gt%2+4+)h(%;a,impossible1k+)d(%;6,likely1M+))
```

```json
"likely"
```

## `delete`

Removes a key from an object:

| rex              | rext           |
|------------------|----------------|
| `delete obj.key` | `~(obj$3,key)` |

```rex
obj = { a:1 b:2 c:3 }
delete obj.b
obj
```

```csv types
text , type                    , line, col
obj  , { a: int b: int c: int }, 1   , 1
a    , int                     , 1   , 9
1    , int                     , 1   , 11
b    , int                     , 1   , 13
2    , int                     , 1   , 15
c    , int                     , 1   , 17
3    , int                     , 1   , 19
obj  , { a: int b: int c: int }, 2   , 8
obj.b, int                     , 2   , 8
obj  , { a: int c: int }       , 3   , 1
```

```rext
(%=obj${1,a2+1,b4+1,c6+}~(obj$1,b)obj$)
```

```json
{"a":1,"c":3}
```

---

# Iteration

## `for` Loops

Values, key-value pairs, or keys only:

```rex
for v in [ 10, 20, 30 ] do v end
```

```rext
>([k+E+Y+]v$v$)
```

```json
30
```

```rex
for k of { a:1 b:2 } do k end
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
for v in 1 .. 10 do when v == 3 do break end; v end
```

```json
2
```

---

# Comprehensions

Body first, then iteration. `none` results are automatically excluded.

## Array Comprehensions

```rex
[ v * 2 for v in 1 .. 3 ]
```

```rext
>[(rn%2+6+)v$(ml%v$4+)]
```

```json
[2, 4, 6]
```

Filtering with `and` — `none` results are excluded:

```rex
[ v >= 10 and v for v in [ 5 15 3 20 ] ]
```

```json
[15, 20]
```

## Object Comprehensions

```rex
{ (k):v * 10 for k v in { a:1 b:2 } }
```

```json
{"a": 10, "b": 20}
```

## `while` Comprehensions

Collect values until condition fails:

```rex
x = 1
[ x = x * 2 while x < 100 ]
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
[ isString("hi") isString(42) ]
```

```json
["hi", null]
```

---

# Host Environment

Rex programs run inside a host that provides extern bindings and functions.
The examples below use this sample domain:

```rexd
extern "H" html(parts: [string], ...values: some) -> string
extern "Jp" json.parse(text: string) -> some
extern "Js" json.stringify(value: some) -> string
extern "Mf" math.floor(n: number) -> integer
extern "E" env = { name: string version: string }
```

Convention: host shortcodes use initial caps (`Jp`, `Mf`) to avoid conflicts
with built-in opcodes (`ad`, `gt`).

## `json.parse` / `json.stringify`

```rex
json.parse('{"a":1,"b":2}')
```

```rext
(Jp%d,{"a":1,"b":2})
```

```json
{"a": 1, "b": 2}
```

```rex
json.stringify([ 1 2 3 ])
```

```rext
(Js%[2+4+6+])
```

```json
"[1,2,3]"
```

## `math.floor`

```rex
math.floor(3.7)
```

```rext
(Mf%1*1a+)
```

```json
3
```

## `env` binding

```rex
env.name
```

```rext
(E'4,name)
```

```json
"Rex"
```

## `html` tagged template

Escapes interpolated values for safe HTML (`&`, `<`, `>`, `"`, `'`):

```rex
html`<p>${"safe & <sound>"}</p>`
```

```rext
(H%[3,<p>4,</p>]e,safe & <sound>)
```

```json
"<p>safe &amp; &lt;sound&gt;</p>"
```

```rex
html`<a href="${"x\"y"}">${"<b>bold</b>"}</a>`
```

```rext
(H%[9,<a href="2,">4,</a>]3,x"yb,<b>bold</b>)
```

```json
"<a href=\"x&quot;y\">&lt;b&gt;bold&lt;/b&gt;</a>"
```

## Large Examples

These are more realistic examples that exercise multiple features at once.

### Website Manifest

A flat map of URL paths to page metadata. The top-level object is indexed so
HTTP request routing can do sparse lookups with minimal parsing overhead. In
production these manifests can have hundreds of thousands of entries.

```rex
{#
  "/":{
    title:"Home"
    template:"landing"
    scripts:[ "analytics" ]
  }
  "/about":{
    title:"About Us"
    template:"page"
  }
  "/blog":{
    title:"Blog"
    template:"listing"
    children:"/blog/*"
  }
  "/blog/hello-world":{
    title:"Hello World"
    template:"post"
    author:"alice"
    published:"2026-01-15"
  }
  "/api/v1/users":{
    template:"json-api"
    methods:[ "GET" "POST" ]
    auth:true
  }
}
```

```rext
{F#001g0M3n201,/{2d^4,Home3n^7,landing7,scripts[9,analytics]}6,/about{1o^8,About Us2u^4,page}5,/blog{W^4,Blog24^7,listing8,children7,/blog/*}h,/blog/hello-world{5,titleb,Hello WorldZ^4,post6,author5,alice9,publisheda,2026-01-15}d,/api/v1/users{8,template8,json-api7,methods[3,GET4,POST]4,autht'}}
```

### Permission Matrix

Schema-shared objects shine when many records share the same keys — the encoder
stores the key list once and subsequent objects reference it with a pointer.

```rex
roles = {#
  admin:{ read:true write:true remove:true manage:true }
  editor:{ read:true write:true remove:false manage:false }
  viewer:{ read:true write:false remove:false manage:false }
  guest:{ read:false write:false remove:false manage:false }
}
```

```rext
=roles${w#0XjD5,admin{U^t't't't'}6,editor{A^t't'f'f'}6,viewer{g^t'f'f'f'}5,guest{4,readf'5,writef'6,removef'6,managef'}}
```

```json
{"admin":{"read":true,"write":true,"remove":true,"manage":true},"editor":{"read":true,"write":true,"remove":false,"manage":false},"viewer":{"read":true,"write":false,"remove":false,"manage":false},"guest":{"read":false,"write":false,"remove":false,"manage":false}}
```

Then we can read a single item.

```rex
roles.viewer.read
```

```rext
(roles$6,viewer4,read)
```

```json
true
```

### Config with Computed Defaults

Variables and expressions make config DRY. Shared values are defined once,
derived values are computed inline.

```rex
// These can be overridden by seeding the interpreter state
app: str = app or "myapp"
port: int = port or 8080
host: str = host or "0.0.0.0"
{
  name:app
  listen:`${host}:${port}`
  database:{
    url:`postgres://localhost:5432/${app}`
    pool-size:10
    timeout:30
  }
  cache:{
    url:`redis://localhost:6379/0`
    ttl:300
  }
  cors:{
    origins:[ `http://localhost:${port}` "https://myapp.com" ]
    methods:[ "GET" "POST" "PUT" "DELETE" ]
  }
}
```

```rext
(%=app$|(app$5,myapp)=port$|(port$3Yw+)=host$|(host$7,0.0.0.0){4,nameapp$6,listend.host$1,:port$8,database{13^w.q,postgres://localhost:5432/app$9,pool-sizek+7,timeoutY+}5,cache{3,urlo,redis://localhost:6379/03,ttl9o+}4,cors{7,origins[o.h,http://localhost:port$h,https://myapp.com]7,methods[3,GET4,POST3,PUT6,DELETE]}})
```

```json
{"name": "myapp", "listen": "0.0.0.0:8080", "database": {"url": "postgres://localhost:5432/myapp", "pool-size": 10, "timeout": 30}, "cache": {"url": "redis://localhost:6379/0", "ttl": 300}, "cors": {"origins": ["http://localhost:8080", "https://myapp.com"], "methods": ["GET", "POST", "PUT", "DELETE"]}}
```

### Lookup Table from List

Build an indexed lookup from a flat array. Comprehensions create the map.

```rex
users = [
  { id:1 name:"Alice" role:"admin" }
  { id:2 name:"Bob" role:"editor" }
  { id:3 name:"Carol" role:"viewer" }
]
{ (`${v.id}`):{ ...v id:none } for v in users }
```

```rext
(%=users$[{A^2+5,Alice5,admin}{g^4+3,Bob6,editor}{S^6+4,name5,Carol4,role6,viewer}]>{users$v$6.(v$6^)b.v${2,idno'}})
```

### Fibonacci via While Comprehension

Compute the Fibonacci sequence as a single expression. The while comprehension
collects each new term until it exceeds the limit.

```rex
a = 0; b = 1
[ c = a + b; a = b; b = c while a <= 100 ]
```

```rext
(%(%=a$+=b$2+)#[(le%a$38+)(%=c$(ad%a$b$)=a$b$=b$c$)])
```

```json
[1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144, 233]
```

### Filtering and Reshaping

Chain comprehensions to transform, filter, and reshape data. None-filtering
removes entries that don't match, no explicit `if` needed.

```rex
scores = {
  alice:95
  bob:42
  carol:78
  dave:31
  eve:88
}
{
  passed:{ (k):v >= 50 and v for k v in scores }
  honor-roll:[ v >= 85 and k for k v in scores ]
  average:a = 0; n = 0; for k v in scores do a += v; n += 1 end; a / n
}
```

```rext
(%=scores${5,alice2-+3,bob1k+5,carol2s+4,dave-+3,eve2M+}{6,passed>{scores$k$v$k$&((ge%v$1A+)v$)}a,honor-roll>[scores$k$v$&((ge%v$2G+)k$)]7,average(%=a$+=n$+>(scores$k$v$(%=a$(ad%a$v$)=n$(ad%n$2+)))(dv%a$n$))})
```


---

# Built-in Methods

## Array Methods

### push

Appends a value and returns the array.

```rex
a = [ 1, 2 ]
a.push(3)
```

```json
[1, 2, 3]
```

### pop

Removes and returns the last element.

```rex
a = [ 1, 2, 3 ]
a.pop()
```

```json
3
```

### join

Concatenates elements with a separator.

```rex
[ "a", "b", "c" ].join("-")
```

```json
"a-b-c"
```

### indexOf (array)

Returns the index of the first match, or none.

```rex
[ 10, 20, 30 ].indexOf(20)
```

```json
1
```

```rex
[ 10, 20, 30 ].indexOf(99)
```

```json
null
```

### contains (array)

Returns the value if found (existence-style), none otherwise.

```rex
[ 1, 2, 3 ].contains(2)
```

```json
2
```

```rex
[ 1, 2, 3 ].contains(9)
```

```json
null
```

### slice (array)

Returns a sub-array from start (inclusive) to end (exclusive).

```rex
[ 1, 2, 3, 4, 5 ].slice(1, 3)
```

```json
[2, 3]
```

## String Methods

### split

Splits a string by separator.

```rex
"a,b,c".split(",")
```

```json
["a", "b", "c"]
```

### trim

Strips leading and trailing whitespace.

```rex
"  hello  ".trim()
```

```json
"hello"
```

### indexOf (string)

Returns the character index of the first match, or none.

```rex
"hello world".indexOf("world")
```

```json
6
```

### contains (string)

Returns the substring if found, none otherwise.

```rex
"hello world".contains("world")
```

```json
"world"
```

```rex
"hello".contains("xyz")
```

```json
null
```

### starts-with

Returns the string if it starts with the prefix, none otherwise.

```rex
"hello".starts-with("hel")
```

```json
"hello"
```

```rex
"hello".starts-with("xyz")
```

```json
null
```

### ends-with

Returns the string if it ends with the suffix, none otherwise.

```rex
"hello".ends-with("llo")
```

```json
"hello"
```

### upper

```rex
"hello".upper()
```

```json
"HELLO"
```

### lower

```rex
"HELLO".lower()
```

```json
"hello"
```

### replace

Replaces the first occurrence.

```rex
"hello world".replace("world", "Rex")
```

```json
"hello Rex"
```

### slice (string)

Returns a substring from start to end (character indices).

```rex
"hello".slice(1, 3)
```

```json
"el"
```

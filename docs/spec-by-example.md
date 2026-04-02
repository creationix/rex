# Rex Language Spec by Example

This file is the golden test suite for the Rex language. Each test case
is defined by code blocks under a markdown header. The test runner
(`crates/rex-core/tests/spec.rs`) parses this file and runs each test.

## Format

- `rex` — source code to compile and run
- `rex input` — variables to inject before running
- `rexd` — domain config for compilation
- `rex output` — expected result value
- `rexc` — expected bytecode (optional, rarely needed)

Prose between blocks is ignored by the runner.

---

## Literals

### Integer

```rex
42
```

```json output
42
```

### Negative integer

```rex
-7
```

```json output
-7
```

### Zero

```rex
0
```

```json output
0
```

### Float

```rex
3.14
```

```rex output
3.14
```

### Hex literal

```rex
0xFF
```

```rex output
255
```

### Binary literal

```rex
0b1010
```

```json output
10
```

### Double-quoted string

```rex
"hello"
```

```json output
"hello"
```

### Single-quoted string

```rex
'hello'
```

```json output
"hello"
```

### Empty string

```rex
""
```

```json output
""
```

### True

```rex
true
```

```json output
true
```

### False

```rex
false
```

```json output
false
```

### Null

```rex
null
```

```json output
null
```

### None

```rex
none
```

```rex output
none
```

### Empty array

```rex
[]
```

```json output
[]
```

### Array with items

```rex
[ 1, 2, 3 ]
```

```json output
[ 1, 2, 3 ]
```

### Array without commas

```rex
[ 1 2 3 ]
```

```json output
[ 1, 2, 3 ]
```

### Empty object

```rex
{}
```

```json output
{}
```

### Object with pairs

```rex
{ a: 1 b: 2 }
```

```json output
{ "a": 1, "b": 2 }
```

### Nested structures

```rex
{ users: [ { name: "Ada" } ] }
```

```rex output
{ users: [ { name: "Ada" } ] }
```

---

## Arithmetic

### Addition

```rex
1 + 2
```

```rex output
3
```

### Subtraction

```rex
10 - 3
```

```rex output
7
```

### Multiplication

```rex
4 * 5
```

```rex output
20
```

### Division

```rex
10 / 2
```

```rex output
5
```

### Modulo

```rex
7 % 3
```

```rex output
1
```

### Negation

```rex
-(5 + 3)
```

```rex output
-8
```

### String concatenation

```rex
"hello" + " " + "world"
```

```rex output
"hello world"
```

### Operator precedence

Multiplication binds tighter than addition.

```rex
2 + 3 * 4
```

```rex output
14
```

### Parentheses override precedence

```rex
(2 + 3) * 4
```

```rex output
20
```

---

## Comparison

Comparisons return the left-hand value on success, `none` on failure.

### Equal

```rex
5 == 5
```

```rex output
5
```

### Not equal (match)

```rex
5 != 3
```

```rex output
5
```

### Not equal (no match)

```rex
5 != 5
```

```rex output
none
```

### Greater than

```rex
5 > 3
```

```rex output
5
```

### Greater than (fails)

```rex
3 > 5
```

```rex output
none
```

### Less than

```rex
3 < 5
```

```rex output
3
```

### Greater or equal

```rex
5 >= 5
```

```rex output
5
```

### Less or equal

```rex
3 <= 5
```

```rex output
3
```

### String comparison

```rex
"a" < "b"
```

```rex output
"a"
```

---

## Bitwise

### Bitwise AND

```rex
0xFF & 0x0F
```

```rex output
15
```

### Bitwise OR

```rex
0xF0 | 0x0F
```

```rex output
255
```

### Bitwise XOR

```rex
0xFF ^ 0x0F
```

```rex output
240
```

### Bitwise NOT

```rex
~0
```

```rex output
-1
```

### Boolean AND

```rex
true & false
```

```rex output
false
```

### Boolean OR

```rex
true | false
```

```rex output
true
```

### Boolean NOT

```rex
~true
```

```rex output
false
```

---

## Existence Logic

Rex uses existence-based logic. Only `none` is falsy.

### Or returns first defined

```rex
none or 42
```

```rex output
42
```

### Or skips multiple nones

```rex
none or none or 7
```

```rex output
7
```

### Or returns first (not truest)

```rex
false or 42
```

```rex output
false
```

### Null is defined

```rex
null or 42
```

```rex output
null
```

### Zero is defined

```rex
0 or 42
```

```rex output
0
```

### Empty string is defined

```rex
"" or "fallback"
```

```rex output
""
```

### And returns last if all defined

```rex
1 and 2 and 3
```

```rex output
3
```

### And short-circuits on none

```rex
1 and none and 3
```

```rex output
none
```

### And-or composition

`and` binds tighter than `or`.

```rex
none and 1 or 2
```

```rex output
2
```

---

## Variables

### Assignment

```rex
x = 42
x
```

```rex output
42
```

### Assignment returns value

```rex
x = 42
```

```rex output
42
```

### Multiple assignments

```rex
x = 1
y = 2
x + y
```

```rex output
3
```

### Compound add

```rex
x = 10
x += 5
x
```

```rex output
15
```

### Compound subtract

```rex
x = 10
x -= 3
x
```

```rex output
7
```

### Compound multiply

```rex
x = 4
x *= 3
x
```

```rex output
12
```

### Undefined variable is none

```rex
x
```

```rex output
none
```

### With input

```rex
a + b
```

```rex input
a = 3
b = 4
```

```rex output
7
```

---

## Semicolons

Semicolons are the compound expression operator.

### Returns last value

```rex
1; 2; 3
```

```rex output
3
```

### Forces expression boundary

`10; -3` is `10` then `-3` (negate), not `10 - 3` (subtract).

```rex
10; -3
```

```rex output
-3
```

### Side effects before result

```rex
x = 1; y = 2; x + y
```

```rex output
3
```

---

## Navigation

### Static property

```rex
{ a: 1 }.a
```

```rex output
1
```

### Nested navigation

```rex
{ a: { b: 42 } }.a.b
```

```rex output
42
```

### Dynamic navigation

```rex
obj = { x: 1 }
key = "x"
obj.(key)
```

```rex output
1
```

### Dynamic navigation with expression

```rex
keys = [ "a", "b" ]
obj = { a: 1 b: 2 }
obj.(keys.0)
```

```rex output
1
```

### Array index

```rex
[ 10, 20, 30 ].1
```

```rex output
20
```

### Array size

```rex
[ 1, 2, 3 ].size
```

```rex output
3
```

### String size

```rex
"hello".size
```

```rex output
5
```

### Missing property is none

```rex
{ a: 1 }.b
```

```rex output
none
```

### Out of bounds is none

```rex
[ 1, 2, 3 ].5
```

```rex output
none
```

---

## Object Mutation

### Set property

```rex
obj = { x: 1 }
obj.x = 2
obj.x
```

```rex output
2
```

### Add property

```rex
obj = {}
obj.name = "Rex"
obj.name
```

```rex output
"Rex"
```

### Set dynamic key

```rex
obj = {}
key = "color"
obj.(key) = "blue"
obj.color
```

```rex output
"blue"
```

### Mutation through alias

Both variables point to the same object.

```rex
a = { x: 1 }
b = a
b.x = 2
a.x
```

```rex output
2
```

### Delete property

```rex
obj = { a: 1 b: 2 }
delete obj.a
obj.a
```

```rex output
none
```

---

## Control Flow

### When true

```rex
when true do 42 end
```

```rex output
42
```

### When none

```rex
when none do 42 end
```

```rex output
none
```

### When-else

```rex
when none do 1 else 2 end
```

```rex output
2
```

### Else-when chain

```rex
x = none
y = true
when x do 1 else when y do 2 else 3 end
```

```rex output
2
```

### Unless

```rex
unless none do 42 end
```

```rex output
42
```

### Unless (defined skips body)

```rex
unless true do 42 end
```

```rex output
none
```

### Unless-else

```rex
unless true do 1 else 2 end
```

```rex output
2
```

### Mixed when-unless chain

```rex
a = none
b = true
when a do 1 else unless b do 2 else 3 end
```

```rex output
3
```

### Binding in condition

The `=` in a condition binds and checks existence.

```rex
when x = 5 + 3 do x * 2 end
```

```rex output
16
```

### Return

```rex
return 42
99
```

```rex output
42
```

### Return in conditional

```rex
when true do return 1 end
2
```

```rex output
1
```

### Return from unless guard

```rex
x = true
unless x do
  return "missing"
end
"ok"
```

```rex output
"ok"
```

---

## Loops

### For-in values

```rex
sum = 0
for v in [ 1, 2, 3 ] do
  sum = sum + v
end
sum
```

```rex output
6
```

### For-in returns last

```rex
for v in [ 1, 2, 3 ] do v * 10 end
```

```rex output
30
```

### For index, value on array

```rex
[ i for i, v in [ 10, 20, 30 ] ]
```

```rex output
[ 0, 1, 2 ]
```

### For key, value on object

```rex
[ k for k, v in { a: 1 b: 2 } ]
```

```rex output
[ "a", "b" ]
```

### For-of (keys only)

```rex
[ k for k of { x: 1 y: 2 } ]
```

```rex output
[ "x", "y" ]
```

### For-in over string

```rex
[ c for c in "hi" ]
```

```rex output
[ "h", "i" ]
```

### While loop

```rex
x = 0
while x < 5 do
  x = x + 1
end
x
```

```rex output
5
```

### Break

```rex
x = 0
while true do
  x = x + 1
  when x == 3 do break end
end
x
```

```rex output
3
```

### Continue

```rex
sum = 0
for v in [ 1, 2, 3, 4, 5 ] do
  unless v % 2 == 0 do continue end
  sum = sum + v
end
sum
```

```rex output
6
```

---

## Ranges

### Ascending range

```rex
1..5
```

```rex output
[ 1, 2, 3, 4, 5 ]
```

### Descending range

```rex
5..1
```

```rex output
[ 5, 4, 3, 2, 1 ]
```

### Single element range

```rex
3..3
```

```rex output
[ 3 ]
```

---

## Comprehensions

### Array map

```rex
[ v * 2 for v in [ 1, 2, 3 ] ]
```

```rex output
[ 2, 4, 6 ]
```

### Array filter

Return `none` to exclude an element.

```rex
[ v >= 3 and v for v in [ 1, 2, 3, 4, 5 ] ]
```

```rex output
[ 3, 4, 5 ]
```

### Array comprehension over range

```rex
[ v * v for v in 1..5 ]
```

```rex output
[ 1, 4, 9, 16, 25 ]
```

### Object comprehension

```rex
{ (k): v * 10 for k, v in { a: 1 b: 2 } }
```

```rex output
{ a: 10 b: 20 }
```

### Object from array

```rex
{ (u.name): u.score for u in [ { name: "Ada" score: 95 }, { name: "Bob" score: 72 } ] }
```

```rex output
{ Ada: 95 Bob: 72 }
```

### Object filter by value

```rex
{ (k): v >= 2 and v for k, v in { a: 1 b: 2 c: 3 } }
```

```rex output
{ b: 2 c: 3 }
```

### Object filter by key

```rex
{ (k == "a" and k): v for k, v in { a: 1 b: 2 } }
```

```rex output
{ a: 1 }
```

### While comprehension

```rex
x = 1
[ x = x * 2 while x < 100 ]
```

```rex output
[ 2, 4, 8, 16, 32, 64, 128 ]
```

### Multi-expression body

```rex
a = 0; b = 1
[ c = a + b
  a = b
  b = c
  while a <= 20 ]
```

```rex output
[ 1, 2, 3, 5, 8, 13, 21, 34 ]
```

### For-of comprehension

```rex
[ k for k of { name: "Rex" version: 1 } ]
```

```rex output
[ "name", "version" ]
```

---

## Template Literals

### Simple template

```rex
`hello`
```

```rex output
"hello"
```

### Template interpolation

```rex
name = "world"
`hello ${name}`
```

```rex output
"hello world"
```

### Integer interpolation

```rex
x = 42
`value: ${x}`
```

```rex output
"value: 42"
```

### Bool interpolation

Booleans render as check/cross marks.

```rex
`${true}`
```

```rex output
"✓"
```

### None interpolation

```rex
`${none}`
```

```rex output
"∅"
```

### Null interpolation

```rex
`${null}`
```

```rex output
"␀"
```

---

## Type Predicates

Type predicates return the value if it matches the type, `none` otherwise.

### isString

```rex
isString("hello")
```

```rex output
"hello"
```

### isString (fails)

```rex
isString(42)
```

```rex output
none
```

### isNumber

```rex
isNumber(42)
```

```rex output
42
```

### isNumber (fails)

```rex
isNumber("text")
```

```rex output
none
```

### isBoolean

```rex
isBoolean(true)
```

```rex output
true
```

### isArray

```rex
isArray([ 1, 2 ])
```

```rex output
[ 1, 2 ]
```

### isObject

```rex
isObject({ a: 1 })
```

```rex output
{ a: 1 }
```

---

## Comments

Comments are preserved by the parser and formatter but don't affect execution.

### Line comment

```rex
// this is a comment
42
```

```rex output
42
```

### Block comment

```rex
/* block comment */
42
```

```rex output
42
```

### Inline comment

```rex
1 + /* middle */ 2
```

```rex output
3
```

---

## Edge Cases

### Deeply nested navigation

```rex
{ a: { b: { c: { d: 42 } } } }.a.b.c.d
```

```rex output
42
```

### Chained or

```rex
none or none or none or "found"
```

```rex output
"found"
```

### Nested comprehension

```rex
[ [ v * 2 for v in row ] for row in [ [ 1, 2 ], [ 3, 4 ] ] ]
```

```rex output
[ [ 2, 4 ], [ 6, 8 ] ]
```

### Comprehension with no results

All filtered out → empty array.

```rex
[ v > 100 and v for v in [ 1, 2, 3 ] ]
```

```rex output
[]
```

### Empty for loop

```rex
for v in [] do v end
```

```rex output
none
```

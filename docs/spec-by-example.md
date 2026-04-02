# Rex Language Spec

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

### Integers

```rex
42
```

```rex output
42
```

### Negative integers

```rex
-7
```

```rex output
-7
```

### Strings

```rex
"hello"
```

```rex output
"hello"
```

### Booleans

```rex
true
```

```rex output
true
```

### Null

```rex
null
```

```rex output
null
```

### None

```rex
none
```

```rex output
none
```

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

### String concatenation

```rex
"hello" + " " + "world"
```

```rex output
"hello world"
```

### With input variables

```rex
a + 2
```

```rex input
a = 4
```

```rex output
6
```

## Existence Logic

Rex uses existence-based logic. Only `none` is falsy.

### Or returns first defined

```rex
none or 42
```

```rex output
42
```

### Or skips none

```rex
none or none or 7
```

```rex output
7
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

### False is defined

`false` is a defined value — only `none` is falsy.

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

## Variables

### Assignment

```rex
x = 42
x
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

### Compound assignment

```rex
x = 10
x += 5
x
```

```rex output
15
```

## Control Flow

### When (true)

```rex
when true do 42 end
```

```rex output
42
```

### When (false)

```rex
when none do 42 end
```

```rex output
none
```

### When-else

```rex
x = none
when x do
  1
else
  2
end
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

### Unless (defined skips)

```rex
unless true do 42 end
```

```rex output
none
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

### For-in loop

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

### Return

```rex
return 42
99
```

```rex output
42
```

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

### Set dynamic key

```rex
obj = {}
obj.(4) = true
obj.(4)
```

```rex output
true
```

### Mutation through alias

Both variables see the same object.

```rex
a = { x: 1 }
b = a
b.x = 2
a.x
```

```rex output
2
```

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

### Object comprehension

```rex
{ (k): v * 10 for k, v in { a: 1 b: 2 } }
```

```rex output
{ a: 10 b: 20 }
```

### Object comprehension from array

```rex
{ (u.name): u.score for u in [ { name: "Ada" score: 95 }, { name: "Bob" score: 72 } ] }
```

```rex output
{ Ada: 95 Bob: 72 }
```

### Object filtering by value

```rex
{ (k): v >= 2 and v for k, v in { a: 1 b: 2 c: 3 } }
```

```rex output
{ b: 2 c: 3 }
```

### While comprehension

```rex
x = 1
[ x = x * 2 while x < 100 ]
```

```rex output
[ 2, 4, 8, 16, 32, 64, 128 ]
```

### For-of (keys)

```rex
[ k for k of { a: 1 b: 2 } ]
```

```rex output
[ "a", "b" ]
```

### For key-value on object

```rex
[ k for k, v in { x: 1 y: 2 } ]
```

```rex output
[ "x", "y" ]
```

### For key-value on array

```rex
[ i for i, v in [ 10, 20, 30 ] ]
```

```rex output
[ 0, 1, 2 ]
```

## Semicolons

Semicolons are the compound expression operator.

### Compound returns last

```rex
1; 2; 3
```

```rex output
3
```

### Forces expression boundary

```rex
10; -3
```

```rex output
-3
```

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

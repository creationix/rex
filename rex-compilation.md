# Rex to REXC Compilation Reference

How Rex source language constructs map to [REXC bytecode](rexc-bytecode.md). See [language.md](language.md) for Rex syntax and semantics, [rx-format.md](rx-format.md) for the RX data layer, and [rexc-bytecode.md](rexc-bytecode.md) for the full bytecode spec.

---

## Literals

| Rex source       | REXC          | Notes                              |
|------------------|---------------|------------------------------------|
| `0`              | `+`           | zigzag(0) = empty                  |
| `42`             | `1k+`         | zigzag(42) = 84                    |
| `-1`             | `1+`          | zigzag(-1) = 1                     |
| `3.14`           | `3*9Q+`       | exp=-2, sig=314                    |
| `"hello"`        | `5,hello`     | 5-byte string                      |
| `""`             | `,`           | 0-byte string                      |
| `true`           | `t'`          | ref                                |
| `false`          | `f'`          | ref                                |
| `null`           | `n'`          | ref                                |
| `none`           | `no'`         | ref (absence value)                |

## Variables

| Rex source       | REXC          |
|------------------|---------------|
| `x`              | `x$`          |
| `my-var`         | `my-var$`     |
| `trace-id`       | `trace-id$`   |

Variable names are b64 byte sequences before the `$` tag. Rex identifiers may contain dashes.

## Assignment

| Rex source       | REXC              | Notes                    |
|------------------|-------------------|--------------------------|
| `x = 42`         | `=x$1k+`          | set place, value         |
| `x += 1`         | `=x$(ad%x$2+)`    | desugared: x = add(x, 1) |
| `x -= 1`         | `=x$(sb%x$2+)`    | desugared: x = sub(x, 1) |
| `x \|\|= val`    | `=x$\|(x$val$)`   | desugared: x = x or val  |
| `delete x`       | `~x$`             | delete                   |

All compound assignment operators (`+=`, `-=`, `*=`, `/=`, `%=`, `||=`, `&&=`) desugar to `=place(op place value)`.

## Navigation

Property access compiles to a call where the first child is the target and subsequent children are string keys:

| Rex source             | REXC                          |
|------------------------|-------------------------------|
| `user.name`            | `(user$4,name)`               |
| `user.address.street`  | `(user$7,address6,street)`    |
| `table.(key)`          | `(table$key$)`                |
| `items.0`              | `(items$+)`                   |

Navigation assignment compiles to `=` with the call chain as the place:

| Rex source             | REXC                          |
|------------------------|-------------------------------|
| `user.name = "Ada"`    | `=(user$4,name)3,Ada`         |

## Operators

Binary operators compile to opcode calls:

| Rex source   | REXC            | Opcode |
|--------------|-----------------|--------|
| `a + b`      | `(ad%a$b$)`     | `ad`   |
| `a - b`      | `(sb%a$b$)`     | `sb`   |
| `a * b`      | `(ml%a$b$)`     | `ml`   |
| `a / b`      | `(dv%a$b$)`     | `dv`   |
| `a % b`      | `(md%a$b$)`     | `md`   |
| `a == b`     | `(eq%a$b$)`     | `eq`   |
| `a != b`     | `(nq%a$b$)`     | `nq`   |
| `a > b`      | `(gt%a$b$)`     | `gt`   |
| `a >= b`     | `(ge%a$b$)`     | `ge`   |
| `a < b`      | `(lt%a$b$)`     | `lt`   |
| `a <= b`     | `(le%a$b$)`     | `le`   |
| `a & b`      | `(an%a$b$)`     | `an`   |
| `a \| b`     | `(or%a$b$)`     | `or`   |
| `a ^ b`      | `(xr%a$b$)`     | `xr`   |
| `a..b`       | `(rn%a$b$)`     | `rn`   |

Unary operators:

| Rex source   | REXC          | Opcode |
|--------------|---------------|--------|
| `-x`         | `(ng%x$)`     | `ng`   |
| `!x`         | `(nt%x$)`     | `nt`   |

### Precedence

The compiler handles precedence during parsing. In REXC, the tree structure encodes precedence directly:

```rex
1 + 2 * 3          // parsed as 1 + (2 * 3)
```
```rexc
(ad%2+(ml%4+6+))   // add(1, mul(2, 3))
```

## Short-Circuit Operators

| Rex source       | REXC          |
|------------------|---------------|
| `a or b`         | `\|(a$b$)`    |
| `a and b`        | `&(a$b$)`     |
| `a or b or c`    | `\|(a$\|(b$c$))` |

`or` and `and` are right-associative in the bytecode when chained. The interpreter short-circuits: `or` skips the right child if the left is defined; `and` skips the right if the left is none.

`nor` compiles to `unless`:

| Rex source       | REXC          |
|------------------|---------------|
| `a nor b`        | `!(a$b$)`     |

## Control Flow

### when / unless

```rex
when x do body end              ->  ?(x$ {body})
when x do a else b end          ->  ?(x$ {a} {b})
unless x do body end            ->  !(x$ {body})
```

When the body is a single expression, it may be emitted without block delimiters:

```rex
when x do 42 end                ->  ?(x$ 1k+)
```

### else-when chains

`else when` chains compile to nested `when` in the else branch:

```rex
when a do x                     ->  ?(a$ x$ ?(b$ y$ z$))
else when b do y
else z
end
```

### Self binding

In `when`, the condition value is pushed onto the self stack. `self` inside the body refers to it:

```rex
when get-user() do              ->  ?(get-user%() ...)
  self.name                         // self = @, depth 0
end
```

## Iteration

### for-in / for-of

```rex
for x in items do body end      ->  >(items$ x$ {body})
for k, v in items do body end   ->  >(items$ k$ v$ {body})
for k of items do body end      ->  <(items$ k$ {body})
```

### while

```rex
while cond do body end          ->  #((gt%n$+) {body})
```

### break / continue

```rex
break                           ->  \       // 0 = break depth 0
continue                        ->  1\      // 1 = continue depth 0
```

## Comprehensions

Array comprehensions use the loop modifier with `[]` instead of `()`:

```rex
[x + 1 for x in items]         ->  >[items$ x$ (ad%x$2+)]
[self * self in items]          ->  >[items$ (ml%@@ )]
[x while cond]                  ->  #[cond$ x$]
```

Object comprehensions use `{}`:

```rex
{k: v for k, v in items}       ->  >{items$ k$ v$ k$ v$}
```

## Data Structures

### Arrays

```rex
[1, 2, 3]                      ->  [2+4+6+]
[]                              ->  []
```

### Objects

```rex
{name: "Ada", score: 95}       ->  {4,name3,Ada5,score2-+}
{}                              ->  {}
```

Computed keys use the key expression directly:

```rex
{(key): value}                  ->  {key$value$}
```

## Blocks

Multi-expression bodies compile to `{}` blocks. The interpreter returns the last expression's value:

```rex
x = 1
y = 2
x + y
```

Compiles to a top-level sequence:

```rexc
=x$2+=y$4+(ad%x$y$)
```

When used as a branch body or loop body, the block gets explicit delimiters:

```rexc
?(cond$ {=x$2+=y$4+(ad%x$y$)})
```

## Return

```rex
return 42                       ->  ;1k+
return                          ->  ;no'
```

In a skip position (conditional branch), return passes the skip flag to its child:

```rex
when x do return [1, 2] end    ->  ?(x$ ;2[2+4+])
                                         ^ child gets length prefix, not the ;
```

## Template Literals

### Untagged

```rex
`hello ${name}`                 ->  .[5,hello name$]
`plain string`                  ->  5,plain string   // no interpolation = plain string
```

### Tagged

```rex
html`<a>${title}</a>`           ->  (html%[4,<a >5,</a>]title$)
```

The tag function receives the string parts array and interpolated values as separate arguments.

## Self

`self` refers to the current context value (condition in `when`, current element in `for`). Nested scopes use depth:

| Rex source           | REXC     | Notes          |
|----------------------|----------|----------------|
| `self`               | `@`      | depth 0        |
| `self` (outer scope) | `1@`     | depth 1        |
| `self.name`          | `(@4,name)` | navigation  |

## Type Predicates

Type predicates are opcodes that return the value if it matches, none otherwise:

| Rex source       | REXC            | Opcode |
|------------------|-----------------|--------|
| `string(x)`      | `(st%x$)`      | `st`   |
| `number(x)`      | `(nm%x$)`      | `nm`   |
| `object(x)`      | `(ob%x$)`      | `ob`   |
| `array(x)`       | `(ar%x$)`      | `ar`   |
| `boolean(x)`     | `(bt%x$)`      | `bt`   |

## Function Calls

Method-style calls compile to navigation:

```rex
items.size                      ->  (items$4,size)
items.0                         ->  (items$+)
```

Domain opcode calls:

```rex
add(1, 2)                       ->  (ad%2+4+)
```

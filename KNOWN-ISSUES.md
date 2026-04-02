# Known Issues

## Typechecker: for..in doesn't narrow element types

When iterating over a typed array with `for..in`, the loop variable isn't narrowed to the element type.

```rex
users: [{ name: str, score: int }] = [
  { name: "Ada" score: 95 }
  { name: "Ben" score: 72 }
]
scores-by-name = { (u.name): u.score for u in users }  // u.name, u.score not resolved
```

**Impact:** Code using typed arrays in comprehensions won't get full type checking on element properties.

**Fix:** `for..in` over `[T]` should bind the loop variable as `T`.

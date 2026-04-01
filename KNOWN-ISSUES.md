# Known Issues

## Typechecker: for..in doesn't narrow element types

When iterating over a typed array with `for..in`, the loop variable isn't narrowed to the element type. The typechecker also doesn't count dynamic property access (`obj.(key)`) as a variable use, producing false "assigned but never used" warnings.

```rex
users: [{name: str, score: int}] = [
  {name: "Ada" score: 95}
  {name: "Ben" score: 72}
]
scores-by-name = {(u.name): u.score for u in users}  // u.name, u.score not resolved

key = "Ada"
scores-by-name.(key)  // key not counted as a use → false "never used" warning
```

**Impact:** `examples/features/collections.rex` has a spurious warning. Any code using typed arrays in comprehensions won't get full type checking.

**Fix:** `for..in` over `[T]` should bind the loop variable as `T`. Dynamic property access `obj.(var)` should count as a use of `var`.

## Formatter is lossy

`rex format` round-trips through the compiler pipeline (lex -> parse -> lower -> decompile), which loses information:

- Comments are stripped
- `extern` declarations are stripped (no bytecode representation)
- Type annotations are lost (`bonus: int = 10` -> `bonus = int`)
- Dynamic navigation becomes static (`grades.(subj)` -> `grades.subj`)
- No trailing newline

The LSP formatting provider is disabled until this is fixed.

**Fix:** Implement a CST-based formatter that operates directly on the parse tree, adjusting only whitespace and indentation while preserving all tokens.

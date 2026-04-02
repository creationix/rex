# Rex Language Spec by Example

This file is the golden test suite for the Rex language. Each test case
is defined by code blocks under a markdown header. The test runner
(`crates/rex-core/tests/spec.rs`) parses this file and runs each test.

## Format

- `rex` — input: compile and run in a shared VM (state carries across blocks)
- `json` — output check: structural match against the last expression result
- `json vars` — output check: structural match against all current variables
- `rexc` — output check: exact match against bytecode of previous rex block

Multiple blocks per test, interleaved freely. Prose is ignored by the runner.

---

## Basics

### Integer literal

```rex
42
```

```json
42
```

### Arithmetic

```rex
2 + 3 * 4
```

```json
14
```

### Variables persist across blocks

```rex
x = 10
```

```rex
x + 5
```

```json
15
```

### Check variables

```rex
a = 1
b = 2
```

```json vars
{"a": 1, "b": 2}
```

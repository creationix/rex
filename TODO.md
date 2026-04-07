# Type System: Built-in Methods, Properties & Intersection Types

## 1. Built-in method argument validation & return types

`builtin_method_type()` currently only returns the return type. Change it to also return argument expectations so `infer_call` can validate arguments.

**Array methods:**

| Method          | Args | Validates                    | Returns                  |
|-----------------|------|------------------------------|--------------------------|
| `push(val)`     | 1    | `val` assignable to `T`      | `typeof(val)` (identity) |
| `pop()`         | 0    | —                            | `T \| none`              |
| `join(sep)`     | 1    | `sep` assignable to `string` | `string`                 |
| `contains(val)` | 1    | `val` assignable to `T`      | `T \| none`              |
| `slice(s, e)`   | 2    | both assignable to `integer` | `[T]`                    |

**String methods:** Remove `indexOf` and `contains`. Remaining methods (`split`, `trim`, `upper`, `lower`, `replace`, `slice`, `starts-with`, `ends-with`) have concrete types, no changes needed.

**Implementation:**
- Change `builtin_method_type` to return a struct like `BuiltinMethod { args: Vec<Type>, returns: Type }`
- In `infer_call`, use returned arg types to validate actual arguments
- For `push`, return the actual arg type (identity) rather than the array type

## 2. `isObject` / `isArray` predicate narrowing

| Predicate     | Narrows to  | Return type         |
|---------------|-------------|---------------------|
| `isObject(x)` | `{*: some}` | `{*: some} \| none` |
| `isArray(x)`  | `[some]`    | `[some] \| none`    |

After narrowing:
- `isObject` → access any property (`x.foo` → `some | none`), iterate (`for k, v in x`)
- `isArray` → index (`x.0` → `some | none`), iterate (`for v in x`), call array methods

## 3. Size/count properties

| Property      | Available on        | Returns   | Notes                                  |
|---------------|---------------------|-----------|----------------------------------------|
| `.length`     | all strings         | `integer` | count of unicode codepoints            |
| `.byteLength` | all strings         | `integer` | count of utf-8 bytes (free from wire format) |
| `.count`      | all arrays, objects | `integer` | number of elements / keys              |

Distinct names prevent ambiguity on intersection types.

## 4. Host-only `string & [string]` intersection type

Hosts can expose values that are simultaneously a string and an array of strings (e.g., URL paths, HTTP headers, query parameters). This is host-created only — Rex code creates `string | [string]` unions, which are assignable to the intersection type.

**Dispatch rules:**
- Context expecting a string → string form
- Context expecting `[string]` → array form
- Indexing and iteration → array form
- `.length` / `.byteLength` → string (codepoints / bytes)
- `.count` → array (number of elements)
- String methods (`split`, `trim`, `starts-with`, etc.) → string form
- Array methods (`push`, `pop`, `slice`, etc.) → array form
- `.contains` → array-only (no ambiguity since it was removed from strings)
- Case-insensitive header keys are a host implementation detail, not a type system concern

## Tests to add

- `[1,2,3].push("x")` → type error (string not assignable to integer)
- `[1,2,3].push(4)` → returns `integer` (not `[integer]`)
- `when isObject(x) do x.foo end` → type is `some | none`
- `when isArray(x) do x.push(1) end` → no error
- `items.slice("a", "b")` → type error (string not assignable to integer)
- `"hello".contains("h")` → type error (contains is array-only)
- `"hello".indexOf("h")` → type error (indexOf removed)

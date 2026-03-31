# RX Format Spec

RX is a compact data format encoded as printable UTF-8. It covers the same data model as JSON — numbers, strings, arrays, objects, booleans, and null — with built-in support for deduplication, schema sharing, and optional random-access indexing.

RX embeds directly in JSON string values with minimal escaping.

> RX is a strict subset of [REXC](rexc-bytecode.md). Every valid RX document is valid REXC.

---

## Parsing Rule

RX is parsed **left-to-right**. Every value starts with zero or more base-64 digits (the varint), followed by a non-b64 tag byte. The tag determines how to interpret the varint and what (if any) body follows.

```
[b64 digits][tag][body]
```

The parser:
1. Scans b64 digits greedily -> varint bytes
2. Reads the next byte -> tag
3. Interprets the varint based on the tag
4. Reads body if the tag requires one

**Worked example** -- parsing `5,hello`:

1. Start at the left: `5` is a b64 digit -> varint = 5
2. Next byte: `,` is not b64 -> tag (string)
3. Tag says there are 5 bytes of body to the right -> `hello`

### Base-64 Digit Alphabet

```
0-9   values 0-9
a-z   values 10-35
A-Z   values 36-61
-     value 62
_     value 63
```

Digits form big-endian base-64 unsigned integers. Zero is an empty string (no digits).

### Zigzag Encoding

Signed integers use zigzag: `n >= 0 ? 2n : -2n - 1`

```
 0 -> 0    -1 -> 1     1 -> 2    -2 -> 3     2 -> 4
```

---

## Scalars

Scalars have no body -- just varint + tag.

| Tag | Name    | Varint meaning                         | Example                                |
|-----|---------|----------------------------------------|----------------------------------------|
| `+` | Integer | zigzag signed integer                  | `+` = 0, `2+` = 1, `1+` = -1, `1k+` = 42 |
| `*` | Decimal | zigzag exponent, then reads next `[sig]+` | `3*1k+` = 42 x 10^-2 = 0.42          |
| `,` | String  | byte count of raw UTF-8 body           | `5,hello` = "hello", `,` = ""         |
| `'` | Ref     | name (opaque b64 bytes)                | `t'` = true, `f'` = false, `n'` = null |

### Decimals

`[exp]*` is a prefix that consumes the next integer as the significand:

```
[zigzag_exp]*[zigzag_sig]+    ->    sig x 10^exp
```

### Built-in References

| Encoding | Value     |
|----------|-----------|
| `t'`     | true      |
| `f'`     | false     |
| `n'`     | null      |
| `no'`    | none      |
| `nan'`   | NaN       |
| `inf'`   | +Infinity |
| `nif'`   | -Infinity |

### Pointers

`[delta]^` -- a relative offset from the byte after `^` to the start of the target value. Used for deduplication: repeated values become pointers to an earlier occurrence.

| Encoding | Meaning                      |
|----------|------------------------------|
| `^`      | delta 0 (target starts here) |
| `3^`     | delta 3 bytes forward        |

---

## Containers

Containers use paired delimiters. The body is zero or more values between the delimiters.

| Open | Close | Name   |
|------|-------|--------|
| `[`  | `]`   | Array  |
| `{`  | `}`   | Object |

### Arrays

Ordered list of values.

| Encoding     | Value       |
|--------------|-------------|
| `[]`         | `[]`        |
| `[2+4+6+]`  | `[1, 2, 3]` |

### Objects

Ordered mapping from strings to values. Encoded as alternating key/value pairs, or as a schema pointer followed by values only.

| Encoding                 | Value                    |
|--------------------------|--------------------------|
| `{}`                     | `{}`                     |
| `{4,name3,Ada5,score2-+}` | `{name: "Ada", score: 95}` |

### Schema Sharing

When the first value inside `{}` resolves to an object or array, it is treated as a **schema** defining the key layout. The remaining children are values in matching order.

Schema and index are mutually exclusive on the same object. Schema is for compression of many small same-shape objects; index is for random access into large flat objects.

```
{9^3,Bob1k+}               -> {name: "Bob", score: 42}
                               (schema pointer to an earlier object with same keys)
```

---

## Indexed Containers

A container can include an index for random access. The `#` tag appears inside the container, before the element data.

### Format

```
[ <packed>#<pointers> <elements> ]
{ <packed>#<pointers> <key0><val0>... }
```

The varint before `#` (`packed`) encodes two values:
- **Lower 3 bits**: pointer width minus 1 (1-8 b64 digits per pointer)
- **Upper bits**: element count (`packed >> 3`)

Each pointer is a fixed-width b64 number giving the byte offset from the end of the pointer table to the start of that element.

### Example

A 2-element array `[1, 2]` with pointer width 1:
- count=2, width=1, packed = (2 << 3) | 0 = 16, varint `g`
- ptr0=`0` (offset 0), ptr1=`2` (offset 2)
- Result: `[g#022+4+]`

### Indexed Objects

Pointers are sorted by key (byte-order comparison of the encoded key) for O(log n) binary search lookup.

### Disambiguation

The `#` tag is distinguished from non-indexed content by requiring at least one b64 digit before it. An empty varint followed by `#` is never an index (in REXC, empty varint + `#` is the While modifier).

### Producer API

The encoder exposes options for the producer to control indexing. Typical usage: specify which container paths should be indexed for random access.

```rust
// Conceptual API -- index the root array and nested objects
let options = EncodeOptions {
    index_paths: vec!["$", "$.*.metadata"],
    ..default()
};
let rx = encode_rx(&value, &options);
```

Containers not on the index path list are encoded eagerly (no index). This keeps small containers compact while enabling lazy access on large ones.

---

## String Chains

`[size].[seg1 seg2 ...]` -- a string built from concatenated segments. Each segment can be a string, pointer, or another chain.

```
5.[3^4,/baz]     -> chain: pointer resolves to "/foo/bar", suffix "/baz" -> "/foo/bar/baz"
```

Used for prefix deduplication of strings with common prefixes (URL paths, header names, etc.).

---

## Tag Summary

| Tag     | Kind   | Description                                                      |
|---------|--------|------------------------------------------------------------------|
| `+`     | scalar | Integer (zigzag)                                                 |
| `*`     | prefix | Decimal exponent (followed by `[sig]+`)                          |
| `,`     | sized  | String (varint = byte count)                                     |
| `'`     | scalar | Named reference (true, false, null, none, etc.)                  |
| `^`     | scalar | Pointer (delta offset for deduplication)                         |
| `.`     | sized  | String chain (varint = byte count of segments)                   |
| `[` `]` | paired | Array                                                            |
| `{` `}` | paired | Object                                                           |
| `#`     | index  | Index header: varint = (count << 3 \| width-1), then pointer table |

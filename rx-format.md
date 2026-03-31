# RX Format Spec

RX is a compact data format encoded as printable UTF-8. It covers the same data model as JSON — numbers, strings, arrays, objects, booleans, and null — with built-in support for deduplication, schema sharing, and optional random-access indexing.

RX embeds directly in JSON string values with minimal escaping (only `"` and `\` need escaping).

> RX is a strict subset of [REXC](rexc-bytecode.md). Every valid RX document is valid REXC.

---

## Parsing Rule

RX is parsed **left-to-right**. Every value starts with zero or more base-64 digits (the varint), followed by a non-b64 tag byte. The tag determines how to interpret the varint and what (if any) body follows.

```
[b64 digits][tag][body]
```

### Algorithm

```
function read_value(input, pos):
    raw = read_b64_digits(input, pos)   // greedy: advance pos past all b64 chars
    tag = read_byte(input, pos)         // the first non-b64 byte
    switch tag:
        '+': return zigzag_decode(b64_to_uint(raw))
        '*': exp = zigzag_decode(b64_to_uint(raw)); sig = read_value(input, pos); return sig * 10^exp
        ',': len = b64_to_uint(raw); return read_utf8(input, pos, len)
        "'": return lookup_ref(raw)     // raw bytes ARE the name, not a number
        '^': delta = b64_to_uint(raw); return resolve_pointer(input, pos, delta)
        '.': size = b64_to_uint(raw); return read_chain(input, pos, size)
        '[': return read_array(input, pos)
        '{': return read_object(input, pos)
```

Note: the varint `raw` bytes before the tag serve dual purpose depending on the tag:
- For most tags (`+`, `*`, `,`, `^`, `.`): raw is decoded as an unsigned integer
- For `'` (ref): raw is treated as an opaque name string (e.g., `t`, `f`, `n`, `no`)
- For `[`, `{`: raw is ignored (but see REXC spec for length prefixes)

### Base-64 Digit Alphabet

```
Character:  0 1 2 3 4 5 6 7 8 9 a b c d e f g h i j k l m
Value:      0 1 2 3 4 5 6 7 8 9 ...                    22

Character:  n o p q r s t u v w x y z A B C D E F G H I J
Value:      23                                          45

Character:  K L M N O P Q R S T U V W X Y Z - _
Value:      46                                    62 63
```

Every byte that is NOT in this set is a tag byte. Tag bytes include: `+ * , ' ^ . [ ] { } # ( ) ? ! | & > < = / ~ ; @ $ % \`

### Varint Decoding

Digits form a big-endian base-64 unsigned integer:

```
b64_to_uint(digits):
    n = 0
    for each digit d in digits (left to right):
        n = n * 64 + value_of(d)
    return n
```

Zero is encoded as an empty string (zero digits). This means `+` (no digits before tag) encodes integer 0.

### Zigzag Encoding

Signed integers use zigzag mapping so small-magnitude values use few digits:

```
encode: n >= 0 ? 2*n : -2*n - 1
decode: n % 2 == 0 ? n/2 : -(n/2) - 1
```

| Signed | Unsigned | B64 digits |
|--------|----------|------------|
| 0      | 0        | *(empty)*  |
| 1      | 2        | `2`        |
| -1     | 1        | `1`        |
| 42     | 84       | `1k`       |
| -42    | 83       | `1j`       |
| 100    | 200      | `38`       |

---

## Scalars

### Integer -- `+`

`[zigzag_varint]+` -- a signed integer encoded via zigzag.

| JSON   | RX     | Zigzag | Digits |
|--------|--------|--------|--------|
| `0`    | `+`    | 0      | empty  |
| `1`    | `2+`   | 2      | `2`    |
| `-1`   | `1+`   | 1      | `1`    |
| `42`   | `1k+`  | 84     | `1k`   |
| `100`  | `38+`  | 200    | `38`   |

### Decimal -- `*`

`[zigzag_exp]*[zigzag_sig]+` -- a decimal number encoded as `sig * 10^exp`. The `*` tag is a **prefix compound**: after reading the exponent, the decoder reads the next value which must be an integer (`+` tag) giving the significand.

| JSON    | RX       | Sig  | Exp | Calculation    |
|---------|----------|------|-----|----------------|
| `3.14`  | `3*9Q+`  | 314  | -2  | 314 * 10^-2    |
| `0.5`   | `1*9+`   | 5    | -1  | 5 * 10^-1      |
| `-0.25` | `1*8+`   | -25  | -1  | -25 * 10^-1    |

### String -- `,`

`[byte_count],[raw UTF-8 bytes]` -- the varint gives the byte length (not character count), followed by that many raw bytes.

| JSON            | RX              | Bytes |
|-----------------|-----------------|-------|
| `""`            | `,`             | 0     |
| `"hi"`          | `2,hi`          | 2     |
| `"hello"`       | `5,hello`       | 5     |
| `"hello world"` | `b,hello world` | 11    |

### Ref -- `'`

`[name]'` -- a named reference. The b64 digits before `'` are the name (NOT decoded as a number). The decoder looks up the name in a table of built-in values.

| Encoding | JSON Value  |
|----------|-------------|
| `t'`     | `true`      |
| `f'`     | `false`     |
| `n'`     | `null`      |
| `no'`    | none (absent/undefined) |
| `nan'`   | NaN         |
| `inf'`   | +Infinity   |
| `nif'`   | -Infinity   |

Encoders may define additional ref names via an external dictionary shared between encoder and decoder.

### Pointer -- `^`

`[delta]^` -- a forward reference to a value that appears later in the byte stream. The delta is the number of bytes from the byte after `^` to the first byte of the target value.

```
resolve_pointer(input, pos, delta):
    target_pos = pos + delta
    save = pos
    pos = target_pos
    value = read_value(input, pos)
    pos = save       // restore position -- pointer does not advance past target
    return value
```

Pointers enable deduplication: when the encoder encounters a value identical to one already written, it can emit a pointer instead of repeating the bytes. The target must appear later in the stream (higher byte offset) because encoding writes right-to-left internally, then reverses.

### String Chain -- `.`

`[size].[seg0 seg1 ...]` -- a concatenated string built from segments. The varint gives the total byte count of all segments combined. Each segment is a value (typically a string or pointer). The decoder reads values until `size` bytes are consumed, then concatenates all segments into a single string.

```
read_chain(input, pos, size):
    end = pos + size
    result = ""
    while pos < end:
        seg = read_value(input, pos)
        result += to_string(seg)
    return result
```

Chains enable prefix deduplication. For example, `/api/users` and `/api/posts` can share the `/api/` prefix via a pointer.

---

## Containers

### Array -- `[` ... `]`

`[` `elem0 elem1 ... elemN` `]` -- zero or more values between brackets. The decoder reads values until it encounters `]`.

```
read_array(input, pos):
    if peek_is_index(input, pos):
        return read_indexed_array(input, pos)
    items = []
    while input[pos] != ']':
        items.push(read_value(input, pos))
    pos += 1   // consume ']'
    return items
```

| JSON          | RX           |
|---------------|--------------|
| `[]`          | `[]`         |
| `[1, 2, 3]`  | `[2+4+6+]`  |
| `["a", "b"]`  | `[1,a1,b]`  |
| `[[1], [2]]`  | `[[2+][4+]]` |

### Object -- `{` ... `}`

`{` `key0 val0 key1 val1 ...` `}` -- alternating key-value pairs. Keys are always strings. The decoder reads pairs until `}`.

```
read_object(input, pos):
    if peek_is_index(input, pos):
        return read_indexed_object(input, pos)
    if input[pos] == '}':
        pos += 1
        return {}
    first = read_value(input, pos)
    if first is a string:
        // Inline keys: alternating key-value pairs
        pairs = [(first, read_value(input, pos))]
        while input[pos] != '}':
            k = read_value(input, pos)
            v = read_value(input, pos)
            pairs.push((k, v))
        pos += 1   // consume '}'
        return pairs
    if first is an object:
        // Schema sharing: first child defines key layout
        keys = keys_of(first)
        pairs = []
        for key in keys:
            v = read_value(input, pos)
            pairs.push((key, v))
        pos += 1   // consume '}'
        return pairs
    // Unrecognized first child -- error or treat as single-element
```

| JSON                         | RX                            |
|------------------------------|-------------------------------|
| `{}`                         | `{}`                          |
| `{"a": 1}`                   | `{1,a2+}`                     |
| `{"name": "Ada", "age": 30}` | `{4,name3,Ada3,age3O+}`       |

### Schema Sharing

When many objects share the same key set (e.g., rows in a table), the encoder writes the first object normally and subsequent objects as a schema pointer plus values only:

```
{4,name3,Ada3,age3O+}     // first object: {name: "Ada", age: 30}
{9^3,Bob1k+}               // second: schema pointer (9 bytes forward) + values
```

The schema pointer resolves to the first object. The decoder extracts its keys (`name`, `age`) and pairs them with the inline values (`"Bob"`, `42`).

Schema and index are **mutually exclusive** on the same object. Schema compresses many small same-shape objects; index enables random access into large flat objects.

---

## Indexed Containers

A container can include an index for O(1) element access (arrays) or O(log n) key lookup (objects). The `#` tag appears inside the container, before the element data.

### Format

```
[ <packed>#<pointers> <elements> ]
{ <packed>#<pointers> <key0><val0><key1><val1>... }
```

### Packed Header

The varint before `#` encodes two values in a single integer:

```
packed = (count << 3) | (width - 1)

count = packed >> 3           // number of elements (or key-value pairs)
width = (packed & 7) + 1     // b64 digits per pointer (1-8)
```

### Pointer Table

After `#`, exactly `count * width` b64 digits follow. Each group of `width` digits is a fixed-width unsigned integer giving the byte offset from the **end of the pointer table** to the **start of that element**.

```
read_indexed_array(input, pos):
    raw = read_b64_digits(input, pos)
    packed = b64_to_uint(raw)
    assert input[pos] == '#'
    pos += 1     // consume '#'
    count = packed >> 3
    width = (packed & 7) + 1
    // Read pointer table
    pointers = []
    for i in 0..count:
        ptr = read_fixed_b64(input, pos, width)
        pointers.push(ptr)
    table_end = pos   // elements start here
    // For eager decoding, ignore pointers and read sequentially:
    items = []
    while input[pos] != ']':
        items.push(read_value(input, pos))
    pos += 1   // consume ']'
    return items
    // For random access, use: element_start = table_end + pointers[i]
```

### Example

Array `[1, 2, 3]` with pointer width 1:

```
count = 3, width = 1
packed = (3 << 3) | (1 - 1) = 24
varint(24) = "o"              // b64 digit for 24
pointers = [0, 2, 4]          // offsets to each element
body = "2+4+6+"               // elements encoded normally

Result: [o#0242+4+6+]
         │││││└────┘ elements
         ││││└───── ptr2 = 4 (third element starts 4 bytes after table)
         │││└────── ptr1 = 2 (second element starts 2 bytes after table)
         ││└─────── ptr0 = 0 (first element starts 0 bytes after table)
         │└──────── '#' tag
         └───────── packed varint "o" = 24
```

### Indexed Objects

For objects without a schema, pointers point to the start of each **key** (not value). Pointers are **sorted by encoded key** (byte-order comparison) for O(log n) binary search lookup. The body preserves original insertion order.

Binary search procedure:
1. Read the pointer at the middle position
2. Seek to that offset, read the key
3. Compare with the search key
4. Narrow the search range and repeat

### Disambiguation from REXC

In REXC, the `#` tag is also used for While loops. The disambiguation rule: an index requires **at least one b64 digit** before `#`. An empty varint followed by `#` is always a While modifier (REXC only). In pure RX, `#` only appears as an index.

```
peek_is_index(input, pos):
    i = pos
    while i < len(input) and is_b64(input[i]): i += 1
    return i > pos and i < len(input) and input[i] == '#'
```

---

## Encoding

### Minimal Encoder

A conformant RX encoder needs only:

1. Encode JSON scalars as `+`, `*`, `,`, `'` values
2. Encode arrays as `[` ... `]`
3. Encode objects as `{` key val key val ... `}`

No pointers, chains, indexes, or schemas are required. A minimal encoder produces valid RX that any decoder can read.

### Optimizing Encoder

An optimizing encoder may additionally:

- **Deduplicate** repeated values using pointers (`^`)
- **Share schemas** for objects with identical key sets
- **Chain strings** with common prefixes (`.`)
- **Index containers** for random access (`#`)

The encoder writes values right-to-left internally (children before parents) so that pointer deltas always reference content at a higher byte offset. The final output is reversed to produce the left-to-right byte stream.

### Index Control

The producer controls which containers get indexed. Typical API:

```
encode(value, index_paths=["$", "$.*.metadata"])
```

Containers matching the path patterns get an index; all others are encoded eagerly. This keeps small containers compact while enabling random access on large ones.

---

## Complete Encoding/Decoding Example

JSON input:
```json
{"users": [{"name": "Ada", "score": 95}, {"name": "Bob", "score": 42}], "count": 2}
```

RX encoding (with schema sharing, no indexing):
```
{5,users[{4,name3,Ada5,score2-+}{7^1k+}]5,count4+}
```

Decoding walkthrough (left to right):
1. `{` -- start object
2. `5,users` -- key "users"
3. `[` -- start array (value for "users")
4. `{` -- start object (first array element)
5. `4,name` -- key "name"
6. `3,Ada` -- value "Ada"
7. `5,score` -- key "score"
8. `2-+` -- value 95 (zigzag 190 = `2-`)
9. `}` -- end first object
10. `{` -- start second object
11. `7^` -- pointer, delta=7 bytes forward, resolves to the first `{...}` object -> schema
12. `1k+` -- value 42
13. `}` -- end second object (keys from schema: name="Bob"... wait, this needs the actual value for name too)

*(Note: with schema sharing, the second object `{7^1k+}` has the schema pointer + only the differing values. The exact encoding depends on the encoder's dedup strategy.)*

---

## Tag Summary

| Tag     | Kind   | Varint meaning                  | Body                        |
|---------|--------|---------------------------------|-----------------------------|
| `+`     | scalar | zigzag signed integer           | none                        |
| `*`     | prefix | zigzag exponent                 | reads next value as `[sig]+` |
| `,`     | sized  | byte count                      | raw UTF-8 bytes             |
| `'`     | name   | opaque name (not a number)      | none                        |
| `^`     | scalar | forward delta in bytes          | none                        |
| `.`     | sized  | byte count of segments          | concatenated values         |
| `[`     | opener | (ignored in RX)                 | values until `]`            |
| `]`     | closer | --                              | --                          |
| `{`     | opener | (ignored in RX)                 | key-value pairs until `}`   |
| `}`     | closer | --                              | --                          |
| `#`     | index  | (count << 3) \| (width - 1)    | fixed-width pointer table   |

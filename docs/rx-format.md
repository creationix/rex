# RX Data Format

RX is a compact binary-safe data format encoded as printable UTF-8. Same data model as JSON — numbers, strings, arrays, objects, booleans, null — with built-in deduplication, schema sharing, and optional indexing.

Embeds directly in JSON strings (only `"` and `\` need escaping). RX is a strict subset of [REXC](rexc-bytecode.md).

---

## Parsing

Every value: zero or more b64 digits (varint), then a non-b64 tag byte.

```
[b64 digits][tag][body]
```

### B64 Alphabet

```
0-9  → 0–9       a-z → 10–35
A-Z  → 36–61     -   → 62       _ → 63
```

Everything else is a tag byte: `+ * , ' ^ . [ ] { } #`

### Varint

Big-endian base-64 unsigned integer. Empty = 0.

### Zigzag

Signed integers: `n >= 0 ? 2n : -2n - 1`. Small magnitudes use few digits.

| Signed | Encoded | Digits |
|--------|---------|--------|
| 0 | 0 | *(empty)* |
| 1 | 2 | `2` |
| -1 | 1 | `1` |
| 42 | 84 | `1k` |
| 100 | 200 | `38` |

---

## Scalars

### Integer (`+`)

`[zigzag]+`

| Value | RX |
|---|---|
| `0` | `+` |
| `1` | `2+` |
| `-1` | `1+` |
| `42` | `1k+` |

### Decimal (`*`)

`[zigzag_exp]*[zigzag_sig]+` — `sig * 10^exp`. Prefix compound: reads the next `+` value.

| Value | RX | Sig | Exp |
|---|---|---|---|
| `3.14` | `3*9Q+` | 314 | -2 |
| `0.5` | `1*9+` | 5 | -1 |

### String (`,`)

`[byte_count],[raw UTF-8]`

| Value | RX |
|---|---|
| `""` | `,` |
| `"hello"` | `5,hello` |

### Ref (`'`)

`[name]'` — b64 digits are the name, not a number.

| RX | Value |
|---|---|
| `t'` | `true` |
| `f'` | `false` |
| `n'` | `null` |
| `no'` | none |
| `nan'` | NaN |
| `inf'` | Infinity |
| `nif'` | -Infinity |

### Pointer (`^`)

`[delta]^` — forward reference. Delta = bytes from after `^` to target's first byte.

Enables deduplication: emit a pointer instead of repeating an identical value. Targets always appear later in the stream (the encoder writes in reverse order, then reverses the output — see Encoding Strategy below).

Pointers can chain: a pointer may resolve to another pointer. Decoders must follow the chain until reaching a concrete value. (The current Rust encoder never emits chained pointers, but decoders must handle them.)

A pointer's target must be the same type as the value it replaces. Invalid deltas (pointing outside the stream or into the middle of a value) are encoder bugs — decoders may treat them as errors.

### Chain (`.`)

`[size].[seg0 seg1 ...]` — concatenated value from segments. Size = total byte count of all segments. Each segment is a value or pointer.

Chains can contain strings, arrays, or objects — but all segments in a chain must be the same type. String chains enable prefix deduplication. Array and object chains concatenate their contents.

---

## Containers

### Array (`[ ]`)

`[elem0 elem1 ... elemN]` — values until `]`.

| JSON | RX |
|---|---|
| `[]` | `[]` |
| `[1, 2, 3]` | `[2+4+6+]` |
| `[[1], [2]]` | `[[2+][4+]]` |

### Object (`{ }`)

`{key0 val0 key1 val1 ...}` — alternating key-value pairs until `}`.

| JSON | RX |
|---|---|
| `{}` | `{}` |
| `{"a": 1}` | `{1,a2+}` |
| `{"name": "Ada", "age": 30}` | `{4,name3,Ada3,age3O+}` |

**Schema sharing**: when many objects share the same keys, the first encodes normally and subsequent ones use a schema pointer + values only:

```
{4,name3,Ada3,age3O+}   first: {name: "Ada", age: 30}
{9^3,Bob1k+}            second: schema pointer + values → {name: "Bob", age: 42}
```

The pointer resolves to the first object. The decoder extracts its keys and pairs with inline values.

---

## Indexed Containers

Optional index for O(1) array access or O(log n) key lookup. `#` appears inside the container, before element data.

### Format

```
[packed # pointers elements]
{packed # pointers key0 val0 key1 val1 ...}
```

**Packed header** (varint before `#`):

```
count = packed >> 3         // number of elements
width = (packed & 7) + 1   // b64 digits per pointer (1–8)
```

**Pointer table**: `count * width` b64 digits. Each group = byte offset from end of table to start of that element.

### Example

`[1, 2, 3]` indexed with width 1:

```
[o#0242+4+6+]
 │││││└────┘ elements
 ││││└───── ptr2=4
 │││└────── ptr1=2
 ││└─────── ptr0=0
 │└──────── # tag
 └───────── packed: count=3, width=1
```

### Indexed Objects

Pointers point to each key (not value). Sorted by encoded key bytes for binary search.

---

## Encoding

A **minimal encoder** needs only: scalars (`+`, `*`, `,`, `'`), arrays (`[]`), objects (`{}`). No pointers, chains, indexes, or schemas required.

An **optimizing encoder** may additionally:
- Deduplicate with pointers (`^`)
- Share schemas for same-shape objects
- Chain strings with common prefixes (`.`)
- Index containers for random access (`#`)

### Encoding Strategy

The key insight is that pointers are forward references — they point to values that appear later in the stream. This is easy to produce with a reverse-order encoding pass:

1. **Walk the value tree depth-first.** On the way back up (post-order), emit each value's bytes and push them onto an output array.
2. **Track cumulative byte count.** After pushing each value, record the running total of bytes in the output array (including the value just pushed).
3. **Deduplicate.** When you encounter a value you've already emitted, emit a pointer instead. Calculate the delta *before* writing the pointer: `delta = current_total_bytes - target_total_bytes_when_written`. Then encode the pointer (`[delta]^`) and push it. This ordering matters — the pointer is a varint whose size depends on the delta, so the delta must be known first.
4. **Write the chunks in reverse order.** Each chunk is already in normal forward byte order — just reverse the chunk list, not the bytes within them.

Container headers (arrays, objects) are emitted after their children in this scheme, which means they naturally appear before their children in the final output — exactly the right order.

### Deduplication Detection

Finding structurally identical values requires comparing subtrees, which is expensive for large or deeply nested values. A practical approach:

1. **Skip large containers.** Arrays and objects beyond a size threshold (e.g., 16 children) are unlikely to have duplicates, and the cost-to-benefit ratio of comparing them is poor.
2. **Limit recursion.** Do a recursive complexity scan, counting each child node. Abort as soon as a threshold is reached (e.g., 64 nodes). Values exceeding the limit are emitted without dedup.
3. **Hash the value.** For values that pass both filters, compute a unique key and look it up in a hash map. On a hit, emit a pointer; on a miss, emit the value and record it.

The details are language-dependent:

- **JavaScript**: Use `JSON.stringify` as the hash key. Engines heavily optimize it, making it faster than manual traversal — but this would be a terrible choice in most other languages.
- **Rust**: Do a single DFS pass over the entire input to recursively calculate the complexity of every value upfront. Then during the encoding pass, only compute hash keys for values under the complexity threshold. This avoids the per-value abort logic and keeps the hot path simple.

**Schema sharing.** Objects with identical key lists can share a schema. Maintain a separate dedup map for key lists — when encoding an object, extract its keys (a flat list of strings), look it up, and if found, emit a pointer to the previous object followed by just the values. This is simpler than general value dedup since keys are always flat string lists.

**Cost check.** Only emit a pointer if it's actually smaller than re-encoding the value. Pointers are varints — they grow with distance from the target. Record the encoded byte size of each value alongside its position in the dedup map. When a duplicate is found, compare the pointer size (varint length of the delta + 1 for `^`) against the original value's size. If the pointer would be equal or larger, just re-emit the value.

---

## Tag Summary

| Tag | Kind | Varint | Body |
|-----|------|--------|------|
| `+` | scalar | zigzag integer | — |
| `*` | prefix | zigzag exponent | next `+` value |
| `,` | sized | byte count | raw UTF-8 |
| `'` | name | opaque name | — |
| `^` | scalar | forward delta | — |
| `.` | sized | segment byte count | values |
| `[` `]` | paired | — | values |
| `{` `}` | paired | — | key-value pairs |
| `#` | index | (count<<3)\|(width-1) | pointer table |

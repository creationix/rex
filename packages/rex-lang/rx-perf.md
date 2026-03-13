# rx Performance Notes

Goal: prove rexc is a well-designed format by building fast, streaming tooling with nice output (highlighting, tree views).

## Proposed Low-Level Parser API

Design constraints:
- Near-zero heap allocations per node visited
- Must work well in both JS/TS and LuaJIT (simple functions, no closures, no object creation)
- Lua-style iterator protocol: call a function repeatedly, passing mutable state
- Use short interned strings for type tags (cheap as integers in both JS and LuaJIT, but readable in debuggers)

### Core: Cursor-based parsing

A **cursor** is a mutable struct (or just a few local variables) that the parser fills in. In JS this is a reusable object; in Lua it could be a few upvalues or table fields.

```ts
// ── Runtime representation ──
// At runtime, Cursor is a single mutable object with 8 monomorphic fields.
// All numbers are always numbers — stable hidden class. No string fields.
interface Cursor {
  readonly data: Uint8Array;    // buffer reference — set once, never changes
  left: number;        // start of this node in buffer (output)
  right: number;       // end of this node in buffer (input to read())
  tag: Tag;            // node type (interned string)
  val: number;         // tag-dependent (see union below)
  ixWidth: number;     // index entry width in bytes, 0 = no index
  ixCount: number;     // number of index entries
  schema: number;      // > 0: right-offset of schema node (ptr or ref), 0: none
}

// ── TypeScript discriminated union ──
// For type-safe access after read(). Narrow via c.tag.
type Tag = CursorState["tag"];
type CursorState =
  | { tag: "int";   val: number }    // val = signed integer (zigzag decoded)
  | { tag: "float"; val: number }    // val = decoded float
  | { tag: "str";   val: number }    // val = byte length; raw UTF-8 at data[left..left+val)
  | { tag: "ref";   val: number }    // val = byte length; ref name at data[left+1..left+1+val)
  | { tag: "true" }                  // no val/ref needed
  | { tag: "false" }                 // no val/ref needed
  | { tag: "null" }                  // no val/ref needed
  | { tag: "undef" }                 // no val/ref needed
  | { tag: "array"; val: number; ixWidth: number; ixCount: number }
  | { tag: "object"; val: number; ixWidth: number; ixCount: number;
      schema: number }               // schema > 0: right-offset of schema node, 0: none
  | { tag: "ptr";   val: number }    // val = target offset into buffer
  | { tag: "chain"; val: number };   // val = content boundary
```

8 fields, all monomorphic numbers (plus `data` and `tag`). No string fields — zero allocations per `read()` for all types.

- `"str"` and `"ref"` both store byte length in `val`. Content bytes differ by 1:
  - `"str"`: body at `data[left..left+val)` (body is leftmost, suffix is length+`,`)
  - `"ref"`: name at `data[left+1..left+1+val)` (leftmost byte is `'` tag, name follows)
  - `readStr`, `strEquals`, `strCompare` handle this internally — callers don't need to know.
- `schema` is only used when `tag === "object"`:
  - `0` = no schema (inline keys, interleaved key/value pairs)
  - `> 0` = right-offset of a schema node in the buffer. `read()` at that offset yields either a `"ptr"` (follow to key list in same buffer) or a `"ref"` (look up in pre-encoded refs, point cursor at that buffer). Resolution is the caller's job — the parser just stores the offset.

### read(c)

The workhorse. Reads one node ending at `c.right`, fills in all cursor fields, and sets `c.left` to the start of this node. Returns the tag (also stored in `c.tag`) so callers can branch on it without reading a field.

```ts
function read(c: Cursor): Tag
```

**`read()` is structure-only** — it finds boundaries and classifies the node, but defers string decoding:

- **int**: decodes b64 + zigzag, stores final signed integer in `val`. No allocation.
- **float**: decodes nested integer + exponent, stores final float in `val`. No allocation.
- **str**: stores byte length in `val`. Raw UTF-8 stays in the buffer at `data[left..left+val)`. Caller uses `readStr(c)` to decode or `strEquals(c, target)` to compare without decoding. **0 allocations.**
- **ref**: builtin refs (`'t`, `'f`, `'n`, `'u`) resolve to `"true"`, `"false"`, `"null"`, `"undef"` tags. `'inf`/`'nif`/`'nan` become `"float"` with `Infinity`/`-Infinity`/`NaN` in `val`. User refs become `tag === "ref"` with `val` = byte length of name, raw bytes at `data[left+1..left+1+val)` (skips `'` tag byte). **0 allocations.**
- **array/object**: `val` = content boundary. Parses index and schema metadata. 0 allocations.
- **ptr**: `val` = target offset. Resolution is just `c.right = c.val; read(c)`.
- **true/false/null/undef**: tag says it all, no fields needed.

**Allocation profile**: 0 for everything. Truly zero-alloc `read()`.

Usage:
```ts
const c = makeCursor(data);   // allocate once, c.right = data.length
read(c);                       // parse root node
// c.tag, c.left, c.right, c.val, etc. are now populated
```

### Iterating containers

For arrays and objects, after `read()` gives you `ARRAY` or `OBJECT`, you iterate children by repeatedly calling `read()` while adjusting `cursor.right`:

```ts
// After read() returned "array" or "object":
const end = c.left;       // save container's left boundary
let right = c.val;        // content boundary

// Iterate children (right-to-left in the buffer)
while (right > end) {
  c.right = right;
  read(c);
  // process c.tag, c.val, c.ref, etc.
  right = c.left;         // advance to next child
}
```

For objects without a schema, children alternate: value, key, value, key (right-to-left). For objects with a schema, only values appear in the content — keys come from the schema.

This is the Lua-style iterator: no generator, no closure, no allocated iterator object. Just a loop calling `read()` with shifting boundaries.

### Two-cursor pattern for objects

Object iteration needs two nodes at once (key + value). Use two cursors:

```ts
const k = makeCursor(data);  // key cursor (shares same data)
const v = makeCursor(data);  // value cursor

// After read(c) returned "object" with no schema:
let right = c.val;
const end = c.left;
while (right > end) {
  k.right = right;
  read(k);                   // read key
  v.right = k.left;
  read(v);                   // read value
  // process k and v
  right = v.left;
}
```

### Index-based random access

For containers with indexes, jump to the Nth child without scanning:

```ts
function seekChild(c: Cursor, index: number): void {
  // c must be a parsed "array" or "object" with ixWidth > 0
  // Read the offset from the index table, set c.right, call read()
}
```

This is how selector resolution avoids scanning entire arrays.

### String comparison (zero-alloc)

For selector traversal and key lookup, compare raw bytes without decoding:

```ts
// Equality check: does cursor's string match target?
function strEquals(c: Cursor, target: string): boolean {
  // Fast reject: check c.val (byte length) against target's UTF-8 byte length
  // Then compare c.data[c.left..c.left+c.val) byte-by-byte
  // Zero allocations
}

// Ordering: compare cursor's string against target (like strcmp)
// Returns <0, 0, or >0. Used for binary search on sorted object keys.
function strCompare(c: Cursor, target: string): number {
  // Lexicographic byte compare on raw UTF-8
  // UTF-8 byte order === Unicode codepoint order, so this is correct for all strings
  // Zero allocations
}
```

In LuaJIT both compare against Lua string bytes directly (interned, known length — same fast-path).

### Pointer resolution

Pointers are just offsets into the same buffer. Resolution is free:

```ts
if (c.tag === "ptr") {
  c.right = c.val;  // val holds the target offset
  read(c);           // re-parse at the target location
}
```

### makeCursor helper

```ts
function makeCursor(data: Uint8Array): Cursor {
  return { data, left: 0, right: data.length, tag: "null", val: 0,
           ixWidth: 0, ixCount: 0, schema: 0 };
}
```

One allocation at startup. Reused for the entire traversal. Multiple cursors sharing the same `data` is fine — they just point at different positions in the same buffer.

### Forward and reverse logical iteration

rexc is read right-to-left in byte order. Children are stored so that the **natural read direction** (decreasing byte offsets) yields them in their **original logical order** (first child first). So the basic `while (right > end)` loop iterates forward logically — no reversal needed.

**Forward (natural)**: iterate by decreasing byte offset. This is the default and requires no extra work.

**Reverse logical order**: sometimes needed (e.g. emitting rexc output where bytes must be written left-to-right in the buffer, which is reverse logical order). This is unnatural — you can't iterate backwards without either an index or a pre-scan.

**Indexed containers (ixWidth > 0):** Direct random access via `seekChild(c, i)`. Count is `c.ixCount`.

**Non-indexed containers:** Forward iteration is free (natural read order). For random access or reverse logical iteration, a pre-scan collects child `right` boundaries into a caller-owned array. The return value is the count:

```ts
// Collect child right-boundaries into a caller-owned array (logical order).
// Returns the number of entries written.
// The array is reused across calls — caller pre-allocates and grows as needed.
function collectChildren(container: Cursor, offsets: number[]): number

// Then access by logical index:
const count = collectChildren(container, offsets);
c.right = offsets[i];
read(c);

// Or iterate in reverse logical order (for rexc byte output):
for (let i = count - 1; i >= 0; i--) {
  c.right = offsets[i];
  read(c);
  // emit — bytes go left-to-right, which is reverse logical order
}
```

For objects without a schema, the offsets alternate key/value boundaries. The caller reads pairs at `offsets[i]` (key) and `offsets[i+1]` (value), stepping by 2.

The array is allocated once and reused across containers. For nested depth-first traversal, a single array works since you finish the inner container before resuming the outer one.

### Object key lookup (for selector resolution)

Finding a key in an object without materializing all entries:

```ts
// Find a key in an object. Fills c with the value node if found.
// Returns true if found, false if not.
// Strategy depends on the object:
//   - Sorted + indexed: binary search via strCompare + seekChild → O(log n)
//   - Unsorted or non-indexed: linear scan via strEquals → O(n)
// Zero allocations either way.
function findKey(c: Cursor, container: Cursor, target: string): boolean
```

The caller doesn't choose the strategy — `findKey` inspects the container's index metadata and picks the best path. The encoder can opt into the fast path by sorting keys and adding an index (which it already does for large objects).

### Byte range extraction (for rexc→rexc passthrough)

After resolving a selector to a cursor, the raw bytes are the output:

```ts
// Get the raw rexc bytes for the node at cursor position
function rawBytes(c: Cursor): Uint8Array {
  return c.data.subarray(c.left, c.right);  // zero-copy view
}
```

### Summary: allocation profile

| Operation | Allocations |
|-----------|------------|
| `makeCursor()` | 1 object (reused) |
| `read()` on any tag | 0 (always zero-alloc) |
| `readStr()` | 1 string (UTF-8 decode on demand) |
| `strEquals()` | 0 (byte compare) |
| `strCompare()` | 0 (byte compare) |
| `findKey()` | 0 (binary search or linear scan, no decode) |
| `seekChild()` | 0 |
| `collectChildren()` | 0 (fills caller-owned array) |
| `rawBytes()` | 0 (subarray view) |

For a full tree walk with output: allocations = cursors + 1 offsets array + 1 `readStr` per string/key emitted.
For selector resolution: allocations = cursors + 0 `readStr` (strEquals only). Truly zero-alloc traversal.

### Complete API surface

```ts
// Setup
function makeCursor(data: Uint8Array): Cursor     // includes data ref, right = data.length

// Core parsing
function read(c: Cursor): Tag                      // reads node at c.right, returns tag

// Container iteration (native right-to-left order)
// → just a while loop: set c.right, call read(c), advance c.right = c.left

// Random access (indexed: via index table, non-indexed: pre-scan into caller array)
function seekChild(c: Cursor, index: number): void          // indexed containers, O(1)
function collectChildren(container: Cursor, offsets: number[]): number  // non-indexed, returns count

// String handling
function strEquals(c: Cursor, target: string): boolean   // equality, 0 alloc
function strCompare(c: Cursor, target: string): number   // ordering (<0, 0, >0), 0 alloc
function readStr(c: Cursor): string                      // force UTF-8 decode

// Object key lookup
function findKey(c: Cursor, container: Cursor, target: string): boolean

// Raw bytes
function rawBytes(c: Cursor): Uint8Array                 // data.subarray(c.left, c.right)

// Pointer resolution
// → just set c.right = c.val and call read(c) again
```

### How rx CLI uses each API

| rx operation | API calls |
|-------------|-----------|
| `rx data.rexc -s .foo --rexc` | `read` → `findKey` → `rawBytes` → write |
| `rx data.rexc -s .[3] --rexc` | `read` → `seekChild` → `rawBytes` → write |
| `rx data.rexc --tree` | `read` → `collectChildren` → loop `readChild` + `read` recursively → emit formatted lines |
| `rx data.rexc --json` | same as tree but emit JSON tokens |
| `rx data.rexc --rexc --color` | `read` → `collectChildren` → loop `readChild` → `rawBytes` per word → `highlightRexc` → write |
| `rx data.rexc --completions .` | `read` → iterate keys with `strEquals` (or just `readStr` for display) |
| `rx data.rexc -s .foo --completions .` | `read` → `findKey` → iterate selected node's keys |

---

## Current Architecture

```
read file → parse to JS objects → format (stringify/highlight) → write
```

Every path materializes the full JS object graph. This is the bottleneck for large files.

## Target Architecture

```
read file → Uint8Array buffer → walk RxNode tree → emit output
```

No JS object materialization. The buffer is the data structure. Future: mmap the buffer for zero-copy file access.

## Fast Paths (by output format)

### rexc → rexc (passthrough)

**No selector:** `cat` the file. Nothing to do.

**With selector:** Walk `RxNode` tree to the selected node using `get()` + `getEntries()`/`getEach()`, then `data.slice(node.left, node.right)`. Zero re-encoding — just byte slicing.

### rexc → rexc (streaming re-encode)

For cases where re-encoding is needed (e.g. adding indexes, deduplication):

1. Read buffer, `get()` the root node
2. For each container, iterate first level to collect child node bounds (pointer arithmetic only)
3. Walk children in **reverse order** (last child first = leftmost bytes first) for left-to-right output
4. Emit each complete rexc word via `onChunk`

The reverse-order walk is necessary because rexc is right-to-left (values before their containers). Random access via indexes makes this cheap for large containers.

### rexc → highlighted rexc

Depends on the serializer emitting **complete rexc words** per chunk. Currently chunks are byte-level fragments — a string body might be split across chunks, and a prefix might land in a different chunk than its tag.

**Needed in rexc.ts:** An `onWord` callback (or modify `onChunk` to emit full words). Each word = `<prefix><tag><body>` as a complete unit. Then `highlightRexc(word)` works per-chunk.

### rexc → tree (pretty-print)

Streaming tree output requires a visitor-based walker:

```ts
interface TreeVisitor {
  onPrimitive(node: RxPrimitive, depth: number): void;
  onKey(node: RxNode, depth: number): void;
  onArrayStart(node: RxArray, depth: number): void;
  onArrayEnd(node: RxArray, depth: number): void;
  onObjectStart(node: RxObject, depth: number): void;
  onObjectEnd(node: RxObject, depth: number): void;
}
```

The visitor walks the `RxNode` tree depth-first and emits formatted text (indentation, colors) without building JS objects. Each callback writes directly to stdout.

Object/array children need the same reverse-order traversal for left-to-right output. `highlightLine()` can be applied per-line as lines are emitted.

**Inline vs block decision:** The current `rex.stringify` decides whether to inline `{a: 1 b: 2}` or use multi-line format based on estimated width. The streaming visitor needs the same heuristic, but can compute it from the byte range size as a proxy (small byte range → likely fits on one line).

### rexc → JSON

Similar to tree output — visitor-based walker that emits JSON tokens directly. Simpler than tree since JSON has no inline/block decision (always indented).

### json → rexc

Already streaming via `rexc.stringify({ onChunk })` once the JSON is parsed. The JSON parse itself is inherently buffered (`JSON.parse`), but that's unavoidable without a streaming JSON parser.

## Selector Fast Path

For any output format, the selector can be resolved at the `RxNode` level before any output begins:

```ts
function selectNode(ctx: RxContext, root: RxNode, segments: Segment[]): RxNode {
  let node = root;
  for (const seg of segments) {
    if (seg.type === "key") {
      // node must be object — scan getEntries() for matching key
      // resolve key node to string, compare with seg.name
    } else {
      // node must be array — use index if available, else scan getEach()
    }
  }
  return node;
}
```

For rexc→rexc, the selected node's byte range is the output. For other formats, the selected node becomes the root for the visitor walk. Either way, only the selected subtree is traversed.

## Completions Fast Path

Shell completions currently do a full `parse()` to JS objects, then walk the object tree. This could use the `RxNode` walker instead — enumerate keys of an object node by scanning `getEntries()` and resolving only the key nodes to strings. Value nodes are never materialized.

## High-Level API: Proxy/Metatable Wrapper

The cursor API is fast but verbose. For normal code that just wants to read data, a Proxy-based wrapper (JS) or metatable wrapper (Lua) gives native syntax backed by lazy cursor reads.

### JS Proxy

```ts
// Optional pre-encoded ref buffers: maps ref name → rexc Uint8Array
type Refs = Record<string, Uint8Array>;

const data = rexc.open(buffer);                    // no refs
const data = rexc.open(buffer, refs);              // with ref support

data.routes[0].op               // → triggers get traps → findKey + seekChild + readStr
data.routes.length              // → ixCount or scan
for (const [k, v] of data.config) { ... }  // → iterate entries
```

Each property access returns a new Proxy wrapping a cursor positioned at the child node. Primitives are resolved on access — `data.timeout` returns a number, `data.name` returns a string. Containers return nested Proxies.

### Internal representation

Each Proxy wraps a lightweight handle — not a full cursor, just `(data, right)`:

```ts
// Frozen pair — one allocation per node access, cached on repeat access
type Handle = { data: Uint8Array; right: number };
```

The Proxy `get` trap creates a temporary cursor, calls `read()` + `findKey()`/`seekChild()`, and returns the resolved value (primitive) or a new Proxy (container). The temporary cursor is from a shared pool — no allocation per access.

### Caching

Repeat access to the same key can cache the resolved `right` offset:

```ts
data.routes[0].op  // first access: findKey("routes") → seekChild(0) → findKey("op")
data.routes[0].op  // second access: cached handle chain, just readStr at known offset
```

Cache is a WeakMap from Proxy → Map<key, Handle>. Only allocated on first repeat access.

### Escape hatch to cursor API

```ts
const HANDLE = Symbol("rexc.handle");
const handle = data.routes[HANDLE];  // get the raw (data, right) pair
const c = makeCursor(handle.data);
c.right = handle.right;
read(c);  // now use cursor API for performance-critical code
```

### Lua metatable equivalent

```lua
local data = rexc.open(buffer)
data.routes[1].op               -- __index → findKey + seekChild + readStr
#data.routes                    -- __len → childCount
for k, v in pairs(data.config) do ... end  -- __pairs → iterate entries
```

Same pattern: `__index` does lazy cursor reads, returns wrapped tables for containers, primitives for scalars. Each wrapped value is a table with `{data=buf, right=offset}` plus a metatable.

### Proxy internal state

Each Proxy wraps a **frozen snapshot** of a parsed node — the minimum info needed to re-enter the cursor API:

```ts
// Stored per Proxy instance (frozen, shared with cache)
type NodeInfo = {
  data: Uint8Array;  // buffer (may differ from main buffer for ref-resolved nodes)
  right: number;     // right-offset of this node
  // Cached from the initial read():
  tag: Tag;
  val: number;
  left: number;
  ixWidth: number;
  ixCount: number;
  schema: number;
};
```

This is 8 fields — same shape as a Cursor, but frozen (never mutated). Created once when the Proxy is first accessed, then reused. The Proxy itself is the allocation; the NodeInfo can be the Proxy's target object.

A shared mutable Cursor is used as a scratch pad for all trap operations:

```ts
const scratch = makeCursor(mainData);       // one per open() call, reused across all traps
const refs: Refs | undefined;               // optional, passed to open()
```

### Proxy traps in detail

**`get(target: NodeInfo, prop: string | symbol)`** — the core trap

```
prop is a number string ("0", "1", ...):
  → array: seekChild(scratch, container=target, parseInt(prop))
  → object: seekChild for nth entry (key+value pair)
  → return wrap(scratch) or resolve primitive

prop is a string key:
  → object: findKey(scratch, container=target, prop)
    → found: read value at scratch.left, return wrap(scratch) or resolve primitive
    → not found: return undefined
  → array: special names only (see below)

prop === "length":
  → array: return ixCount (if indexed) or scan to count children
  → object: same (number of entries)

prop === Symbol.iterator:
  → array: return generator that yields wrapped values
  → object: return generator that yields [key, value] pairs

prop === Symbol.toPrimitive or "valueOf" or "toString":
  → resolve the node to a JS primitive (readStr for str, val for int/float, etc.)

prop === HANDLE (escape hatch symbol):
  → return the NodeInfo directly for cursor API access
```

Implementation for string key lookup on objects:
```ts
// Reuse scratch cursor
scratch.data = target.data;
scratch.right = target.right;
scratch.left = target.left;
scratch.val = target.val;
scratch.ixWidth = target.ixWidth;
scratch.ixCount = target.ixCount;
scratch.schema = target.schema;
scratch.tag = target.tag;

if (findKey(scratch, scratch /* container state from target */, prop)) {
  // scratch now points at the value node
  return wrapOrResolve(scratch);
}
return undefined;
```

**`ownKeys(target: NodeInfo)`** — enumerate keys

```
object: iterate all key nodes via while loop, readStr each key → return string[]
array: return ["0", "1", ..., String(count-1), "length"]
```

This allocates the key array. Unavoidable for `Object.keys()` / `for...in`. For streaming output, use `Symbol.iterator` instead.

```ts
// For objects:
const keys: string[] = [];
const k = makeCursor(target.data);  // or use scratch
let right = target.val;
while (right > target.left) {
  k.right = right;
  read(k);  // key
  keys.push(readStr(k));
  k.right = k.left;
  read(k);  // skip value
  right = k.left;
}
return keys;
```

**`getOwnPropertyDescriptor(target: NodeInfo, prop: string)`**

Required for `ownKeys` to work. Returns `{configurable: true, enumerable: true, value: get(target, prop)}` for existing keys. For performance, can defer the `value` computation (V8 often doesn't need it for `Object.keys()`).

**`has(target: NodeInfo, prop: string | symbol)`** — `"key" in obj`

```
object: findKey(scratch, target, prop) → boolean (zero-alloc, no value read)
array: parseInt(prop) < count
```

**`deleteProperty`, `set`, `defineProperty`** — mutation traps

All throw `TypeError("rexc data is read-only")`.

**`isExtensible`, `preventExtensions`**

Return `false` / no-op. The object is sealed.

**`getPrototypeOf`**

Returns `null` (no prototype chain).

### Helper: wrapOrResolve

After `read(scratch)` positions the cursor at a node, decide what to return:

```ts
function wrapOrResolve(c: Cursor): unknown {
  // Follow pointers transparently
  while (c.tag === "ptr") {
    c.right = c.val;
    read(c);
  }
  // Resolve refs via pre-encoded ref map
  if (c.tag === "ref") {
    if (!refs) return undefined;  // no refs map → unresolvable ref
    const refBuf = refs[readStr(c)];
    if (!refBuf) return undefined;
    c.data = refBuf; c.right = refBuf.length; read(c);
    // c now points into the ref's buffer — recursion handles nested ptrs/refs
  }
  // Primitives → return JS value directly (no Proxy)
  switch (c.tag) {
    case "int": case "float": return c.val;
    case "str": return readStr(c);
    case "true": return true;
    case "false": return false;
    case "null": return null;
    case "undef": return undefined;
  }
  // Containers → return new Proxy wrapping a NodeInfo snapshot
  const info: NodeInfo = {
    data: c.data, right: c.right, tag: c.tag, val: c.val,
    left: c.left, ixWidth: c.ixWidth, ixCount: c.ixCount, schema: c.schema,
  };
  return new Proxy(info, handler);
}
```

### Allocation summary per trap

| Trap | Allocations |
|------|------------|
| `get` on primitive value | 1 string (for "str") or 0 |
| `get` on container value | 1 NodeInfo + 1 Proxy |
| `get` cache hit | 0 (return cached Proxy) |
| `ownKeys` | 1 string[] + N strings |
| `has` | 0 (findKey + strEquals) |
| `Symbol.iterator` | 1 generator + per-yield: 1 string or 1 Proxy |

### What the Proxy layer handles

- **Pointer resolution**: transparent — if a value is `"ptr"`, follow it before returning
- **Ref resolution**: if a value is `"ref"`, look up in `refs: Record<string, Uint8Array>`, switch `data` buffer
- **Schema resolution**: for schema objects, zip keys and values from different positions/buffers
- **Type coercion**: `"int"`/`"float"` → number, `"str"` → string, `"true"`/`"false"` → boolean, `"null"` → null, `"undef"` → undefined
- **Iteration**: `for..of` / `Object.keys()` / `Object.entries()` → iterate keys/values via cursor
- **Length**: `.length` on arrays → `childCount`
- **JSON.stringify()**: works via `toJSON` trap — materializes the subtree

### What the Proxy layer does NOT handle

- Streaming output (use cursor API)
- Zero-alloc traversal (use cursor API)
- Byte-slicing passthrough (use cursor API + `rawBytes`)
- Mutation (rexc is read-only)

### Performance expectations

- Single property access: ~3 cursor reads (findKey is the bottleneck), 1-2 allocations (Proxy + string)
- Deep path like `data.a.b.c.d`: ~4 findKey calls, 4 Proxy allocations (or 0 if cached)
- Full iteration of 1000-entry array: 1000 read() calls + 1000 Proxy allocations (or 1000 resolve calls if accessing primitives)
- For hot loops: drop to cursor API via `HANDLE` escape hatch

This is fine for CLI tools, config reading, request routing. For bulk data processing, use the cursor API directly.

## Implementation Order

1. **Cursor API** — `read()`, `seekChild()`, `findKey()`, `strEquals()`, `strCompare()`, `readStr()`, `rawBytes()`. Foundation for everything else.
2. **Byte-slicing selector** — rexc input + rexc output + selector → just slice bytes via cursor API. Biggest win for the common `rx data.rexc -s .path --rexc` case.
3. **Full-word chunks** — modify `rexc.ts` encoder to emit complete words per `onChunk`. Enables streaming highlighted rexc output.
4. **Streaming tree/JSON visitors** — walk cursor tree and emit formatted text directly. Eliminates JS object materialization for `rx data.rexc` output.
5. **Proxy wrapper** — high-level API for normal code. Built on cursor API.
6. **mmap** — replace `readFile` with mmap for zero-copy buffer access. Drop-in replacement for the Uint8Array buffer.

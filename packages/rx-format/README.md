# @creationix/rx

Inspect, convert, and filter REXC and JSON data.

## Install

```sh
bun add -g @creationix/rx
```

## CLI Usage

```sh
rx data.rexc                         # pretty-print as tree
rx data.rexc --to json               # convert rexc → JSON
rx data.json --to rexc               # convert JSON → rexc
cat data.rexc | rx                   # read from stdin (auto-detect)
rx data.rexc -s routes 0 op          # select a sub-value
rx data.rexc --to json -o out.json   # write to file
```

### Input

| Form | Description |
|------|-------------|
| `<file>` | Read from file (format auto-detected by extension) |
| `-` | Read from stdin explicitly |
| (no args, piped) | Read from stdin automatically |

### Format control

| Flag | Description |
|------|-------------|
| `--from json\|rexc` | Force input format (default: auto-detect) |
| `--to json\|rexc\|tree` | Output format |
| `-j`, `--json` | Shortcut for `--to json` |
| `-r`, `--rexc` | Shortcut for `--to rexc` |
| `-t`, `--tree` | Shortcut for `--to tree` |

Format is auto-detected from file extension (`.json`, `.rexc`) or by content sniffing on stdin. Output defaults to tree view on a TTY, JSON when piped.

### Encoding

| Flag | Description |
|------|-------------|
| `--indexes <n>` | Add indexes to containers with >= n entries |
| | Use `false` to disable indexes entirely |

### Filtering

| Flag | Description |
|------|-------------|
| `-s`, `--select <seg>...` | Space-delimited selector segments (e.g. `-s foo bar 0 baz`) |

### Output

| Flag | Description |
|------|-------------|
| `-o`, `--out <path>` | Write to file instead of stdout |
| `--color` | Force ANSI color |
| `--no-color` | Disable ANSI color |
| `-h`, `--help` | Show help message |

### Shell completions

```sh
rx --completions setup [zsh|bash]    # install tab completions
rx --completions zsh|bash            # print completion script to stdout
```

### Run without installing

```sh
bun run rx data.rexc                 # from repo root
```

## Programmatic API

The cursor-based parser provides zero-allocation reads over REXC binary data.

```ts
import {
  makeCursor,
  read,
  readStr,
  resolveStr,
  strEquals,
  strCompare,
  findKey,
  seekChild,
  collectChildren,
  rawBytes,
} from "@creationix/rx";
```

### Cursor basics

```ts
const c = makeCursor(data);   // allocate once, c.right = data.length
read(c);                       // parse root node
// c.tag, c.left, c.right, c.val are now populated
```

### Iterating containers

```ts
// After read() returned "array" or "object":
const end = c.left;
let right = c.val;
while (right > end) {
  c.right = right;
  read(c);
  // process c.tag, c.val, etc.
  right = c.left;
}
```

### Random access

```ts
// Indexed containers: O(1) access
seekChild(child, container, index);

// Non-indexed: collect boundaries first, then access by index
const offsets: number[] = [];
const count = collectChildren(container, offsets);
c.right = offsets[i];
read(c);
```

### String handling

```ts
readStr(c)                    // decode string at cursor to JS string (1 allocation)
strEquals(c, "target")        // zero-alloc equality check
strCompare(c, "target")       // zero-alloc ordering (<0, 0, >0)
resolveStr(c)                 // follow pointers and concatenate chains
```

### Object key lookup

```ts
const v = makeCursor(data);
if (findKey(v, container, "key")) {
  // v now points at the value node
}
```

### Raw bytes

```ts
rawBytes(c)                   // zero-copy Uint8Array view of node bytes
```

See [rx-perf.md](rx-perf.md) for detailed architecture notes and the Proxy wrapper design.

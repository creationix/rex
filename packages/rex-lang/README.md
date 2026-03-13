# @creationix/rex

Rex compiler and parser package.

## Install

```sh
bun add -g @creationix/rex
```

## Publish to npm

From `packages/rex-lang`:

```sh
npm whoami
bun run prepublishOnly
npm publish --access public
```

Or one-off dry run:

```sh
bun run pack:dry-run
```

## CLI

```sh
rex --help
rex --expr "when x do y end"
rex --file input.rex
cat input.rex | rex
rex --expr "a and b" --ir
rex --expr "x = method + path x" --minify-names --dedupe-values
```

Run without installing globally:

```sh
bunx @creationix/rex --help
bunx @creationix/rex --expr "when x do y end"

npx -y @creationix/rex -- --help
npx -y @creationix/rex -- --expr "when x do y end"
```

`npx` works with Node.js alone (v22.18+). `bunx` works with Bun alone.

## Data Tool (`rx`)

`rx` inspects, converts, and filters REXC and JSON data.

```sh
rx data.rexc                         # pretty-print as tree
rx data.rexc --to json               # convert rexc → JSON
rx data.json --to rexc               # convert JSON → rexc
cat data.rexc | rx                   # read from stdin (auto-detect)
rx -s .routes[0].op data.rexc        # select a sub-value
rx data.rexc --to json -o out.json   # write to file
```

Format is auto-detected from file extension (`.json`, `.rexc`) or by content sniffing on stdin. Override with `--from json|rexc`.

Output defaults to tree view on a TTY, JSON when piped. Override with `--to json|rexc|tree` (shortcuts: `-j`, `-r`, `-t`).

Run without installing:

```sh
bun run rx data.rexc                             # from repo root
bun run packages/rex-lang/rx-cli.ts data.rexc    # from anywhere
```

## Programmatic API

```ts
import { compile, parseToIR, optimizeIR, encodeIR } from "@creationix/rex";

const source = "when x do y else z end";
const encoded = compile(source);
const optimized = compile(source, { optimize: true, minifyNames: true, dedupeValues: true });

const ir = parseToIR(source);
const optimizedIR = optimizeIR(ir);
const reEncoded = encodeIR(optimizedIR);
```

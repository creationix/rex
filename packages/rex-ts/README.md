# rex-ts

TypeScript tagged template literals for generating [Rex](../../language.md) middleware. Compile Rex source at build time with interpolated values, optional domain type checking, and REXC bytecode output.

## Quick Start

```ts
import { rex, rexc, route } from "@creationix/rex-ts";

// Generate Rex source with interpolated JS values
const source = rex`status = ${200}`;
// → 'status = 200'

// Compile directly to REXC bytecode
const bytecode = rexc`
  when method == "GET" do
    status = ${200}
  end
`;

// Get both source and bytecode
const r = route`status = ${200}`;
r.source;   // 'status = 200'
r.bytecode; // compiled REXC string
```

## Value Interpolation

Interpolated values are converted to Rex literals automatically:

```ts
rex`x = ${42}`                    // → 'x = 42'
rex`x = ${"hello"}`              // → 'x = "hello"'
rex`x = ${true}`                 // → 'x = true'
rex`x = ${null}`                 // → 'x = null'
rex`x = ${[1, 2, 3]}`           // → 'x = [1, 2, 3]'
rex`x = ${{ ok: true }}`        // → 'x = {ok: true}'
```

Strings are escaped for safe embedding:

```ts
rex`x = ${'say "hi"'}`          // → 'x = "say \\"hi\\""'
```

## API

### `rex` (tagged template) → `string`

Produces Rex source code. Interpolations become Rex literals.

### `rexc` (tagged template) → `string`

Produces compiled REXC bytecode. Same interpolation as `rex`, then compiles.

### `route` (tagged template) → `RexRoute`

Returns `{ source, bytecode, diagnostics }`. Useful when you need both the source (for debugging) and bytecode (for deployment).

### `toRex(value)` → `string`

Convert a JS value to its Rex source representation. Used internally by the tagged templates, but exported for direct use.

### `compile(source)` → `string`

Re-exported from `@creationix/rex`. Compile Rex source to REXC bytecode.

## Domain Type Checking

For production use, define a `.rexd` domain interface describing your extern bindings and get type-checked compilation:

```ts
import { createDomain } from "@creationix/rex-ts";

const domain = createDomain(`
  extern method = string
  extern path = string
  extern query = {*: string}
  extern mut status = integer
  extern mut headers = {mut *: string}
`);
```

### `domain.rexc` (tagged template) → `string`

Compile with domain-aware function resolution and type checking. In strict mode (default), throws on type errors:

```ts
const bytecode = domain.rexc`
  when method == "GET" and path == "/api/users" do
    status = 200
    headers.content-type = "application/json"
  end
`;
```

### `domain.route` (tagged template) → `RexRoute`

Same as `domain.rexc` but returns `{ source, bytecode, diagnostics }`:

```ts
const r = domain.route`
  when method == "POST" do
    status = 201
  else
    status = 405
  end
`;
console.log(r.diagnostics); // type warnings/errors
```

### `domain.check(source)` → `Diagnostic[]`

Type-check Rex source without compiling. Returns an array of diagnostics:

```ts
const diags = domain.check("status = 200");
for (const d of diags) {
  console.log(`[${d.kind}] ${d.start}-${d.end}: ${d.message}`);
}
```

### `domain.compile(source)` → `string`

Compile with domain function resolution but without type checking.

### `createDomain(rexd, options?)` → `RexDomain`

Create a domain from a `.rexd` interface string.

Options:
- `strict` (default: `true`) — throw on type errors in `rexc` and `route`. Set `false` to collect diagnostics without throwing.

```ts
// Non-strict: collect diagnostics without throwing
const domain = createDomain(rexdSource, { strict: false });
const r = domain.route`status = "oops"`;
console.log(r.diagnostics); // [{kind: "error", ...}]
console.log(r.bytecode);    // still compiled (may be wrong)
```

## CDN Edge Middleware Example

The intended use case: a JS framework generates Rex middleware at build time, emitting REXC bytecode strings for deployment to a CDN's edge servers.

```ts
import { createDomain } from "@creationix/rex-ts";

// Define the CDN's edge middleware interface
const edge = createDomain(`
  extern method = string
  extern path = string
  extern query = {*: string}
  extern req = {headers: {*: string}, body: string}
  extern mut status = integer
  extern mut headers = {mut *: string}
  extern path-match(pattern: string) -> some
`);

// Generate routes in your framework's build step
const routes = [
  {
    path: "/api/users",
    middleware: edge.route`
      when method == "GET" do
        status = 200
        headers.content-type = "application/json"
      else when method == "POST" do
        status = 201
        headers.content-type = "application/json"
      else
        status = 405
        headers.x-error = "method not allowed"
      end
    `,
  },
  {
    path: "/api/health",
    middleware: edge.route`
      status = 200
      headers.content-type = "text/plain"
    `,
  },
];

// Emit bytecode for the CDN config
for (const r of routes) {
  console.log(`${r.path}: ${r.middleware.bytecode.length} bytes`);
  cdnConfig.addEdgeMiddleware(r.path, r.middleware.bytecode);
}
```

## Native Bindings

Domain features (`createDomain`) require the `rex-node` native module for Rust-powered compilation and type checking. If the native module isn't available, `createDomain` falls back gracefully:

- `domain.compile()` uses the pure-TS compiler (no domain function resolution)
- `domain.check()` returns an empty array (no type checking)
- `domain.rexc` and `domain.route` still work, just without type safety

Build the native module:

```sh
cd packages/rusty-rex
cargo build -p rex-node --release
cp target/release/librex_node.dylib crates/rex-node/rex-node.darwin-arm64.node
```

# HTTP Domain Rex Samples

This folder contains realistic Rex policy/routing samples with different focus areas.

## Domain host interfaces

- `edge-config` is a host-provided, read-only object for customer-attached project configuration.
- Samples can read values like `edge-config.routing.default-operation-timeout-ms` and `edge-config.routing.response-tag`.
- Secrets should come from host APIs (for example `provider-signing-secret(provider)`), not from inline literals.

## Files

- `routing.rex` — full, end-to-end API routing + middleware pipeline.
- `routing-minimal.rex` — compact baseline router for quick debugging.
- `auth-policies.rex` — auth/authorization strategy matrix.
- `rate-limits.rex` — layered rate limiting and burst control.
- `caching.rex` — cache read/write/etag/stale behavior.
- `webhooks.rex` — signature validation, replay protection, retries.
- `error-mapping.rex` — upstream/internal error normalization.
- `multi-tenant.rex` — tenant routing, quotas, and policy inheritance.
- `events-and-streams.rex` — event ingestion + stream response style policy.
- `harness-demo.rex` — deterministic policy used by sample test vectors.

## Compile all samples

Generates two output files per sample:

- `*.rexc` (debug/normal compile)
- `*.opt.rexc` (optimized compile)

Run from repo root:

```sh
bun samples/http-domain/compile-samples.ts
```

## Test harness (`foo.rex` + `foo.test.rex`)

Use `*.test.rex` files as Rex-authored test vector docs for corresponding program files.

- Each sample now has a matching harness file (`routing.rex` + `routing.test.rex`, etc.).
- `harness-demo.rex` also has a dedicated `harness-demo.test.rex`.

Run all sample tests:

```sh
bun samples/http-domain/run-sample-tests.ts
```

Test doc shape (Rex object):

```rex
{
	program: "optional-override.rex" // optional; defaults foo.test.rex -> foo.rex
	cases: [
		{
			name: "example"
			input: {vars: {method: "GET", path: "/health"}}
			expect: {value: {status: 200}}
		}
	]
}
```

`expect.vars` and `expect.refs` can assert subsets of final runtime state.

All files are standalone Rex documents intended for editor/manual testing.

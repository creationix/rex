/**
 * rex-ts — Tagged template literals for generating Rex middleware.
 *
 * Usage:
 *   import { rex, rexc, createDomain } from '@creationix/rex-ts'
 *
 *   // Simple: generate Rex source with interpolated values
 *   const source = rex`status = ${200}`
 *
 *   // With domain: type-checked compilation
 *   const domain = createDomain(`
 *     extern method = string
 *     extern mut status = integer
 *     extern mut headers = {mut *: string}
 *   `)
 *   const bytecode = domain.rexc`
 *     when method == "GET" do
 *       status = ${200}
 *     end
 *   `
 *   // Throws at build time if the Rex code doesn't type-check
 */

import { compile } from "../../crates/rex-node";

// ── Value interpolation ────────────────────────────────────────────────

/**
 * Convert a JS value to its Rex source representation.
 * Used by the tagged template to safely embed values.
 */
export function toRex(value: unknown): string {
	if (value === null) return "null";
	if (value === undefined) return "undefined";

	switch (typeof value) {
		case "string":
			// Escape for double-quoted Rex string
			return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;

		case "number":
			if (Number.isNaN(value)) return "NaN";
			if (!Number.isFinite(value)) return value > 0 ? "Infinity" : "-Infinity";
			return String(value);

		case "boolean":
			return String(value);

		case "object": {
			if (Array.isArray(value)) {
				return `[${value.map(toRex).join(", ")}]`;
			}
			// Plain object → Rex object literal
			const entries = Object.entries(value as Record<string, unknown>);
			if (entries.length === 0) return "{}";
			const pairs = entries.map(([k, v]) => {
				// Use bare key if it's identifier-like, else quoted
				const key = /^[a-zA-Z_][a-zA-Z0-9_-]*$/.test(k) ? k : `"${k}"`;
				return `${key}: ${toRex(v)}`;
			});
			return `{${pairs.join(", ")}}`;
		}

		default:
			return "undefined";
	}
}

// ── Tagged templates ───────────────────────────────────────────────────

/**
 * Tagged template that produces Rex source code.
 * Interpolated values are converted to Rex literals.
 *
 * ```ts
 * const src = rex`status = ${200}`
 * // → 'status = 200'
 * ```
 */
export function rex(
	strings: TemplateStringsArray,
	...values: unknown[]
): string {
	let result = strings[0]!;
	for (let i = 0; i < values.length; i++) {
		result += toRex(values[i]);
		result += strings[i + 1]!;
	}
	return result;
}

/**
 * Tagged template that compiles Rex source to REXC bytecode.
 * No type checking — use `createDomain().rexc` for type-safe compilation.
 *
 * ```ts
 * const bytecode = rexc`
 *   when method == "GET" do
 *     status = ${200}
 *   end
 * `
 * ```
 */
export function rexc(
	strings: TemplateStringsArray,
	...values: unknown[]
): string {
	const source = rex(strings, ...values);
	return compile(source);
}

// ── Domain (type-checked compilation) ──────────────────────────────────

/** A diagnostic from the Rex type checker. */
export interface Diagnostic {
	kind: "error" | "warning";
	start: number;
	end: number;
	message: string;
}

/** A compiled Rex route with source, bytecode, and diagnostics. */
export interface RexRoute {
	/** Original Rex source */
	source: string;
	/** Compiled REXC bytecode */
	bytecode: string;
	/** Type-check diagnostics (empty if no domain or no issues) */
	diagnostics: Diagnostic[];
}

/** Options for domain compilation. */
export interface DomainOptions {
	/** If true, type errors throw instead of being returned in diagnostics. Default: true. */
	strict?: boolean;
}

/**
 * A domain provides type-checked Rex compilation against a `.rexd` interface.
 *
 * The check/compile functions are optional — they require the `rex-node` native
 * module. If unavailable, compilation falls back to the pure-TS compiler
 * (no type checking, no domain function resolution).
 */
export interface RexDomain {
	/** The raw .rexd source */
	rexd: string;

	/**
	 * Tagged template: compile Rex source to REXC with type checking.
	 * Throws on type errors when strict mode is enabled (default).
	 */
	rexc: (strings: TemplateStringsArray, ...values: unknown[]) => string;

	/**
	 * Build a route: compile + type-check, return source + bytecode + diagnostics.
	 */
	route: (strings: TemplateStringsArray, ...values: unknown[]) => RexRoute;

	/**
	 * Type-check Rex source against this domain. Returns diagnostics.
	 */
	check: (source: string) => Diagnostic[];

	/**
	 * Compile Rex source with domain-aware function resolution.
	 */
	compile: (source: string) => string;
}

// Try to load rex-node native bindings (optional — graceful fallback)
let nativeBindings: {
	compileWithDomain: (source: string, domain: string) => string;
	check: (source: string, domain: string) => Diagnostic[];
} | null = null;

try {
	// rex-node is a native module — may not be available in all environments
	const mod = await import("../../crates/rex-node");
	nativeBindings = {
		compileWithDomain: mod.compileWithDomain,
		check: mod.check,
	};
} catch {
	// Native bindings not available — domain features will use fallback
}

/**
 * Create a domain for type-checked Rex compilation.
 *
 * ```ts
 * const domain = createDomain(`
 *   extern method = string
 *   extern path = string
 *   extern mut status = integer
 *   extern mut headers = {mut *: string}
 * `)
 *
 * const bytecode = domain.rexc`
 *   when method == "GET" do
 *     status = 200
 *   end
 * `
 * ```
 */
export function createDomain(rexd: string, options?: DomainOptions): RexDomain {
	const strict = options?.strict ?? true;

	function domainCompile(source: string): string {
		if (nativeBindings) {
			return nativeBindings.compileWithDomain(source, rexd);
		}
		// Fallback: compile without domain resolution
		return compile(source);
	}

	function domainCheck(source: string): Diagnostic[] {
		if (nativeBindings) {
			return nativeBindings.check(source, rexd);
		}
		// No native bindings — can't type-check
		return [];
	}

	function domainRexc(
		strings: TemplateStringsArray,
		...values: unknown[]
	): string {
		const source = rex(strings, ...values);
		if (strict) {
			const diagnostics = domainCheck(source);
			const errors = diagnostics.filter((d) => d.kind === "error");
			if (errors.length > 0) {
				const messages = errors
					.map((e) => `  [${e.start}-${e.end}] ${e.message}`)
					.join("\n");
				throw new Error(`Rex type errors:\n${messages}\n\nSource:\n${source}`);
			}
		}
		return domainCompile(source);
	}

	function domainRoute(
		strings: TemplateStringsArray,
		...values: unknown[]
	): RexRoute {
		const source = rex(strings, ...values);
		const diagnostics = domainCheck(source);
		if (strict) {
			const errors = diagnostics.filter((d) => d.kind === "error");
			if (errors.length > 0) {
				const messages = errors
					.map((e) => `  [${e.start}-${e.end}] ${e.message}`)
					.join("\n");
				throw new Error(`Rex type errors:\n${messages}\n\nSource:\n${source}`);
			}
		}
		const bytecode = domainCompile(source);
		return { source, bytecode, diagnostics };
	}

	return {
		rexd,
		rexc: domainRexc,
		route: domainRoute,
		check: domainCheck,
		compile: domainCompile,
	};
}

// ── Simple route (no domain) ───────────────────────────────────────────

/**
 * Build a route without type checking.
 * For type-checked routes, use `createDomain().route`.
 */
export function route(
	strings: TemplateStringsArray,
	...values: unknown[]
): RexRoute {
	const source = rex(strings, ...values);
	const bytecode = compile(source);
	return { source, bytecode, diagnostics: [] };
}

// ── Re-exports ─────────────────────────────────────────────────────────

export { compile } from "../../crates/rex-node";

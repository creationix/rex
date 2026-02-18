import { describe, expect, test } from "bun:test";
import {
	compile,
	encodeIR,
	minifyLocalNamesIR,
	optimizeIR,
	parseToIR,
} from "./rex.ts";

describe("Rex IR optimizer", () => {
	test("folds arithmetic constants", () => {
		const optimized = optimizeIR(parseToIR("1 + 2"));
		expect(optimized).toEqual({ type: "number", raw: "3", value: 3 });
	});

	test("propagates constant bindings into navigation", () => {
		const optimized = optimizeIR(parseToIR(`
	t = {a: 1, b: 2}
	t.b
`));

		expect(optimized).toEqual({ type: "number", raw: "2", value: 2 });
	});

	test("folds constant conditionals", () => {
		expect(optimizeIR(parseToIR("when 1 do 2 else 3 end"))).toEqual({ type: "number", raw: "2", value: 2 });
		expect(optimizeIR(parseToIR("unless undefined do 2 else 3 end"))).toEqual({
			type: "number",
			raw: "2",
			value: 2,
		});
	});

	test("supports compile with optimize option", () => {
		expect(compile("1 + 2")).toBe("(1%2+4+)");
		expect(compile("1 + 2", { optimize: true })).toBe("6+");
	});

	test("replaces self capture vars with self forms", () => {
		const optimized = optimizeIR(parseToIR(`
x = self
x
`));

		expect(optimized).toEqual({ type: "self" });
	});

	test("rewrites captured self to depth-aware self in nested loop body", () => {
		const optimized = optimizeIR(parseToIR("x = self for [1] do x end"));
		expect(optimized).toEqual({
			type: "for",
			binding: {
				type: "binding:expr",
				source: {
					type: "array",
					items: [{ type: "number", raw: "1", value: 1 }],
				},
			},
			body: [{ type: "selfDepth", depth: 2 }],
		});
	});

	test("inlines named lookup table in explicit actions/handler form", () => {
		const source = `
actions = {
  create-user: 'users/create'
  delete-user: 'users/delete'
}
when handler = actions.(headers.x-action) do
  headers.x-handler = handler
end
`;

		const optimized = optimizeIR(parseToIR(source));
		expect(optimized).toEqual({
			type: "conditional",
			head: "when",
			condition: {
				type: "navigation",
				target: {
					type: "object",
					entries: [
						{ key: { type: "key", name: "create-user" }, value: { type: "string", raw: "'users/create'" } },
						{ key: { type: "key", name: "delete-user" }, value: { type: "string", raw: "'users/delete'" } },
					],
				},
				segments: [
					{
						type: "dynamic",
						key: {
							type: "navigation",
							target: { type: "identifier", name: "headers" },
							segments: [{ type: "static", key: "x-action" }],
						},
					},
				],
			},
			thenBlock: [
				{
					type: "assign",
					op: "=",
					place: {
						type: "navigation",
						target: { type: "identifier", name: "headers" },
						segments: [{ type: "static", key: "x-handler" }],
					},
					value: { type: "self" },
				},
			],
		});
	});

	test("eliminates dead pure assignments after propagation", () => {
		expect(optimizeIR(parseToIR("x = 1 y = 2 y"))).toEqual({
			type: "number",
			raw: "2",
			value: 2,
		});
	});

	test("keeps unresolved explicit selfDepth captures as named vars", () => {
		const optimized = optimizeIR(parseToIR("x = self@2 for [1] do x end"));
		expect(optimized).toEqual({
			type: "program",
			body: [
				{
					type: "assign",
					op: "=",
					place: { type: "identifier", name: "x" },
					value: { type: "selfDepth", depth: 2 },
				},
				{
					type: "for",
					binding: {
						type: "binding:expr",
						source: {
							type: "array",
							items: [{ type: "number", raw: "1", value: 1 }],
						},
					},
					body: [{ type: "identifier", name: "x" }],
				},
			],
		});
	});

	test("does not treat reassigned captures as self", () => {
		expect(optimizeIR(parseToIR("x = self x = 1 x"))).toEqual({
			type: "number",
			raw: "1",
			value: 1,
		});
	});

	test("inlines adjacent pure alias chains", () => {
		expect(optimizeIR(parseToIR("x = headers.x-action y = x y"))).toEqual({
			type: "navigation",
			target: { type: "identifier", name: "headers" },
			segments: [{ type: "static", key: "x-action" }],
		});
	});

	test("does not inline across effectful boundaries", () => {
		expect(optimizeIR(parseToIR("x = headers.x-action trace(x) x"))).toEqual({
			type: "program",
			body: [
				{
					type: "assign",
					op: "=",
					place: { type: "identifier", name: "x" },
					value: {
						type: "navigation",
						target: { type: "identifier", name: "headers" },
						segments: [{ type: "static", key: "x-action" }],
					},
				},
				{
					type: "call",
					callee: { type: "identifier", name: "trace" },
					args: [{ type: "identifier", name: "x" }],
				},
				{ type: "identifier", name: "x" },
			],
		});
	});

	test("minifies local variable names by frequency", () => {
		const minified = minifyLocalNamesIR(parseToIR("x = method y = x + path y"));
		expect(minified).toEqual({
			type: "program",
			body: [
				{
					type: "assign",
					op: "=",
					place: { type: "identifier", name: "" },
					value: { type: "identifier", name: "method" },
				},
				{
					type: "assign",
					op: "=",
					place: { type: "identifier", name: "1" },
					value: {
						type: "binary",
						op: "add",
						left: { type: "identifier", name: "" },
						right: { type: "identifier", name: "path" },
					},
				},
				{ type: "identifier", name: "1" },
			],
		});
	});

	test("compile supports minifyNames without renaming globals", () => {
		const encoded = compile("route-key = method + path route-key", { minifyNames: true });
		expect(encoded).toContain("method$");
		expect(encoded).toContain("path$");
		expect(encoded).toContain("=$");
		expect(encoded).not.toContain("route-key$");
	});

	test("compile maps configured domain symbols to apostrophe refs", () => {
		const encoded = encodeIR(parseToIR("headers.x-tenant"), { domainRefs: { headers: 6 } });
		expect(encoded).toBe("(6'x-tenant:)");
	});

	test("compile accepts pre-parsed domain config object", () => {
		const domainConfig = {
			data: {
				H: {
					names: ["headers"],
				},
			},
		};
		const encoded = compile("headers.x-tenant", { domainConfig });
			expect(encoded).toBe("(H'x-tenant:)");
	});

	test("compile deduplicates repeated large literals with pointers", () => {
		const encoded = compile("a = {message: \"this_is_a_large_repeated_literal\"} b = {message: \"this_is_a_large_repeated_literal\"} [a b]", {
			dedupeValues: true,
			dedupeMinBytes: 12,
		});
		expect(encoded.includes("^")).toBe(true);
	});

	test("compile deduplicates short repeated values when pointer is smaller", () => {
		const encoded = compile("[\"abcd\" \"abcd\"]", { dedupeValues: true });
		expect(encoded.includes("^")).toBe(true);
	});
});

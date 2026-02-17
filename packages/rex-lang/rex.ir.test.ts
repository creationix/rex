import { describe, expect, test } from "bun:test";
import { grammar, semantics } from "./rex.ts";

function expectIR(input: string, expected: unknown) {
	const match = grammar.match(input);
	expect(match.succeeded(), match.message).toBe(true);
	const ir = semantics(match).toIR();
	expect(ir).toEqual(expected);
}

describe("Rex IR (handwritten)", () => {
	test("assignment lowers to assign node", () => {
		expectIR("x = 1", {
			type: "assign",
			op: "=",
			place: { type: "identifier", name: "x" },
			value: { type: "number", raw: "1", value: 1 },
		});
	});

	test("navigation and dynamic navigation", () => {
		expectIR("user.name", {
			type: "navigation",
			target: { type: "identifier", name: "user" },
			segments: [{ type: "static", key: "name" }],
		});
		expectIR("table.(k)", {
			type: "navigation",
			target: { type: "identifier", name: "table" },
			segments: [{ type: "dynamic", key: { type: "identifier", name: "k" } }],
		});
	});

	test("calls lower with normalized args", () => {
		expectIR("f(1, 2)", {
			type: "call",
			callee: { type: "identifier", name: "f" },
			args: [
				{ type: "number", raw: "1", value: 1 },
				{ type: "number", raw: "2", value: 2 },
			],
		});
	});

	test("conditional chain preserves when/unless heads", () => {
		expectIR("when a do x() else unless b do y() else z() end", {
			type: "conditional",
			head: "when",
			condition: { type: "identifier", name: "a" },
			thenBlock: [
				{ type: "call", callee: { type: "identifier", name: "x" }, args: [] },
			],
			elseBranch: {
				type: "elseChain",
				head: "unless",
				condition: { type: "identifier", name: "b" },
				thenBlock: [
					{ type: "call", callee: { type: "identifier", name: "y" }, args: [] },
				],
				elseBranch: {
					type: "else",
					block: [{ type: "call", callee: { type: "identifier", name: "z" }, args: [] }],
				},
			},
		});
	});

	test("for binding and comprehension lowering", () => {
		expectIR("for v in [1, 2] do v end", {
			type: "for",
			binding: {
				type: "binding:valueIn",
				value: "v",
				source: {
					type: "array",
					items: [
						{ type: "number", raw: "1", value: 1 },
						{ type: "number", raw: "2", value: 2 },
					],
				},
			},
			body: [{ type: "identifier", name: "v" }],
		});

		expectIR("[v in [1, 2] ; v * 2]", {
			type: "arrayComprehension",
			binding: {
				type: "binding:valueIn",
				value: "v",
				source: {
					type: "array",
					items: [
						{ type: "number", raw: "1", value: 1 },
						{ type: "number", raw: "2", value: 2 },
					],
				},
			},
			body: {
				type: "binary",
				op: "mul",
				left: { type: "identifier", name: "v" },
				right: { type: "number", raw: "2", value: 2 },
			},
		});
	});
});

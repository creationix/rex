import { describe, expect, test } from "bun:test";
import { grammar, semantics } from "./rex.ts";

function expectIR(input: string, expected: unknown) {
	const match = grammar.match(input);
	expect(match.succeeded()).toBe(true);
	const ir = semantics(match).toIR();
	expect(ir).toEqual(expected);
}

describe("Rex IR (handwritten)", () => {
	test("scalar literals and predicate keywords lower correctly", () => {
		expectIR("true", { type: "boolean", value: true });
		expectIR("false", { type: "boolean", value: false });
		expectIR("null", { type: "null" });
		expectIR("undefined", { type: "undefined" });
		expectIR("self", { type: "self" });
		expectIR("self@2", { type: "selfDepth", depth: 2 });
		expectIR("self ^ 2", {
			type: "binary",
			op: "bitXor",
			left: { type: "self" },
			right: { type: "number", raw: "2", value: 2 },
		});
		expectIR("-42", { type: "number", raw: "-42", value: -42 });
		expectIR("-123.456", { type: "number", raw: "-123.456", value: -123.456 });
		expectIR("0x2A", { type: "number", raw: "0x2A", value: 42 });
		expectIR("0b101", { type: "number", raw: "0b101", value: 5 });
		expectIR("'ok'", { type: "string", raw: "'ok'" });
		expectIR("string", { type: "identifier", name: "string" });
		expectIR("number", { type: "identifier", name: "number" });
	});

	test("assignment lowers to assign node", () => {
		expectIR("x = 1", {
			type: "assign",
			op: "=",
			place: { type: "identifier", name: "x" },
			value: { type: "number", raw: "1", value: 1 },
		});

		expectIR("user.(key) += 2", {
			type: "assign",
			op: "+=",
			place: {
				type: "navigation",
				target: { type: "identifier", name: "user" },
				segments: [{ type: "dynamic", key: { type: "identifier", name: "key" } }],
			},
			value: { type: "number", raw: "2", value: 2 },
		});
	});

	test("binary and unary operators preserve precedence", () => {
		expectIR("a + b * c", {
			type: "binary",
			op: "add",
			left: { type: "identifier", name: "a" },
			right: {
				type: "binary",
				op: "mul",
				left: { type: "identifier", name: "b" },
				right: { type: "identifier", name: "c" },
			},
		});

		expectIR("~a or delete self.name", {
			type: "binary",
			op: "or",
			left: { type: "unary", op: "not", value: { type: "identifier", name: "a" } },
			right: {
				type: "unary",
				op: "delete",
				value: {
					type: "navigation",
					target: { type: "self" },
					segments: [{ type: "static", key: "name" }],
				},
			},
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

	test("postfix chains preserve left-to-right structure", () => {
		expectIR("obj.a(1).(k)(2)", {
			type: "call",
			callee: {
				type: "navigation",
				target: {
					type: "call",
					callee: {
						type: "navigation",
						target: { type: "identifier", name: "obj" },
						segments: [{ type: "static", key: "a" }],
					},
					args: [{ type: "number", raw: "1", value: 1 }],
				},
				segments: [{ type: "dynamic", key: { type: "identifier", name: "k" } }],
			},
			args: [{ type: "number", raw: "2", value: 2 }],
		});
	});

	test("arrays and objects lower with normalized elements", () => {
		expectIR("[1 2, 3,]", {
			type: "array",
			items: [
				{ type: "number", raw: "1", value: 1 },
				{ type: "number", raw: "2", value: 2 },
				{ type: "number", raw: "3", value: 3 },
			],
		});

		expectIR('{name: "Rex", 404: "Not Found", (k): v}', {
			type: "object",
			entries: [
				{
					key: { type: "key", name: "name" },
					value: { type: "string", raw: '"Rex"' },
				},
				{
					key: { type: "number", raw: "404", value: 404 },
					value: { type: "string", raw: '"Not Found"' },
				},
				{
					key: { type: "identifier", name: "k" },
					value: { type: "identifier", name: "v" },
				},
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

		expectIR("for k, v in table do v end", {
			type: "for",
			binding: {
				type: "binding:keyValueIn",
				key: "k",
				value: "v",
				source: { type: "identifier", name: "table" },
			},
			body: [{ type: "identifier", name: "v" }],
		});

		expectIR("for k of record do k end", {
			type: "for",
			binding: {
				type: "binding:keyOf",
				key: "k",
				source: { type: "identifier", name: "record" },
			},
			body: [{ type: "identifier", name: "k" }],
		});

		expectIR("for users do self end", {
			type: "for",
			binding: {
				type: "binding:expr",
				source: { type: "identifier", name: "users" },
			},
			body: [{ type: "self" }],
		});

	});

	test("while loop lowering", () => {
		expectIR("while x > 0 do x -= 1 end", {
			type: "while",
			condition: {
				type: "binary",
				op: "gt",
				left: { type: "identifier", name: "x" },
				right: { type: "number", raw: "0", value: 0 },
			},
			body: [
				{
					type: "assign",
					op: "-=",
					place: { type: "identifier", name: "x" },
					value: { type: "number", raw: "1", value: 1 },
				},
			],
		});

		expectIR("while x do process(self) end", {
			type: "while",
			condition: { type: "identifier", name: "x" },
			body: [
				{
					type: "call",
					callee: { type: "identifier", name: "process" },
					args: [{ type: "self" }],
				},
			],
		});
	});

	test("array comprehension lowering", () => {
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

		expectIR('{k, v in scores ; (k): v * 100}', {
			type: "objectComprehension",
			binding: {
				type: "binding:keyValueIn",
				key: "k",
				value: "v",
				source: { type: "identifier", name: "scores" },
			},
			key: { type: "identifier", name: "k" },
			value: {
				type: "binary",
				op: "mul",
				left: { type: "identifier", name: "v" },
				right: { type: "number", raw: "100", value: 100 },
			},
		});
	});

	test("programs and loop control lower to block-like IR", () => {
		expectIR(
			`total = 0
for v in [1, 2] do
  when v == 2 do break end
  continue
end
total`,
			{
				type: "program",
				body: [
					{
						type: "assign",
						op: "=",
						place: { type: "identifier", name: "total" },
						value: { type: "number", raw: "0", value: 0 },
					},
					{
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
						body: [
							{
								type: "conditional",
								head: "when",
								condition: {
									type: "binary",
									op: "eq",
									left: { type: "identifier", name: "v" },
									right: { type: "number", raw: "2", value: 2 },
								},
								thenBlock: [{ type: "break" }],
							},
							{ type: "continue" },
						],
					},
					{ type: "identifier", name: "total" },
				],
			},
		);
	});

	test("standalone do-expression lowers to sequence semantics", () => {
		expectIR("do x = 10 x + 2 end", {
			type: "program",
			body: [
				{
					type: "assign",
					op: "=",
					place: { type: "identifier", name: "x" },
					value: { type: "number", raw: "10", value: 10 },
				},
				{
					type: "binary",
					op: "add",
					left: { type: "identifier", name: "x" },
					right: { type: "number", raw: "2", value: 2 },
				},
			],
		});

		expectIR("do end", { type: "undefined" });
	});
});

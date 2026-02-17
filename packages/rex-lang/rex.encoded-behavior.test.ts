import { describe, expect, test } from "bun:test";
import { compile } from "./rex.ts";

describe("Rex encoded behavior", () => {
	test("encodes decimals using signed significands", () => {
		expect(compile("3.14")).toBe("3*9Q+");
		expect(compile("0.000001")).toBe("b*2+");
		expect(compile("-0.000001")).toBe("b*1+");
	});

	test("prefixes skippable conditional branches when they are containers", () => {
		const encoded = compile("when x do [1, 2] else {a: 1} end");
		expect(encoded).toBe("?(x$4[2+4+]4{a:2+})");
	});

	test("does not prefix scalar conditional branches", () => {
		expect(compile("unless x do y else z end")).toBe("!(x$y$z$)");
	});

	test("prefixes loop body containers", () => {
		expect(compile("for v in [1, 2] do [v] end")).toBe(">([2+4+]v$2[v$])");
	});

	test("encodes loop control scalars", () => {
		expect(compile("for 1 do break continue end")).toBe(">(2+4(%;1;))");
	});

	test("prefixes nested container values in arrays and objects", () => {
		expect(compile("[1, [2, 3], 4]")).toBe("[2+4[4+6+]8+]");
		expect(compile("{a: [1], b: {c: 2}}")).toBe("{a:2[2+]b:4{c:4+}}");
	});

	test("prefixes comprehension result containers in skip positions", () => {
		expect(compile("[v in [1, 2] ; {x: v}]")).toBe(">[[2+4+]v$4{x:v$}]");
		expect(compile("{v in [1, 2] ; (v): [v]}"))
			.toBe(">{[2+4+]v$v$2[v$]}");
	});

	test("encodes and/or as control-flow containers with skippable rhs", () => {
		expect(compile("a and b")).toBe("&(a$b$)");
		expect(compile("a or b")).toBe("|(a$b$)");
		expect(compile("a and [1, 2]")).toBe("&(a$4[2+4+])");
		expect(compile("a or {x: 1}")).toBe("|(a$4{x:2+})");
	});
});

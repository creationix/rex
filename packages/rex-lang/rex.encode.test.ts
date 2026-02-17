import { describe, expect, test } from "bun:test";
import { compile, encodeIR, parseToIR } from "./rex.ts";

describe("Rex encoding backend", () => {
	test("encodes assignment", () => {
		expect(compile("x = 42")).toBe("=x$1k+");
	});

	test("encodes arithmetic through opcode calls", () => {
		expect(compile("1 + 2")).toBe("(1%2+4+)");
		expect(compile("a & b")).toBe("(b%a$b$)");
	});

	test("encodes arrays and objects", () => {
		expect(compile("[1, 2, 3]")).toBe("[2+4+6+]");
		expect(compile('{color: "red", size: 42}')).toBe("{color:red:size:1k+}");
	});

	test("encodes conditionals with prefixed skip branches", () => {
		expect(compile("when x > 10 do x + 1 end")).toBe("?((9%x$k+)6(1%x$2+))");
		expect(compile("unless x do y else z end")).toBe("!(x$y$z$)");
	});

	test("encodes mutation and navigation", () => {
		expect(compile("delete self.name")).toBe("~(@name:)");
		expect(compile("table.(k) = v")).toBe("=(table$k$)v$");
	});

	test("exposes parseToIR and encodeIR", () => {
		const ir = parseToIR("[v in [1, 2] ; v * 2]");
		expect(encodeIR(ir)).toBe(">[[2+4+]v$6(3%v$4+)]");
	});

	test("encodes standalone do-expression", () => {
		expect(compile("do x = 10 20 end")).toBe("(%=x$k+E+)");
	});

	test("encodes depth-aware self from high-level form", () => {
		expect(compile("self@2")).toBe("1@");
	});
});

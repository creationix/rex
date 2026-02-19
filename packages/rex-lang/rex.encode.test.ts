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

	test("encodes compound assignments for identifiers and navigations", () => {
		expect(compile("x += 2")).toBe("=x$6(1%x$4+)");
		expect(compile("x -= 2")).toBe("=x$6(2%x$4+)");
		expect(compile("x *= 2")).toBe("=x$6(3%x$4+)");
		expect(compile("x /= 2")).toBe("=x$6(4%x$4+)");
		expect(compile("x %= 2")).toBe("=x$6(k%x$4+)");
		expect(compile("x &= y")).toBe("=x$6(b%x$y$)");
		expect(compile("x |= y")).toBe("=x$6(c%x$y$)");
		expect(compile("x ^= y")).toBe("=x$6(d%x$y$)");
		expect(compile("obj.count += 1")).toBe("=(obj$count:)g(1%(obj$count:)2+)");
		expect(compile("table.(k) *= 3")).toBe("=(table$k$)e(3%(table$k$)6+)");
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

	test("encodes built-in scalar references and self", () => {
		expect(compile("true")).toBe("1'");
		expect(compile("false")).toBe("2'");
		expect(compile("null")).toBe("3'");
		expect(compile("undefined")).toBe("4'");
		expect(compile("self")).toBe("@");
		expect(compile("self@3")).toBe("2@");
	});

	test("encodes existence operators and depth-self condition", () => {
		expect(compile("a and b")).toBe("&(a$b$)");
		expect(compile("a or b")).toBe("|(a$b$)");
		expect(compile("when self@2 do self end")).toBe("?(1@@)");
		expect(compile("unless self@2 do self end")).toBe("!(1@@)");
	});

	test('encodes skippable prefixes for container branches but not scalar branches', () => {
		expect(compile("when x do [1 2] else {a: 1} end")).toBe("?(x$4[2+4+]4{a:2+})");
		expect(compile("unless x do y else z end")).toBe("!(x$y$z$)");
		expect(compile("foo and [1 2 3] or [4 5 6]")).toBe("|(&(foo$6[2+4+6+])6[8+a+c+])");
	});

	test("encodes left-associative boolean chaining", () => {
		expect(compile("foo and bar and baz")).toBe("&(foo$bar$baz$)");
		expect(compile("foo or bar or baz")).toBe("|(foo$bar$baz$)");
		expect(compile("foo and [1 2] and [3 4]")).toBe("&(foo$4[2+4+]4[6+8+])");
		expect(compile("foo or [1 2] or [3 4]")).toBe("|(foo$4[2+4+]4[6+8+])");
		expect(compile("foo or bar and baz or qux")).toBe("|(&(|(foo$bar$)baz$)qux$)");
	});

	test("encodes explicit boolean grouping with parentheses", () => {
		expect(compile("(foo or bar) and (baz or qux)")).toBe("&(|(foo$bar$)8|(baz$qux$))");
		expect(compile("foo and (bar or baz) and qux")).toBe("&(foo$8|(bar$baz$)qux$)");
		expect(compile("(foo and bar) or (baz and qux)")).toBe("|(&(foo$bar$)8&(baz$qux$))");
		expect(compile("(foo or bar or baz)")).toBe("|(foo$bar$baz$)");
		expect(compile("foo or (bar and baz) or qux")).toBe("|(foo$8&(bar$baz$)qux$)");
		expect(compile("(foo or [1 2]) and ([3 4] or qux)")).toBe("&(|(foo$4[2+4+])a|([6+8+]qux$))");
		expect(compile("(foo and [1 2]) or ([3 4] and qux)")).toBe("|(&(foo$4[2+4+])a&([6+8+]qux$))");
	});

	test("encodes nested when/unless else-chain structure", () => {
		expect(compile("when a do x() else unless b do y() else z() end")).toBe(
			"?(a$2(x$)c!(b$2(y$)2(z$)))",
		);
		expect(compile("unless x do [1 2] else [3 4] end")).toBe("!(x$4[2+4+]4[6+8+])");
	});

	test("encodes for-binding variants and comprehensions", () => {
		expect(compile("for v in [1, 2] do v end")).toBe(">([2+4+]v$v$)");
		expect(compile("for k, v in table do v end")).toBe(">(table$k$v$v$)");
		expect(compile("for k of record do k end")).toBe("<(record$k$k$)");
		expect(compile("for users do self end")).toBe(">(users$@)");
		expect(compile("{k, v in scores ; (k): v * 100}")).toBe(">{scores$k$v$k$7(3%v$38+)}");
	});

	test("encodes while loops", () => {
		expect(compile("while x do self end")).toBe("#(x$@)");
		expect(compile("while x > 0 do x -= 1 end")).toBe("#((9%x$+)b=x$6(2%x$2+))");
	});
});

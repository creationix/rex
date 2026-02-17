import { describe, expect, test } from "bun:test";
import grammar from "./rex.ohm-bundle.js";

function expectParses(input: string) {
	const match = grammar.match(input);
	expect(match.succeeded(), match.message).toBe(true);
}

function expectFails(input: string) {
	const match = grammar.match(input);
	expect(match.failed()).toBe(true);
}

describe("Rex grammar", () => {
	test("parses scalar literals", () => {
		expectParses("42");
		expectParses("-42");
		expectParses("0x2A");
		expectParses("-0x2A");
		expectParses("0b101010");
		expectParses("-0b101010");
		expectParses("3.14");
		expectParses("-1e-6");
		expectParses("true");
		expectParses("false");
		expectParses("null");
		expectParses("undefined");
	});

	test("parses strings and escapes", () => {
		expectParses('"hello"');
		expectParses("'world'");
		expectParses('"\\u1234"');
		expectParses('"\\x48\\x69"');
		expectParses("'escaped \\\' single'");
		expectParses('"null\\0byte"');
	});

	test("parses arrays and objects", () => {
		expectParses("[1, 2, 3]");
		expectParses("[1 2 3]");
		expectParses("[1,2,3,]");
		expectParses('{name: "Rex", age: 65}');
		expectParses('{404: "Not Found", 500: "Error"}');
		expectParses('{(field): "value"}');
	});

	test("parses navigation and calls", () => {
		expectParses("user.name");
		expectParses("foo.bar.(key).baz");
		expectParses("table.(k1).(k2)");
		expectParses("string(value)");
		expectParses('path-match("/api/*")');
	});

	test("parses assignment and arithmetic operators", () => {
		expectParses("x = 42");
		expectParses("x += 1");
		expectParses("x -= 5");
		expectParses("x *= 2");
		expectParses("x /= 10");
		expectParses("x %= 3");
		expectParses("x &= 0xFF");
		expectParses("x |= 0x80");
		expectParses("x ^= mask");
		expectParses("a + b * c");
		expectParses("(a + b) * c");
	});

	test("parses comparisons and boolean/existence operators", () => {
		expectParses("age > 18 and age < 65");
		expectParses("x == y");
		expectParses("x != y");
		expectParses("a & b | c ^ d");
		expectParses("~a");
		expectParses("a or b or c");
	});

	test("parses when/unless forms", () => {
		expectParses("when age > 18 do allow(self) end");
		expectParses("unless string(value) do handle-non-string() end");
		expectParses("when authorized == true do proceed() else deny() end");
		expectParses("when string(value) do a() else when number(value) do b() else c() end");
		expectParses("when a do x() else unless b do y() else z() end");
		expectParses("do x = 1 x + 2 end");
		expectParses("value = do n = 10 n * 2 end");
	});

	test("parses for loops", () => {
		expectParses("for [1, 2, 3] do process(self) end");
		expectParses("for 5 do process(self) end");
		expectParses("for v in [1, 2, 3] do process(v) end");
		expectParses("for k, v in [1, 2, 3] do process(k, v) end");
		expectParses("for k of {a: 1, b: 2} do log(k) end");
		expectParses("for v in [1,2,3,4,5] do when v == 4 do break end when v % 2 != 0 do continue end process(v) end");
	});

	test("parses empty calls and empty containers", () => {
		expectParses("ping()");
		expectParses("[]");
		expectParses("{}");
	});

	test("parses array comprehensions", () => {
		expectParses("[100 ; self % 2 > 0 and self % 3 > 0 and self % 5 > 0]");
		expectParses("[v in [1, 2, 3] ; v * 2]");
		expectParses("[k, v in [10, 20, 30] ; v + k]");
		expectParses("[k of {name: \"Rex\", age: 65} ; k]");
	});

	test("parses object comprehensions", () => {
		expectParses("{k, v in {a: 1, b: 2} ; (k): v * 10}");
		expectParses('{v in ["x", "y", "z"] ; (v): true}');
		expectParses('{k, v in scores ; ("player-" + k): v * 100}');
		expectParses("{users ; (self.name): self.score}");
		expectParses("{k, v in data ; (k): v != null and v}");
	});

	test("parses multi-expression program", () => {
		expectParses(`total = 0
for [10, 20, 30] do
  total += self
end
total`);
	});

	test("rejects invalid identifier usage for keywords", () => {
		expectFails("when = 1");
		expectFails("for = 1");
		expectFails("string = 1");
	});

	test("rejects missing computed key parens for dotted keys", () => {
		expectFails("{users ; self.name: self.score}");
		expectFails("{v in users ; v.name: v}");
	});
});

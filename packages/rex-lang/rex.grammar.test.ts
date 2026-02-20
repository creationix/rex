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
		expectParses("self@2");
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
		expectParses("size([1, 2, 3])");
		expectParses('path-match("/api/*")');
	});

	test("parses assignment and arithmetic operators", () => {
		expectParses("x = 42");
		expectParses("x := 42");
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
		expectParses("a nor b");
	});

	test("parses when/unless forms", () => {
		expectParses("when age > 18 do allow(self) end");
		expectParses("unless string(value) do handle-non-string() end");
		expectParses("when authorized == true do proceed() else deny() end");
		expectParses("when string(value) do a() else when number(value) do b() else c() end");
		expectParses("when a do x() else unless b do y() else z() end");
	});

	test("parses range expressions", () => {
		expectParses("1..10");
		expectParses("x..y");
		expectParses("a + 1 .. b - 1");
		expectParses("1..10 == x");
		expectParses("for i in 1..10 do i end");
		expectParses("[self * 2 in 1..5]");
	});

	test("parses for loops", () => {
		expectParses("for in [1, 2, 3] do process(self) end");
		expectParses("for in 1..5 do process(self) end");
		expectParses("for v in [1, 2, 3] do process(v) end");
		expectParses("for k, v in [1, 2, 3] do process(k, v) end");
		expectParses("for k of {a: 1, b: 2} do log(k) end");
		expectParses("for of items do log(self) end");
		expectParses("for v in [1,2,3,4,5] do when v == 4 do break end when v % 2 != 0 do continue end process(v) end");
	});

	test("parses while loops", () => {
		expectParses("while x do x -= 1 end");
		expectParses("while get-next() do process(self) end");
		expectParses("while x > 0 do x -= 1 end");
	});

	test("parses empty calls and empty containers", () => {
		expectParses("ping()");
		expectParses("[]");
		expectParses("{}");
	});

	test("parses array comprehensions", () => {
		expectParses("[self % 2 > 0 and self % 3 > 0 and self % 5 > 0 in 1..100]");
		expectParses("[v * 2 for v in [1, 2, 3]]");
		expectParses("[v + k for k, v in [10, 20, 30]]");
		expectParses("[k for k of {name: \"Rex\", age: 65}]");
		expectParses("[self of items]");
		expectParses("[self while next-item()]");
		expectParses("[self * 2 while pop(queue)]");
	});

	test("parses object comprehensions", () => {
		expectParses("{(k): v * 10 for k, v in {a: 1, b: 2}}");
		expectParses('{(v): true for v in ["x", "y", "z"]}');
		expectParses('{("player-" + k): v * 100 for k, v in scores}');
		expectParses("{(self.name): self.score in users}");
		expectParses("{(k): v != null and v for k, v in data}");
		expectParses("{(self): self * 10 while pop(queue)}");
	});

	test("parses logical not", () => {
		expectParses("not x");
		expectParses("not composites.(self)");
		expectParses("not x and y");
		expectParses("not not x");
	});

	test("parses multi-expression program", () => {
		expectParses(`total = 0
for in [10, 20, 30] do
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

	test("rejects symbolic logical operators and compounds", () => {
		expectFails("a && b");
		expectFails("a || b");
		expectFails("x &&= y");
		expectFails("x ||= y");
		expectFails("self.name &&= 2");
		expectFails("self.name ||= 1");
	});
});

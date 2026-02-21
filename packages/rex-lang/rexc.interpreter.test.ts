import { describe, expect, test } from "bun:test";
import { compile } from "./rex.ts";
import { evaluateRexc, evaluateSource } from "./rexc-interpreter.ts";

type EvalSource = (source: string, ctx?: Parameters<typeof evaluateSource>[1]) => ReturnType<typeof evaluateSource>;

const MODES: Array<{ name: string; evalSource: EvalSource }> = [
	{ name: "unoptimized", evalSource: (source, ctx) => evaluateSource(source, ctx) },
	{ name: "optimized", evalSource: (source, ctx) => evaluateRexc(compile(source, { optimize: true }), ctx) },
];

for (const mode of MODES) {
	describe(`rexc interpreter (${mode.name})`, () => {
		const evaluateSource = mode.evalSource;
	test("evaluates arithmetic and comparisons", () => {
		expect(evaluateSource("1 + 2").value).toBe(3);
		expect(evaluateSource("10 > 3").value).toBe(10);
		expect(evaluateSource("2 > 3").value).toBeUndefined();
	});

	test("supports assignment and multi-expression programs", () => {
		const state = { vars: {} as Record<string, unknown> };
		const result = evaluateSource("x = 10 x + 2", state);
		expect(result.value).toBe(12);
		expect(result.state.vars["x"]).toBe(10);
	});

	test("supports conditional skip behavior", () => {
		expect(evaluateSource("when 1 do 2 else 3 end").value).toBe(2);
		expect(evaluateSource("unless undefined do 2 else 3 end").value).toBe(2);
	});

	test("nor returns right when left is undefined", () => {
		expect(evaluateSource("undefined nor 42").value).toBe(42);
		expect(evaluateSource("1 nor 42").value).toBeUndefined();
		expect(evaluateSource("x = 5 x nor 99").value).toBeUndefined();
		expect(evaluateSource("x nor 99").value).toBe(99);
	});

	test("supports navigation call semantics", () => {
		const compiled = compile("user.name");
		const result = evaluateRexc(compiled, {
			vars: { user: { name: "Rex" } },
		});
		expect(result.value).toBe("Rex");
	});

	test("only exposes own properties during navigation", () => {
		expect(evaluateSource("[1, 2, 3].map").value).toBeUndefined();
		expect(evaluateSource('"hi".toUpperCase').value).toBeUndefined();
		expect(evaluateSource("[1, 2, 3].length").value).toBeUndefined();
		expect(evaluateSource('"hi".length').value).toBeUndefined();
		const proto = { hidden: 1 };
		const obj = Object.create(proto) as Record<string, unknown>;
		obj.visible = 2;
		expect(evaluateRexc(compile("obj.hidden"), { vars: { obj } }).value).toBeUndefined();
		const plain = { visible: 2 };
		expect(evaluateRexc(compile("plain.visible"), { vars: { plain } }).value).toBe(2);
	});

		test("supports comprehensions and loop control scalar", () => {
			expect(evaluateSource("[v + 1 for v in [1, 2, 3]]").value).toEqual([2, 3, 4]);
			expect(evaluateSource("{(v): v * 10 for v in [1, 2]}").value).toEqual({ "1": 10, "2": 20 });
			expect(evaluateSource("for in 1..5 do break end").value).toBeUndefined();
		});

		test("supports array concatenation with +", () => {
			expect(evaluateSource("[1, 2] + [3, 4]").value).toEqual([1, 2, 3, 4]);
			expect(evaluateSource("a = [1] a += [2, 3] a").value).toEqual([1, 2, 3]);
		});

		test("supports object merge with +", () => {
			expect(evaluateSource("{a: 1} + {b: 2}").value).toEqual({ a: 1, b: 2 });
			expect(evaluateSource("{a: 1} + {a: 2, b: 3}").value).toEqual({ a: 2, b: 3 });
			expect(evaluateSource("obj = {a: 1} obj += {b: 2} obj").value).toEqual({ a: 1, b: 2 });
		});

		test("returns undefined for mixed-type add", () => {
			expect(evaluateSource("1 + \"2\"").value).toBeUndefined();
			expect(evaluateSource("[1] + 2").value).toBeUndefined();
			expect(evaluateSource("{a: 1} + 2").value).toBeUndefined();
		});

		test("supports array methods", () => {
			expect(evaluateSource("[1, 2].push(3)").value).toEqual([1, 2, 3]);
			expect(evaluateSource("[1, 2].unshift(0)").value).toEqual([0, 1, 2]);
			expect(evaluateSource("[1, 2].pop()").value).toBe(2);
			expect(evaluateSource("[1, 2].shift()").value).toBe(1);
			expect(evaluateSource("[1, 2, 3, 4].slice(1, 3)").value).toEqual([2, 3]);
			expect(evaluateSource("[1, 2, 3].join('-')").value).toBe("1-2-3");
			expect(evaluateSource("[1].size").value).toBe(1);
		});

		test("supports string methods", () => {
			expect(evaluateSource("'a,b'.split(',')").value).toEqual(["a", "b"]);
			expect(evaluateSource("'ab'.join('-')").value).toBe("a-b");
			expect(evaluateSource("'hello'.slice(1, 4)").value).toBe("ell");
			expect(evaluateSource("'hello'.starts-with('he')").value).toBe(true);
			expect(evaluateSource("'hello'.ends-with('lo')").value).toBe(true);
			expect(evaluateSource("'hi'.size").value).toBe(2);
		});

		test("supports size for arrays and strings", () => {
			expect(evaluateSource("[1, 2, 3].size").value).toBe(3);
			expect(evaluateSource('"a💡".size').value).toBe(2);
			expect(evaluateSource("{a: 1}.size").value).toBeUndefined();
		});

	test("iterates strings by Unicode code points", () => {
		expect(evaluateSource('[self in "a💡"]').value).toEqual(["a", "💡"]);
	});

	test("supports in/of binding forms and key/value bindings", () => {
		expect(evaluateSource("[k for k of {a: 1, b: 2}]").value).toEqual(["a", "b"]);
		expect(evaluateSource("[k for k in {a: 1, b: 2}]").value).toEqual([1, 2]);
		expect(evaluateSource("[k + v for k, v in [10, 20]]").value).toEqual([10, 21]);
		expect(evaluateSource("{(k): v for k, v in {a: 1, b: 2}}").value).toEqual({ a: 1, b: 2 });
	});

	test("supports nested break and continue behavior", () => {
		expect(evaluateSource("for in 1..3 do for in 1..3 do break end 99 end").value).toBe(99);
		expect(evaluateSource("for in 1..3 do continue end").value).toBeUndefined();
	});

	test("supports loop-control scalar depth decoding", () => {
		expect(evaluateRexc("2;").value).toEqual({ kind: "break", depth: 2 });
		expect(evaluateRexc("3;").value).toEqual({ kind: "continue", depth: 2 });
	});

	test("supports while loops", () => {
		// Basic countdown
		const r1 = evaluateSource("x = 3 while x > 0 do x -= 1 end x");
		expect(r1.value).toBe(0);

		// While returns last body value
		const r2 = evaluateSource("x = 3 while x > 0 do x -= 1 end");
		expect(r2.value).toBe(0);

		// Condition is existence-based — stops on undefined
		expect(evaluateSource("while undefined do 99 end").value).toBeUndefined();

		// Self is set to condition value
		const r3 = evaluateSource("x = 3 while x > 0 do x -= 1 self end");
		expect(r3.value).toBe(1);

		// Break exits early
		const r4 = evaluateSource("x = 10 while x > 0 do x -= 1 when x == 5 do break end end x");
		expect(r4.value).toBe(5);

		// Continue skips to next iteration (must be last expression in body)
		const r5 = evaluateSource("x = 3 while x > 0 do x -= 1 continue end");
		expect(r5.value).toBeUndefined();
	});

	test("swap-assign := returns old value", () => {
		const r1 = evaluateSource("x = 5 x := 10");
		expect(r1.value).toBe(5);
		expect(r1.state.vars["x"]).toBe(10);

		// swap-assign with expression
		const r2 = evaluateSource("x = 3 x := x + 1");
		expect(r2.value).toBe(3);
		expect(r2.state.vars["x"]).toBe(4);

		// swap-assign on navigation place
		const r3 = evaluateSource("obj = {count: 5} obj.count := 10");
		expect(r3.value).toBe(5);
		expect((r3.state.vars["obj"] as Record<string, unknown>)["count"]).toBe(10);
	});

	test("supports variable delete", () => {
		const result = evaluateSource("x = 10 delete x x");
		expect(result.value).toBeUndefined();
		expect(result.state.vars["x"]).toBeUndefined();
	});

	test("supports depth-aware self and apostrophe references", () => {
		expect(evaluateRexc("@", { self: "root" }).value).toBe("root");
		expect(evaluateRexc(">([2+]1@)", { self: "outer" }).value).toBe("outer");
		expect(evaluateRexc("5'", { refs: { 5: "headers" } }).value).toBe("headers");
		expect(evaluateSource("for in [10] do for in [20] do self@2 end end").value).toBe(10);
	});

	test("supports built-in apostrophe references", () => {
		expect(evaluateRexc("tr'").value).toBe(true);
		expect(evaluateRexc("fl'").value).toBe(false);
		expect(evaluateRexc("nl'").value).toBeNull();
		expect(evaluateRexc("un'").value).toBeUndefined();
	});

	test("supports while array comprehensions", () => {
		// Collect values while condition is defined
		const r1 = evaluateSource("x = 3 [x -= 1 while x > 0]");
		expect(r1.value).toEqual([2, 1, 0]);

		// Self is the condition value
		const r2 = evaluateSource("x = 3 [self while x > 1 and (x -= 1)]");
		expect(r2.value).toEqual([2, 1]);

		// Empty when condition is immediately undefined
		expect(evaluateSource("[99 while undefined]").value).toEqual([]);
	});

	test("supports while object comprehensions", () => {
		const r1 = evaluateSource("x = 3 {(x): x * 10 while x > 1 and (x -= 1)}");
		expect(r1.value).toEqual({ "2": 20, "1": 10 });
	});

	test("supports logical not", () => {
		// not undefined → true
		expect(evaluateSource("not undefined").value).toBe(true);

		// not (defined value) → undefined
		expect(evaluateSource("not 5").value).toBeUndefined();
		expect(evaluateSource("not true").value).toBeUndefined();
		expect(evaluateSource("not 0").value).toBeUndefined();
		expect(evaluateSource("not false").value).toBeUndefined();
		expect(evaluateSource("not 'hello'").value).toBeUndefined();

		// not not (defined) → true
		expect(evaluateSource("not not 5").value).toBe(true);

		// not not undefined → undefined
		expect(evaluateSource("not not undefined").value).toBeUndefined();

		// Composition with and
		expect(evaluateSource("not undefined and 42").value).toBe(42);
		expect(evaluateSource("not 5 and 42").value).toBeUndefined();
	});

	test("supports existence operator runtime semantics", () => {
		expect(evaluateSource("undefined or 5").value).toBe(5);
		expect(evaluateSource("0 or 5").value).toBe(0);
		expect(evaluateSource("undefined and 5").value).toBeUndefined();
		expect(evaluateSource("1 and 2").value).toBe(2);
	});

	test("supports deep self stack reads", () => {
		expect(evaluateRexc("2@", { selfStack: ["grand", "parent", "child"] }).value).toBe("grand");
	});

	test("self in when (?) then-branch equals condition value", () => {
		// Literal condition
		expect(evaluateSource("when 3 do self end").value).toBe(3);
		expect(evaluateSource("when 'hello' do self end").value).toBe("hello");
		expect(evaluateSource("when 0 do self end").value).toBe(0);
		expect(evaluateSource("when false do self end").value).toBe(false);

		// Assignment condition — self is the assigned value
		expect(evaluateSource("when x = 3 do self end").value).toBe(3);

		// Expression condition
		expect(evaluateSource("when 2 + 3 do self end").value).toBe(5);
	});

	test("self in when (?) else-branch inherits outer self", () => {
		// No outer self — self is initial (undefined)
		expect(evaluateSource("when undefined do 'yes' else self end").value).toBeUndefined();

		// Inside a for loop — self should be the loop item, not the undefined condition
		expect(evaluateSource("for in [42] do when undefined do 'yes' else self end end").value).toBe(42);
	});

	test("self in unless (!) then-branch inherits outer self", () => {
		// No outer self — self is initial (undefined)
		expect(evaluateSource("unless undefined do self end").value).toBeUndefined();

		// Inside a for loop — self should be the loop item, not the undefined condition
		expect(evaluateSource("for in [42] do unless undefined do self end end").value).toBe(42);
	});

	test("self in unless (!) else-branch equals condition value", () => {
		// Condition is defined, so else runs and self = condition
		expect(evaluateSource("unless 7 do 'no' else self end").value).toBe(7);
		expect(evaluateSource("unless 'hi' do 'no' else self end").value).toBe("hi");
	});

	test("self in for loop body equals iteration value", () => {
		expect(evaluateSource("for in [10, 20, 30] do self end").value).toBe(30);
		expect(evaluateSource("[self * self in 1..3]").value).toEqual([1, 4, 9]);
	});

	test("self in array comprehension equals iteration value", () => {
		expect(evaluateSource("[self for v in [5, 6]]").value).toEqual([5, 6]);
	});

	test("self in object comprehension equals iteration value", () => {
		expect(evaluateSource("{(self): self * 10 for v in [1, 2]}").value).toEqual({ "1": 10, "2": 20 });
	});

	test("nested self depth through conditionals and loops", () => {
		// when inside for — self is the when condition, self@2 is the loop item
		expect(evaluateSource("for in [100] do when 5 do self end end").value).toBe(5);
		expect(evaluateSource("for in [100] do when 5 do self@2 end end").value).toBe(100);

		// Nested for loops — self@2 reaches outer loop
		expect(evaluateSource("for in [10] do for in [20] do self@2 end end").value).toBe(10);

		// when inside when — self at each depth
		expect(evaluateSource("when 'outer' do when 'inner' do self end end").value).toBe("inner");
		expect(evaluateSource("when 'outer' do when 'inner' do self@2 end end").value).toBe("outer");
	});

	test("supports range expressions", () => {
		expect(evaluateSource("1..5").value).toEqual([1, 2, 3, 4, 5]);
		expect(evaluateSource("0..4").value).toEqual([0, 1, 2, 3, 4]);
		expect(evaluateSource("5..1").value).toEqual([5, 4, 3, 2, 1]);
		expect(evaluateSource("3..3").value).toEqual([3]);
		expect(evaluateSource("[self * 2 in 1..5]").value).toEqual([2, 4, 6, 8, 10]);
		expect(evaluateSource("for i in 1..3 do i * 10 end").value).toBe(30);
	});

	test("supports reference place mutation", () => {
		expect(evaluateRexc("(%=5'k+5')", { refs: { 5: 0 } }).value).toBe(10);
	});

	test("supports type predicate calls through evaluateSource", () => {
		expect(evaluateSource("number(42)").value).toBe(42);
		expect(evaluateSource('number("hello")').value).toBeUndefined();
		expect(evaluateSource('string("hello")').value).toBe("hello");
		expect(evaluateSource("string(42)").value).toBeUndefined();
		expect(evaluateSource("boolean(true)").value).toBe(true);
		expect(evaluateSource("boolean(42)").value).toBeUndefined();
		expect(evaluateSource("array([1, 2])").value).toEqual([1, 2]);
		expect(evaluateSource("array(42)").value).toBeUndefined();
		expect(evaluateSource("object({x: 1})").value).toEqual({ x: 1 });
		expect(evaluateSource("object(42)").value).toBeUndefined();
	});

	test("type predicates compose with when for type dispatch", () => {
		expect(evaluateSource('when n = number(42) do n + 1 end').value).toBe(43);
		expect(evaluateSource('when n = number("hi") do n + 1 else "not a number" end').value).toBe("not a number");
		expect(evaluateSource('x = 42 when string(x) do "string" else when number(x) do "number" else "other" end').value).toBe("number");
	});
	});
}

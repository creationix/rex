import { describe, expect, test } from "bun:test";
import { compile } from "./rex.ts";
import { evaluateRexc, evaluateSource } from "./rexc-interpreter.ts";

describe("rexc interpreter (streaming)", () => {
	test("evaluates arithmetic and comparisons", () => {
		expect(evaluateSource("1 + 2").value).toBe(3);
		expect(evaluateSource("10 > 3").value).toBe(10);
		expect(evaluateSource("2 > 3").value).toBeUndefined();
	});

	test("supports assignment and standalone do expression", () => {
		const state = { vars: {} as Record<string, unknown> };
		const result = evaluateSource("do x = 10 x + 2 end", state);
		expect(result.value).toBe(12);
		expect(result.state.vars["x"]).toBe(10);
	});

	test("supports conditional skip behavior", () => {
		expect(evaluateSource("when 1 do 2 else 3 end").value).toBe(2);
		expect(evaluateSource("unless undefined do 2 else 3 end").value).toBe(2);
	});

	test("supports navigation call semantics", () => {
		const compiled = compile("user.name");
		const result = evaluateRexc(compiled, {
			vars: { user: { name: "Rex" } },
		});
		expect(result.value).toBe("Rex");
	});

	test("supports comprehensions and loop control scalar", () => {
		expect(evaluateSource("[v in [1, 2, 3] ; v + 1]").value).toEqual([2, 3, 4]);
		expect(evaluateSource("{v in [1, 2] ; (v): v * 10}").value).toEqual({ "1": 10, "2": 20 });
		expect(evaluateSource("for 5 do break end").value).toBeUndefined();
	});

	test("supports in/of binding forms and key/value bindings", () => {
		expect(evaluateSource("[k of {a: 1, b: 2} ; k]").value).toEqual([1, 2]);
		expect(evaluateSource("[k in {a: 1, b: 2} ; k]").value).toEqual([1, 2]);
		expect(evaluateSource("[k, v in [10, 20] ; k + v]").value).toEqual([10, 21]);
		expect(evaluateSource("{k, v in {a: 1, b: 2} ; (k): v}").value).toEqual({ a: 1, b: 2 });
	});

	test("supports nested break and continue behavior", () => {
		expect(evaluateSource("for 3 do for 3 do break end 99 end").value).toBe(99);
		expect(evaluateSource("for 3 do continue end").value).toBeUndefined();
	});

	test("supports loop-control scalar depth decoding", () => {
		expect(evaluateRexc("2;").value).toEqual({ kind: "break", depth: 2 });
		expect(evaluateRexc("3;").value).toEqual({ kind: "continue", depth: 2 });
	});

	test("supports variable delete", () => {
		const result = evaluateSource("do x = 10 delete x x end");
		expect(result.value).toBeUndefined();
		expect(result.state.vars["x"]).toBeUndefined();
	});

	test("supports depth-aware self and apostrophe references", () => {
		expect(evaluateRexc("@", { self: "root" }).value).toBe("root");
		expect(evaluateRexc(">([2+]1@)", { self: "outer" }).value).toBe("outer");
		expect(evaluateRexc("5'", { refs: { 5: "headers" } }).value).toBe("headers");
		expect(evaluateSource("for [10] do for [20] do self@2 end end").value).toBe(10);
	});
});

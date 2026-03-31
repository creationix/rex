import { describe, expect, test } from "bun:test";
import { rex, rexc, route, toRex, createDomain } from "./index";

// ── toRex ──────────────────────────────────────────────────────────────

describe("toRex", () => {
	test("strings are double-quoted and escaped", () => {
		expect(toRex("hello")).toBe('"hello"');
		expect(toRex('say "hi"')).toBe('"say \\"hi\\""');
		expect(toRex("back\\slash")).toBe('"back\\\\slash"');
	});

	test("numbers", () => {
		expect(toRex(42)).toBe("42");
		expect(toRex(-3.14)).toBe("-3.14");
		expect(toRex(0)).toBe("0");
		expect(toRex(NaN)).toBe("NaN");
		expect(toRex(Infinity)).toBe("Infinity");
		expect(toRex(-Infinity)).toBe("-Infinity");
	});

	test("booleans", () => {
		expect(toRex(true)).toBe("true");
		expect(toRex(false)).toBe("false");
	});

	test("null and undefined", () => {
		expect(toRex(null)).toBe("null");
		expect(toRex(undefined)).toBe("undefined");
	});

	test("arrays", () => {
		expect(toRex([])).toBe("[]");
		expect(toRex([1, 2, 3])).toBe("[1, 2, 3]");
		expect(toRex(["a", "b"])).toBe('["a", "b"]');
	});

	test("objects", () => {
		expect(toRex({})).toBe("{}");
		expect(toRex({ name: "Ada", score: 95 })).toBe(
			'{name: "Ada", score: 95}',
		);
	});

	test("objects with non-identifier keys get quoted", () => {
		expect(toRex({ "content-type": "json" })).toBe(
			'{content-type: "json"}',
		);
		expect(toRex({ "has space": 1 })).toBe('{"has space": 1}');
	});

	test("nested structures", () => {
		expect(toRex({ items: [1, 2], ok: true })).toBe(
			"{items: [1, 2], ok: true}",
		);
	});
});

// ── rex tagged template ────────────────────────────────────────────────

describe("rex", () => {
	test("plain source passthrough", () => {
		const src = rex`status = 200`;
		expect(src).toBe("status = 200");
	});

	test("interpolates numbers", () => {
		const code = 200;
		const src = rex`status = ${code}`;
		expect(src).toBe("status = 200");
	});

	test("interpolates strings", () => {
		const ct = "application/json";
		const src = rex`headers.content-type = ${ct}`;
		expect(src).toBe('headers.content-type = "application/json"');
	});

	test("interpolates objects", () => {
		const config = { ok: true, code: 200 };
		const src = rex`return ${config}`;
		expect(src).toBe("return {ok: true, code: 200}");
	});

	test("multiple interpolations", () => {
		const method = "GET";
		const status = 200;
		const src = rex`when method == ${method} do status = ${status} end`;
		expect(src).toBe('when method == "GET" do status = 200 end');
	});
});

// ── rexc tagged template ──────────────────────────────────────────────

describe("rexc", () => {
	test("compiles to bytecode", () => {
		const bc = rexc`42`;
		expect(bc).toBe("1k+");
	});

	test("compiles with interpolation", () => {
		const code = 200;
		const bc = rexc`${code}`;
		expect(bc).toBe("6g+");
	});

	test("compiles routing middleware", () => {
		const bc = rexc`
			when method == "GET" do
				status = 200
			end
		`;
		expect(bc.length).toBeGreaterThan(0);
		expect(bc).toContain("?");
	});
});

// ── route builder ─────────────────────────────────────────────────────

describe("route", () => {
	test("returns source and bytecode", () => {
		const r = route`status = 200`;
		expect(r.source).toBe("status = 200");
		expect(r.bytecode.length).toBeGreaterThan(0);
		expect(r.diagnostics).toEqual([]);
	});

	test("with interpolation", () => {
		const status = 404;
		const r = route`status = ${status}`;
		expect(r.source).toBe("status = 404");
		expect(r.bytecode.length).toBeGreaterThan(0);
	});
});

// ── createDomain ──────────────────────────────────────────────────────

describe("createDomain", () => {
	test("stores rexd source", () => {
		const domain = createDomain(`
			extern method = string
			extern mut status = integer
		`);
		expect(domain.rexd).toContain("extern method");
	});

	test("compile produces bytecode", () => {
		const domain = createDomain(`
			extern method = string
			extern mut status = integer
		`);
		const bc = domain.compile("status = 200");
		expect(bc.length).toBeGreaterThan(0);
	});

	test("rexc tagged template compiles", () => {
		const domain = createDomain(`
			extern method = string
			extern mut status = integer
		`);
		const bc = domain.rexc`status = ${200}`;
		expect(bc.length).toBeGreaterThan(0);
	});

	test("route returns source + bytecode + diagnostics", () => {
		const domain = createDomain(`
			extern method = string
			extern mut status = integer
		`);
		const r = domain.route`
			when method == "GET" do
				status = ${200}
			end
		`;
		expect(r.source).toContain("method");
		expect(r.bytecode.length).toBeGreaterThan(0);
		expect(Array.isArray(r.diagnostics)).toBe(true);
	});

	test("check returns diagnostics array", () => {
		const domain = createDomain(`
			extern method = string
			extern mut status = integer
		`);
		const diags = domain.check("status = 200");
		expect(Array.isArray(diags)).toBe(true);
	});
});

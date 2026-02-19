import { describe, expect, test } from "bun:test";
import { parse, stringify } from "./rex.ts";

describe("rex parse/stringify", () => {
	test("parse handles data-oriented rex documents", () => {
		const input = `{
  name: "rex"
  enabled: true
  retries: 3
  labels: ["a" "b"]
  nested: {ok: true level: 2}
  missing: undefined
}`;

		expect(parse(input)).toEqual({
			name: "rex",
			enabled: true,
			retries: 3,
			labels: ["a", "b"],
			nested: { ok: true, level: 2 },
			missing: undefined,
		});
	});

	test("parse rejects non-data expressions", () => {
		expect(() => parse("x = 1")).toThrow("only supports data expressions");
	});

	test("parse does not auto-load domain extensions", () => {
		expect(() => parse("headers")).toThrow("only supports data expressions");
	});

	test("stringify uses bare keys, no commas, and inline tiny structures", () => {
		const value = {
			name: "rex",
			"x-y": true,
			meta: { ok: true, size: 2 },
			"not valid": "quoted",
			list: [1, 2, { small: 1 }],
		};
		const out = stringify(value);

		expect(out).toContain("x-y: true");
		expect(out).toContain('"not valid": "quoted"');
		expect(out).toContain("meta: {ok: true size: 2}");
		expect(out.includes(",")).toBe(false);
		expect(parse(out)).toEqual(value);
	});

	test("stringify wraps with 2-space indentation when exceeding width", () => {
		const out = stringify(
			{
				section: {
					title: "a very long title to force line wrapping",
					flags: ["alpha", "beta", "gamma", "delta"],
				},
			},
			{ maxWidth: 40 },
		);

		expect(out).toContain("\n  section: {");
		expect(out).toContain("\n    title: ");
		expect(out.includes(",")).toBe(false);
		expect(parse(out)).toEqual({
			section: {
				title: "a very long title to force line wrapping",
				flags: ["alpha", "beta", "gamma", "delta"],
			},
		});
	});

	test("parse and stringify round-trip", () => {
		const value = {
			name: "service",
			config: {
				enabled: true,
				retries: 5,
			},
			items: [1, 2, 3],
		};
		expect(parse(stringify(value))).toEqual(value);
	});

	test("parse preserves integer keys", () => {
		const input = '{404: "Not Found", 200: "OK"}';
		const result = parse(input);
		expect(result).toEqual({ "404": "Not Found", "200": "OK" });
	});

	test("stringify emits integer keys without quotes", () => {
		const value = { "404": "Not Found", "200": "OK" };
		const out = stringify(value);
		expect(out).toContain("404:");
		expect(out).toContain("200:");
		expect(out).not.toContain('"404"');
		expect(out).not.toContain('"200"');
	});

	test("integer keys round-trip through parse/stringify", () => {
		const input = '{404: "Not Found", 200: "OK", name: "status"}';
		const parsed = parse(input);
		const output = stringify(parsed);
		expect(parse(output)).toEqual(parsed);
		// Ensure integer keys stay unquoted after round-trip
		expect(output).toContain("404:");
		expect(output).not.toContain('"404"');
	});

	test("stringify handles mixed bare, integer, and quoted keys", () => {
		const value = {
			name: "test",
			"404": "Not Found",
			"not valid": "needs quotes",
			"0": "zero",
		};
		const out = stringify(value);
		expect(out).toContain("name:");
		expect(out).toContain("404:");
		expect(out).toContain("0:");
		expect(out).toContain('"not valid":');
		expect(out).not.toContain('"404"');
		expect(out).not.toContain('"0"');
		expect(parse(out)).toEqual(value);
	});
});

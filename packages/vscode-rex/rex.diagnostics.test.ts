import { describe, expect, test } from "bun:test";
import { getRexParseFailure } from "./src/rex-diagnostics";

describe("rex diagnostics", () => {
	test("returns null for valid rex source", () => {
		expect(getRexParseFailure("when x do y end")).toBeNull();
		expect(getRexParseFailure("x = 1\nfor v in [1,2] do v end")).toBeNull();
	});

	test("returns parse failure details for invalid source", () => {
		const failure = getRexParseFailure("when x do");
		expect(failure).not.toBeNull();
		expect(failure?.message).toContain('expected "end"');
		expect(failure?.line).toBe(1);
		expect(failure?.column).toBe(10);
		expect(failure?.startOffset).toBe(9);
	});

	test("tracks line and column across multiple lines", () => {
		const failure = getRexParseFailure("x = 1\nwhen y do\n  z");
		expect(failure).not.toBeNull();
		expect(failure?.message).toContain('Line 3, col 4');
		expect(failure?.line).toBe(3);
		expect(failure?.column).toBe(4);
		expect(failure?.startOffset).toBe(19);
	});
});

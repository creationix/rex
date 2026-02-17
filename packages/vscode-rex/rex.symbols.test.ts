import { describe, expect, test } from "bun:test";
import {
	analyzeRexSymbols,
	findDefinitionAtOffset,
	findReferencesAtOffset,
} from "./src/rex-symbols";

describe("rex symbols", () => {
	test("collects assignment and loop binding definitions", () => {
		const source = `x = 1
for v in [1, 2] do
  y = v
end`;
		const analysis = analyzeRexSymbols(source);
		const names = analysis.definitions.map((definition) => definition.name);
		expect(names).toContain("x");
		expect(names).toContain("v");
		expect(names).toContain("y");
	});

	test("goto definition resolves nearest visible binding", () => {
		const source = `x = 1
for v in [1, 2] do
  y = v
  y
end`;
		const targetOffset = source.lastIndexOf("y");
		const definition = findDefinitionAtOffset(source, targetOffset);
		expect(definition).not.toBeNull();
		expect(definition?.name).toBe("y");
		expect(source.slice(definition!.start, definition!.end)).toBe("y");
	});

	test("goto definition prefers inner loop binding over outer variable", () => {
		const source = `v = 0
for v in [1, 2] do
  v
end`;
		const usageOffset = source.lastIndexOf("v");
		const definition = findDefinitionAtOffset(source, usageOffset);
		expect(definition).not.toBeNull();
		expect(definition?.name).toBe("v");
		expect(definition?.start).toBe(source.indexOf("v in"));
	});

	test("find references includes declaration when requested", () => {
		const source = `x = 1
y = x + x
x`;
		const atUsage = source.lastIndexOf("x");
		const refs = findReferencesAtOffset(source, atUsage, true);
		expect(refs.map((entry) => source.slice(entry.start, entry.end))).toEqual([
			"x",
			"x",
			"x",
			"x",
		]);
		expect(refs.length).toBe(4);
	});

	test("find references excludes declaration when requested", () => {
		const source = `x = 1
y = x + x
x`;
		const atUsage = source.lastIndexOf("x");
		const refs = findReferencesAtOffset(source, atUsage, false);
		expect(refs.length).toBe(3);
		expect(refs.every((entry) => entry.start !== source.indexOf("x ="))).toBe(true);
	});

	test("find references respects shadowed loop bindings", () => {
		const source = `v = 0
for v in [1, 2] do
  v
end
v`;
		const innerUsage = source.indexOf("  v") + 2;
		const outerUsage = source.lastIndexOf("v");

		const innerRefs = findReferencesAtOffset(source, innerUsage, true);
		expect(innerRefs.map((entry) => entry.start)).toEqual([
			source.indexOf("v in"),
			source.indexOf("  v") + 2,
		]);

		const outerRefs = findReferencesAtOffset(source, outerUsage, true);
		expect(outerRefs.map((entry) => entry.start)).toEqual([
			source.indexOf("v = 0"),
			source.lastIndexOf("v"),
		]);
	});
});

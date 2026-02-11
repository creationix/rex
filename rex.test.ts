import { describe, expect, test } from "bun:test";
import { grammar, semantics } from "./rex.ts";

function parse(input: string) {
	const match = grammar.match(input);
	if (match.failed()) throw new Error(match.message);
	return semantics(match).toJSON();
}

describe("Rex", () => {
	test("integer literal", () => {
		expect(parse("42")).toEqual(42);
	});
	test("negative integer literal", () => {
		expect(parse("-42")).toEqual(-42);
	});
	test("hexadecimal literal", () => {
		expect(parse("0x2A")).toEqual(42);
	});
	test("negative hexadecimal literal", () => {
		expect(parse("-0x2A")).toEqual(-42);
	});
	test("binary literal", () => {
		expect(parse("0b101010")).toEqual(42);
	});
	test("negative binary literal", () => {
		expect(parse("-0b101010")).toEqual(-42);
	});
	test("decimal literal", () => {
		expect(parse("3.14")).toEqual(3.14);
	});
	test("negative decimal literal", () => {
		expect(parse("-3.14")).toEqual(-3.14);
	});
	test("decimal literal with exponent", () => {
		expect(parse("1e6")).toEqual(1e6);
	});
	test("negative decimal literal with exponent", () => {
		expect(parse("-1e6")).toEqual(-1e6);
	});
	test("decimal literal with negative exponent", () => {
		expect(parse("1e-6")).toEqual(1e-6);
	});
	test("negative decimal literal with negative exponent", () => {
		expect(parse("-1e-6")).toEqual(-1e-6);
	});
	test("simple call", () => {
		expect(parse("(add 1 2)")).toEqual([["$add", 1, 2]]);
	});
});

import { describe, expect, test } from "bun:test";
import { TOKEN_TYPES, tokenize } from "./src/rexc-tokenizer.ts";

const K = TOKEN_TYPES.indexOf("keyword");
const P = TOKEN_TYPES.indexOf("property");
const S = TOKEN_TYPES.indexOf("string");

function keywordCount(input: string) {
	return tokenize(input).filter((token) => token.type === K).length;
}

describe("rexc tokenizer", () => {
	test("recognizes control containers with > and < openers", () => {
		expect(keywordCount(">([2+4+]v$2[v$])")).toBeGreaterThan(0);
		expect(keywordCount(">[[2+4+]v$4{x:v$}]")).toBeGreaterThan(0);
		expect(keywordCount(">{[2+4+]v$v$2[v$]}")).toBeGreaterThan(0);
		expect(keywordCount("<([2+4+]k$2[k$])")).toBeGreaterThan(0);
	});

	test("tolerates <] and <{ forms", () => {
		expect(keywordCount("<]iter$2[x$]]")).toBeGreaterThan(0);
		expect(keywordCount("<{iter$k$2{a:2+}}")).toBeGreaterThan(0);
	});

	test("recognizes loop-control scalar n;", () => {
		const tokens = tokenize("; 1; 2; x;");
		const keywordTokens = tokens.filter((token) => token.type === K);
		expect(keywordTokens.length).toBe(4);
	});

	test("treats @ and apostrophe as scalar tags, not string quote", () => {
		const tokens = tokenize("@ 1@ ' 1'");
		const propertyTokens = tokens.filter((token) => token.type === P);
		const stringTokens = tokens.filter((token) => token.type === S);
		expect(propertyTokens.length).toBe(4);
		expect(stringTokens.length).toBe(0);
	});
});

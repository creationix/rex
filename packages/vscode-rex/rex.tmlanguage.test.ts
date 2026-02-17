import { describe, expect, test } from "bun:test";

describe("rex TextMate grammar", () => {
	test("targets high-level infix rex syntax", async () => {
		const grammarPath = new URL("./syntaxes/rex.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			repository: Record<
				string,
				{ patterns?: Array<{ include?: string }>; begin?: string; end?: string; match?: string }
			>;
		};

		const includes = new Set(
			(grammar.repository.expressions?.patterns ?? [])
				.map((pattern) => pattern.include)
				.filter((include): include is string => Boolean(include)),
		);

		expect(includes.has("#control-keyword")).toBe(true);
		expect(includes.has("#existence-operator")).toBe(true);
		expect(includes.has("#comparison-operator")).toBe(true);
		expect(includes.has("#assignment-operator")).toBe(true);
		expect(includes.has("#value-operator")).toBe(true);
		expect(includes.has("#self-depth")).toBe(true);
		expect(includes.has("#navigation-dynamic")).toBe(true);
		expect(includes.has("#navigation-static")).toBe(true);
		expect(includes.has("#object-key")).toBe(true);

		expect(grammar.repository["control-keyword"]?.match).toContain(
			"(when|unless|else|for|in|of|do|end|break|continue)",
		);
		expect(grammar.repository["existence-operator"]?.match).toContain("(and|or)");
		expect(grammar.repository["self-depth"]?.match).toContain("self(?:@[1-9][0-9]*)?");
		expect(grammar.repository["assignment-operator"]?.match).toContain("\\+=|-=|\\*=|/=|%=|&=|\\|=|\\^=|=");
		expect(grammar.repository["navigation-dynamic"]?.match).toContain("\\.\\(");

		expect(grammar.repository["paren-expression"]).toBeUndefined();
		expect(grammar.repository["paren-expression-generic"]).toBeUndefined();
		expect(grammar.repository["interpolation"]).toBeUndefined();
	});
});

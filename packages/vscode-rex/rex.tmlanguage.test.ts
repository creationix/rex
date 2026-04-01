import { describe, expect, test } from "bun:test";

describe("rex TextMate grammar", () => {
	test("has required pattern includes", async () => {
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

		// Required includes
		expect(includes.has("#control-keyword")).toBe(true);
		expect(includes.has("#logical-operator")).toBe(true);
		expect(includes.has("#comparison-operator")).toBe(true);
		expect(includes.has("#assignment-operator")).toBe(true);
		expect(includes.has("#value-operator")).toBe(true);
		expect(includes.has("#declaration-keyword")).toBe(true);
		expect(includes.has("#template-literal")).toBe(true);
		expect(includes.has("#arrow-operator")).toBe(true);
		expect(includes.has("#navigation-dynamic")).toBe(true);
		expect(includes.has("#navigation-static")).toBe(true);
		expect(includes.has("#object-key")).toBe(true);

		// Removed patterns should not exist
		expect(includes.has("#self-depth")).toBe(false);
		expect(includes.has("#existence-operator")).toBe(false);
	});

	test("control keywords include return and delete", async () => {
		const grammarPath = new URL("./syntaxes/rex.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			repository: Record<string, { match?: string }>;
		};

		const match = grammar.repository["control-keyword"]?.match ?? "";
		expect(match).toContain("return");
		expect(match).toContain("delete");
		expect(match).toContain("when");
		expect(match).toContain("break");
	});

	test("declaration keywords: type, extern, mut", async () => {
		const grammarPath = new URL("./syntaxes/rex.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			repository: Record<string, { match?: string }>;
		};

		const match = grammar.repository["declaration-keyword"]?.match ?? "";
		expect(match).toContain("type");
		expect(match).toContain("extern");
		expect(match).toContain("mut");
	});

	test("logical operators: and, or (no nor)", async () => {
		const grammarPath = new URL("./syntaxes/rex.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			repository: Record<string, { match?: string }>;
		};

		const match = grammar.repository["logical-operator"]?.match ?? "";
		expect(match).toContain("and");
		expect(match).toContain("or");
		expect(match).not.toContain("nor");
	});

	test("literal keywords: no undefined, has none", async () => {
		const grammarPath = new URL("./syntaxes/rex.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			repository: Record<string, { match?: string }>;
		};

		const match = grammar.repository["literal-keyword"]?.match ?? "";
		expect(match).not.toContain("undefined");
		expect(match).toContain("none");
		expect(match).toContain("null");
	});

	test("template literal support", async () => {
		const grammarPath = new URL("./syntaxes/rex.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			repository: Record<string, { begin?: string; end?: string; patterns?: unknown[] }>;
		};

		const tmpl = grammar.repository["template-literal"];
		expect(tmpl).toBeDefined();
		expect(tmpl?.begin).toBe("`");
		expect(tmpl?.end).toBe("`");

		const interp = grammar.repository["template-interpolation"];
		expect(interp).toBeDefined();
		expect(interp?.begin).toContain("\\{");
	});

	test("arrow operator for return types", async () => {
		const grammarPath = new URL("./syntaxes/rex.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			repository: Record<string, { match?: string }>;
		};

		const match = grammar.repository["arrow-operator"]?.match ?? "";
		expect(match).toBe("->");
	});

	test("self and nor patterns are removed", async () => {
		const grammarPath = new URL("./syntaxes/rex.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			repository: Record<string, unknown>;
		};

		expect(grammar.repository["self-depth"]).toBeUndefined();
	});

	test("no legacy patterns", async () => {
		const grammarPath = new URL("./syntaxes/rex.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			repository: Record<string, unknown>;
		};

		expect(grammar.repository["paren-expression"]).toBeUndefined();
		expect(grammar.repository["paren-expression-generic"]).toBeUndefined();
		expect(grammar.repository["interpolation"]).toBeUndefined();
	});
});

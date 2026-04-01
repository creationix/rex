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

		// Core syntax — things TM grammar handles unambiguously
		expect(includes.has("#comment")).toBe(true);
		expect(includes.has("#template-literal")).toBe(true);
		expect(includes.has("#string-single")).toBe(true);
		expect(includes.has("#string-double")).toBe(true);
		expect(includes.has("#number")).toBe(true);
		expect(includes.has("#literal-keyword")).toBe(true);
		expect(includes.has("#control-keyword")).toBe(true);
		expect(includes.has("#storage-type")).toBe(true);
		expect(includes.has("#storage-modifier")).toBe(true);
		expect(includes.has("#logical-operator")).toBe(true);
		expect(includes.has("#arrow-operator")).toBe(true);
		expect(includes.has("#comparison-operator")).toBe(true);
		expect(includes.has("#assignment-operator")).toBe(true);
		expect(includes.has("#value-operator")).toBe(true);

		// Context-dependent patterns removed — LSP semantic tokens handle these
		expect(includes.has("#object-key")).toBe(false);
		expect(includes.has("#function-call")).toBe(false);
		expect(includes.has("#identifier")).toBe(false);
		expect(includes.has("#navigation-static")).toBe(false);
		expect(includes.has("#navigation-dynamic")).toBe(false);
		expect(includes.has("#type-keyword")).toBe(false);
		expect(includes.has("#type-predicate")).toBe(false);
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

	test("storage type: extern, type", async () => {
		const grammarPath = new URL("./syntaxes/rex.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			repository: Record<string, { match?: string }>;
		};

		const match = grammar.repository["storage-type"]?.match ?? "";
		expect(match).toContain("type");
		expect(match).toContain("extern");
	});

	test("storage modifier: mut", async () => {
		const grammarPath = new URL("./syntaxes/rex.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			repository: Record<string, { match?: string }>;
		};

		const match = grammar.repository["storage-modifier"]?.match ?? "";
		expect(match).toContain("mut");
	});

	test("logical operators: and, or, not", async () => {
		const grammarPath = new URL("./syntaxes/rex.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			repository: Record<string, { match?: string }>;
		};

		const match = grammar.repository["logical-operator"]?.match ?? "";
		expect(match).toContain("and");
		expect(match).toContain("or");
		expect(match).toContain("not");
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
});

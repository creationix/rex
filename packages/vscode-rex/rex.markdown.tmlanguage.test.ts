import { describe, expect, test } from "bun:test";

describe("rex markdown TextMate grammar", () => {
	test("matches rex fenced code blocks only", async () => {
		const grammarPath = new URL("./syntaxes/rex-markdown.tmLanguage.json", import.meta.url);
		const raw = await Bun.file(grammarPath).text();
		const grammar = JSON.parse(raw) as {
			injectionSelector?: string;
			patterns?: Array<{
				begin?: string;
				end?: string;
				contentName?: string;
				patterns?: Array<{ include?: string }>;
			}>;
		};

		const firstPattern = grammar.patterns?.[0];
		expect(grammar.injectionSelector).toContain("text.html.markdown");
		expect(firstPattern?.begin).toContain("(rex)");
		expect(firstPattern?.begin?.includes("rex-infix")).toBe(false);
		expect(firstPattern?.begin).toContain("(?i:(rex))\\s*$");
		expect(firstPattern?.contentName).toBe("meta.embedded.block.rex");
		expect(firstPattern?.patterns?.[0]?.include).toBe("source.rex");
	});
});

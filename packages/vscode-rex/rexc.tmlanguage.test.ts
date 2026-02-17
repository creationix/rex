import { describe, expect, test } from "bun:test";

describe("rexc TextMate grammar", () => {
	test("includes new control-flow patterns", async () => {
		const raw = await Bun.file("./syntaxes/rexc.tmLanguage.json").text();
		const grammar = JSON.parse(raw) as {
			repository: Record<string, { patterns?: Array<{ include?: string }>; begin?: string }>;
		};

		const includes = new Set(
			(grammar.repository.value.patterns ?? [])
				.map((pattern) => pattern.include)
				.filter((include): include is string => Boolean(include)),
		);

		expect(includes.has("#control-flow-loop-paren")).toBe(true);
		expect(includes.has("#control-flow-bracket")).toBe(true);
		expect(includes.has("#control-flow-brace")).toBe(true);
		expect(includes.has("#loop-control")).toBe(true);
		expect(includes.has("#reference")).toBe(true);
		expect(includes.has("#self-depth")).toBe(true);

		expect(grammar.repository["control-flow-loop-paren"]?.begin).toContain("[><]\\(");
		expect(grammar.repository["control-flow-bracket"]?.begin).toContain("[><][\\[\\]]");
		expect(grammar.repository["control-flow-brace"]?.begin).toContain("[><]\\{");
		expect(grammar.repository["reference"]?.match).toContain("('");
		expect(grammar.repository["self-depth"]?.match).toContain("(@)");
	});
});

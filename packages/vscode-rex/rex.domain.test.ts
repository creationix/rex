import { describe, expect, test } from "bun:test";
import { entryToDetail, parseDomainSchema, resolveDomainPath } from "./src/rex-domain";

describe("rex domain schema", () => {
	test("parses valid schema", () => {
		const schema = parseDomainSchema(
			JSON.stringify({
				globals: {
					headers: {
						type: "object",
						description: "Inbound request headers",
						properties: {
							"x-action": { type: "string", description: "Action selector" },
						},
					},
				},
			}),
		);
		expect(schema).not.toBeNull();
		expect(schema?.globals?.headers?.type).toBe("object");
	});

	test("rejects invalid schema", () => {
		expect(parseDomainSchema("not json")).toBeNull();
		expect(parseDomainSchema(JSON.stringify({ globals: [] }))).toBeNull();
		expect(parseDomainSchema(JSON.stringify({ globals: { x: { type: 42 } } }))).toBeNull();
	});

	test("resolves nested path", () => {
		const schema = parseDomainSchema(
			JSON.stringify({
				globals: {
					headers: {
						type: "object",
						properties: {
							request: {
								type: "object",
								properties: {
									id: { type: "string", description: "Request id" },
								},
							},
						},
					},
				},
			}),
		)!;
		const entry = resolveDomainPath(schema, ["headers", "request", "id"]);
		expect(entry?.type).toBe("string");
		expect(entry?.description).toBe("Request id");
		expect(entryToDetail(entry!)).toBe("string");
	});
});

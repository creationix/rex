import { describe, expect, test } from "bun:test";
import {
	entryToDetail,
	parseDomainSchema,
	resolveDomainPath,
	resolveDomainPrefixMatches,
} from "./src/rex-domain";

describe("rex domain schema", () => {
	test("parses current .config.rex format and resolves dotted names", () => {
		const schema = parseDomainSchema(`
{
	data: {
		H: {
			names: ['req.headers' 'headers']
			type: 'object'
			desc: 'Inbound request headers'
		}
		M: {
			names: ['req.method']
			type: 'string'
		}
		LI: {
			names: ['log.info']
			type: 'function'
			desc: 'Info logger'
			args: { message: 'any' }
			returns: 'undefined'
		}
	}
}
`)!;

		expect(resolveDomainPath(schema, ["req", "headers"])?.type).toBe("object");
		expect(resolveDomainPath(schema, ["req", "headers"])?.description).toBe("Inbound request headers");
		expect(resolveDomainPath(schema, ["headers"])?.type).toBe("object");
		expect(resolveDomainPath(schema, ["log", "info"])?.type).toBe("function");
		expect(entryToDetail(resolveDomainPath(schema, ["log", "info"])!)).toBe("(message: any) -> undefined");

		const reqMatches = resolveDomainPrefixMatches(schema, "req");
		expect(Object.keys(reqMatches).sort()).toEqual(["headers", "method"]);

		const logMatches = resolveDomainPrefixMatches(schema, "log");
		expect(Object.keys(logMatches)).toEqual(["info"]);
	});

	test("returns null for invalid config documents", () => {
		expect(parseDomainSchema("not rex")).toBeNull();
		expect(parseDomainSchema("[]")).toBeNull();
		expect(parseDomainSchema("{data: []}")).toBeNull();
	});
});

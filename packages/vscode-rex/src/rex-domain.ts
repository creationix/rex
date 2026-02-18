import { evaluateSource } from "../../rex-lang/rexc-interpreter";

export interface RexDomainEntry {
	type?: string;
	description?: string;
	args?: Record<string, string>;
	returns?: string;
	properties?: Record<string, RexDomainEntry>;
}

export interface RexDomainSchema {
	globals?: Record<string, RexDomainEntry>;
}

export function parseDomainSchema(raw: string): RexDomainSchema | null {
	let value: unknown;
	try {
		value = evaluateSource(raw).value;
	} catch {
		return null;
	}
	if (!isObject(value)) return null;
	const globals: Record<string, RexDomainEntry> = {};
	for (const section of Object.values(value)) {
		if (!isObject(section)) continue;
		for (const entry of Object.values(section)) {
			if (!isObject(entry)) continue;
			const names = (entry as Record<string, unknown>).names;
			if (!Array.isArray(names) || names.length === 0 || names.some((name) => typeof name !== "string")) continue;
			const detail: RexDomainEntry = {};
			if (typeof (entry as Record<string, unknown>).type === "string") detail.type = (entry as Record<string, unknown>).type as string;
			if (typeof (entry as Record<string, unknown>).desc === "string") detail.description = (entry as Record<string, unknown>).desc as string;
			const argsValue = (entry as Record<string, unknown>).args;
			if (isObject(argsValue)) {
				const args: Record<string, string> = {};
				for (const [argName, argType] of Object.entries(argsValue)) {
					if (typeof argType === "string") args[argName] = argType;
				}
				if (Object.keys(args).length > 0) detail.args = args;
			}
			if (typeof (entry as Record<string, unknown>).returns === "string") detail.returns = (entry as Record<string, unknown>).returns as string;
			for (const rawName of names as string[]) {
				const path = rawName.split(".").filter((segment) => segment.length > 0);
				if (path.length === 0) continue;
				insertEntryPath(globals, path, detail);
			}
		}
	}
	return Object.keys(globals).length > 0 ? { globals } : null;
}

function insertEntryPath(globals: Record<string, RexDomainEntry>, path: string[], detail: RexDomainEntry) {
	let cursor = globals;
	for (let index = 0; index < path.length; index += 1) {
		const segment = path[index]!;
		const isLeaf = index === path.length - 1;
		const existing = cursor[segment];
		if (isLeaf) {
			if (!existing) {
				cursor[segment] = { ...detail };
			} else {
				existing.type ??= detail.type;
				existing.description ??= detail.description;
				existing.args ??= detail.args;
				existing.returns ??= detail.returns;
			}
			return;
		}

		if (!existing) {
			cursor[segment] = { type: "object", properties: {} };
		}
		const next = cursor[segment]!;
		next.properties ??= {};
		cursor = next.properties;
	}
}

export function resolveDomainPath(
	schema: RexDomainSchema,
	segments: string[],
): RexDomainEntry | null {
	if (segments.length === 0) return null;
	const globals = schema.globals;
	if (!globals) return null;
	const [head, ...tail] = segments;
	let current = globals[head];
	if (!current) return null;
	for (const segment of tail) {
		const properties = current.properties;
		if (!properties) return null;
		current = properties[segment];
		if (!current) return null;
	}
	return current;
}

export function resolveDomainPrefixMatches(
	schema: RexDomainSchema,
	prefix: string,
): Record<string, RexDomainEntry> {
	const out: Record<string, RexDomainEntry> = {};
	if (!prefix) return out;
	const globals = schema.globals;
	if (!globals) return out;

	for (const [name, entry] of collectNamedEntries(globals)) {
		if (!name.startsWith(`${prefix}.`)) continue;
		const rest = name.slice(prefix.length + 1);
		if (!rest) continue;
		const next = rest.split(".")[0];
		if (!next) continue;
		out[next] ??= entry;
	}

	return out;
}

export function entryToDetail(entry: RexDomainEntry): string {
	if (entry.args || entry.returns || entry.type?.trim() === "function") {
		const args = entry.args
			? Object.entries(entry.args).map(([name, type]) => `${name}: ${type}`).join(", ")
			: "";
		const returns = entry.returns?.trim();
		return returns ? `(${args}) -> ${returns}` : `(${args})`;
	}
	const type = entry.type?.trim();
	if (!type) return "domain symbol";
	return type;
}

function collectNamedEntries(
	map: Record<string, RexDomainEntry>,
	basePath: string[] = [],
): Array<[string, RexDomainEntry]> {
	const out: Array<[string, RexDomainEntry]> = [];
	for (const [name, entry] of Object.entries(map)) {
		const fullName = [...basePath, name].join(".");
		out.push([fullName, entry]);
		if (entry.properties) {
			out.push(...collectNamedEntries(entry.properties, [...basePath, name]));
		}
	}
	return out;
}

function isObject(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

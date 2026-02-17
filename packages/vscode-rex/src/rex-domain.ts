export interface RexDomainEntry {
	type?: string;
	description?: string;
	aliases?: string[];
	properties?: Record<string, RexDomainEntry>;
}

export interface RexDomainSchema {
	globals?: Record<string, RexDomainEntry>;
}

export function parseDomainSchema(raw: string): RexDomainSchema | null {
	let value: unknown;
	try {
		value = JSON.parse(raw);
	} catch {
		return null;
	}
	if (!isObject(value)) return null;

	const schema: RexDomainSchema = {};
	if ("globals" in value) {
		if (!isEntryMap((value as Record<string, unknown>).globals)) return null;
		schema.globals = (value as Record<string, unknown>).globals as Record<string, RexDomainEntry>;
	}
	return schema;
}

export function resolveDomainPath(
	schema: RexDomainSchema,
	segments: string[],
): RexDomainEntry | null {
	if (segments.length === 0) return null;
	const globals = schema.globals;
	if (!globals) return null;
	const [head, ...tail] = segments;
	let current = resolveEntryByName(globals, head);
	if (!current) {
		current = resolveEntryByAliasDeep(globals, head);
	}
	if (!current) return null;
	for (const segment of tail) {
		const properties = current.properties;
		if (!properties) return null;
		current = resolveEntryByName(properties, segment);
		if (!current) return null;
	}
	return current;
}

export function entryToDetail(entry: RexDomainEntry): string {
	const type = entry.type?.trim();
	if (!type) return "domain symbol";
	return type;
}

function isEntryMap(value: unknown): value is Record<string, RexDomainEntry> {
	if (!isObject(value)) return false;
	for (const key of Object.keys(value)) {
		const entry = (value as Record<string, unknown>)[key];
		if (!isEntry(entry)) return false;
	}
	return true;
}

function isEntry(value: unknown): value is RexDomainEntry {
	if (!isObject(value)) return false;
	const candidate = value as Record<string, unknown>;
	if (candidate.type !== undefined && typeof candidate.type !== "string") return false;
	if (candidate.description !== undefined && typeof candidate.description !== "string") return false;
	if (candidate.aliases !== undefined) {
		if (!Array.isArray(candidate.aliases)) return false;
		if (!candidate.aliases.every((item) => typeof item === "string")) return false;
	}
	if (candidate.properties !== undefined && !isEntryMap(candidate.properties)) return false;
	return true;
}

function resolveEntryByName(
	map: Record<string, RexDomainEntry>,
	name: string,
): RexDomainEntry | undefined {
	const exact = map[name];
	if (exact) return exact;
	for (const entry of Object.values(map)) {
		if (entry.aliases?.includes(name)) return entry;
	}
	return undefined;
}

function resolveEntryByAliasDeep(
	map: Record<string, RexDomainEntry>,
	name: string,
): RexDomainEntry | undefined {
	for (const entry of Object.values(map)) {
		if (entry.aliases?.includes(name)) return entry;
		if (entry.properties) {
			const nested = resolveEntryByAliasDeep(entry.properties, name);
			if (nested) return nested;
		}
	}
	return undefined;
}

function isObject(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

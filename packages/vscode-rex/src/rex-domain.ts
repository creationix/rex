export interface RexDomainEntry {
	type?: string;
	description?: string;
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
	const [head, ...tail] = segments;
	const globals = schema.globals;
	if (!globals) return null;
	let current = globals[head];
	if (!current) return null;
	for (const segment of tail) {
		current = current.properties?.[segment];
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
	if (candidate.properties !== undefined && !isEntryMap(candidate.properties)) return false;
	return true;
}

function isObject(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

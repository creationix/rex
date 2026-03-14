/** Parse and validate a refs JSON string. Returns the refs object. */
export function parseRefs(text: string): Record<string, unknown> {
	const trimmed = text.trim()
	if (!trimmed) return {}
	const val = JSON.parse(trimmed)
	if (typeof val !== 'object' || val === null || Array.isArray(val))
		throw new Error('Must be a JSON object')
	for (const k of Object.keys(val)) {
		if (typeof k !== 'string') throw new Error('All keys must be strings')
		if (!/^[A-Za-z0-9_-]*$/.test(k)) throw new Error(`Invalid b64 key: ${k}`)
	}
	return val
}

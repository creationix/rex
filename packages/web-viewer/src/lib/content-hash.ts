/**
 * Content-addressable hash of REXC bytes.
 * Uses FNV-1a (two independent 32-bit hashes for 64-bit collision resistance).
 * Input is the raw REXC text string (UTF-8 bytes).
 */
export function contentHash(rexcText: string): string {
	if (!rexcText) return '0'
	let h1 = 0x811c9dc5
	let h2 = 0x62b821d7
	for (let i = 0; i < rexcText.length; i++) {
		const c = rexcText.charCodeAt(i)
		h1 ^= c; h1 = Math.imul(h1, 0x01000193)
		h2 ^= c; h2 = Math.imul(h2, 0x0100019d)
	}
	return (h1 >>> 0).toString(36) + (h2 >>> 0).toString(36)
}

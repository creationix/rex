import { KIND_COLORS } from './colors.ts'

const textDecoder = new TextDecoder()

// b64 digit characters
const B64_CHARS = new Set('0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_')

// Tag characters and their kind mappings
const TAG_KINDS: Record<string, string> = {
	'+': 'number',
	'*': 'number',
	':': 'string',
	',': 'string',
	'%': 'opcode',
	'@': 'self',
	"'": 'reference',
	'$': 'variable',
	'^': 'pointer',
	';': 'loopControl',
	'.': 'pathChain',
	'=': 'set',
	'/': 'swap',
	'~': 'delete',
}

const CONTAINER_CHARS = new Set('[]{}()')
const MAX_CHARS = 500

/**
 * Returns an HTML string with color-coded REXC bytes.
 * Truncation from the left is handled by CSS (direction: rtl + text-overflow: ellipsis)
 * on the container element. We just cap the source string to MAX_CHARS from the right.
 */
export function colorizeBytes(input: Uint8Array, start: number, end: number, nodeKind: string): string {
	const len = end - start
	if (len <= 0) return ''

	const sliceStart = len > MAX_CHARS ? end - MAX_CHARS : start
	const raw = textDecoder.decode(input.subarray(sliceStart, end))

	let html = ''
	let i = 0

	while (i < raw.length) {
		const ch = raw[i]!
		let color: string
		let segEnd = i + 1

		if (CONTAINER_CHARS.has(ch)) {
			color = KIND_COLORS['object'] || '#dcdcaa'
		} else if (TAG_KINDS[ch]) {
			color = KIND_COLORS[TAG_KINDS[ch]!] || KIND_COLORS[nodeKind] || '#d4d4d4'
		} else if (B64_CHARS.has(ch)) {
			// Group consecutive b64 digits
			while (segEnd < raw.length && B64_CHARS.has(raw[segEnd]!) && !TAG_KINDS[raw[segEnd]!]) segEnd++
			color = '#999'
		} else {
			// String content or other bytes — use node kind color
			while (segEnd < raw.length && !B64_CHARS.has(raw[segEnd]!) && !CONTAINER_CHARS.has(raw[segEnd]!) && !TAG_KINDS[raw[segEnd]!]) segEnd++
			color = KIND_COLORS[nodeKind] || KIND_COLORS['string'] || '#ce9178'
		}

		const text = raw.slice(i, segEnd).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
		html += `<span style="color:${color}">${text}</span>`
		i = segEnd
	}

	return html
}

/** Color palette for REXC tag characters. */
export const TAG_COLORS: Record<string, string> = {
	':': '#dcdcaa',   // object — gold
	';': '#dcdcaa',   // array — gold
	'+': '#b5cea8',   // integer — green
	'*': '#b5cea8',   // float — green
	',': '#ce9178',   // string — orange
	"'": '#569cd6',   // ref — blue
	'^': '#c586c0',   // pointer — purple
	'.': '#c586c0',   // chain — purple
	'#': '#666666',   // index — dim gray
}

/** Color for b64 digits. */
export const B64_COLOR = '#888'

/** Color for annotations/comments. */
export const DIM_COLOR = '#555'

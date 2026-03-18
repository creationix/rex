/** Color palette for REXC tag characters — raw hex so JS can derive alpha variants (e.g. color + "22"). */
export const TAG_COLORS: Record<string, string> = {
	"'": '#fb7676',
	',': '#fbb676',
	'+': '#b6fb76',
	'*': '#76fb76',
	'key': '#76fbb6',
	'.': '#76b6fb',
	':': '#7676fb',
	'^': '#b676fb',
	';': '#fb76b6',
	'#': '#9c9c9c',
}

/** Color for b64 digits. */
export const B64_COLOR = 'var(--color-text-dim)'

/** Color for annotations/comments. */
export const DIM_COLOR = 'var(--color-text-placeholder)'
export type Pill = { label: string; color: string }

/** Tag → pill label + color, shared by data view and encoding view. */
export const PILL_INFO: Record<string, Pill> = {
	',': { label: 'str', color: TAG_COLORS[','] },
	'.': { label: 'chain', color: TAG_COLORS['.'] },
	'^': { label: 'ptr', color: TAG_COLORS['^'] },
	':': { label: 'obj', color: TAG_COLORS[':'] },
	';': { label: 'arr', color: TAG_COLORS[';'] },
	'#': { label: 'idx', color: TAG_COLORS['#'] },
	'+': { label: 'int', color: TAG_COLORS['+'] },
	'*': { label: 'dec', color: TAG_COLORS['*'] },
	"'": { label: 'ref', color: TAG_COLORS["'"] },
}

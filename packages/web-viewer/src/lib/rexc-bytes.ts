import { TAG_COLORS, B64_COLOR, DIM_COLOR } from './colors.ts'
import type { ASTNode } from '@creationix/rx'

const textDecoder = new TextDecoder()

const STR_COLOR = TAG_COLORS[',']!   // orange for string values
const NUM_COLOR = TAG_COLORS['+']!   // green for numbers
const PTR_COLOR = '#c586c0'          // purple for pointers
const CHAIN_COLOR = '#4ec9b0'        // teal for chains
const IDX_COLOR = TAG_COLORS['#']!   // gray for index
const OBJ_COLOR = TAG_COLORS[':']!   // gold for objects/arrays
const REF_COLOR = TAG_COLORS["'"]!   // blue for refs

/** Render an ASTNode as an HTML string for a single encoding row. */
export function renderNode(node: ASTNode): string {
	const tag = node.tag
	const color = TAG_COLORS[tag] || '#d4d4d4'

	// Raw bytes of the tag+b64 suffix: data[left..right)
	const raw = textDecoder.decode(node.data.subarray(node.left, node.right))

	switch (tag) {
		case ',':
			return `<span style="color:${color}">${escHtml(raw.charAt(0))}</span><span style="color:${B64_COLOR}">${escHtml(raw.slice(1))}</span>`
		case "'":
			return `<span style="color:${color}">${escHtml(raw)}</span>`
		case '+':
		case '*':
		case ':':
		case ';':
		case '^':
		case '.':
			return `<span style="color:${color}">${escHtml(raw.charAt(0))}</span><span style="color:${B64_COLOR}">${escHtml(raw.slice(1))}</span>`
		case '#':
			return `<span style="color:${color}">${escHtml(raw.charAt(0))}</span><span style="color:${B64_COLOR}">${escHtml(raw.slice(1))}</span>`
		default:
			return `<span style="color:${color}">${escHtml(raw)}</span>`
	}
}

function pill(label: string, color: string, title?: string): string {
	const tt = title ? ` title="${escHtml(title)}"` : ''
	return `<span${tt} style="background:${color}22;color:${color};border:1px solid ${color}44;border-radius:3px;padding:0 3px;font-size:10px;margin-right:3px;">${label}</span>`
}

function fmtStr(s: string): string {
	const truncated = s.length > 200 ? s.slice(0, 197) + '...' : s
	return `<span style="color:${STR_COLOR}">"${escHtml(truncated)}"</span>`
}

const TAG_PILL: Record<string, [string, string]> = {
	',': ['str', STR_COLOR],
	'.': ['chain', CHAIN_COLOR],
	'^': ['ptr', PTR_COLOR],
	':': ['obj', OBJ_COLOR],
	';': ['arr', OBJ_COLOR],
	'#': ['idx', IDX_COLOR],
	'+': ['int', NUM_COLOR],
	'*': ['dec', NUM_COLOR],
	"'": ['ref', REF_COLOR],
}

function resolveValueAnnotation(node: ASTNode): string {
	try {
		const r = node.resolve
		// Show a pill for the resolved node's type if it differs from the source
		let mid = ''
		if (r !== node && r.tag !== node.tag) {
			const p = TAG_PILL[r.tag]
			if (p) mid = pill(p[0], p[1])
		}
		const v = r.value
		if (typeof v === 'string') return `${mid}${fmtStr(v)}`
		if (typeof v === 'number' || typeof v === 'boolean') return `${mid}<span style="color:${DIM_COLOR}">${v}</span>`
		if (v === null) return `${mid}<span style="color:${DIM_COLOR}">null</span>`
		if (v === undefined) return `${mid}<span style="color:${DIM_COLOR}">undefined</span>`
		// For containers, add obj/arr pill only if not already shown via mid
		const isArr = Array.isArray(v)
		const cPill = (r.tag !== ':' && r.tag !== ';' && r.tag !== '#')
			? pill(isArr ? 'arr' : 'obj', OBJ_COLOR) : ''
		return `${mid}${cPill}<span style="color:${DIM_COLOR}">${r.length}</span>`
	} catch { /* resolve can fail on malformed data */ }
	return ''
}

/** Annotation HTML for a node (shown after the main content). */
export function annotateNode(node: ASTNode): string {
	switch (node.tag) {
		case '+': return `${pill('int', NUM_COLOR)}<span style="color:${NUM_COLOR}">${node.b64}</span>`
		case '*': return `${pill('dec', NUM_COLOR)}<span style="color:${NUM_COLOR}">${node.value}</span>`
		case ',': {
			const content = textDecoder.decode(node.data.subarray(node.left - node.size, node.left))
			return `${pill('str', STR_COLOR)}${fmtStr(content)}`
		}
		case "'": {
			const v = node.value
			if (v === null) return `${pill('ref', REF_COLOR)}<span style="color:${REF_COLOR}">null</span>`
			if (v === true) return `${pill('ref', REF_COLOR)}<span style="color:${REF_COLOR}">true</span>`
			if (v === false) return `${pill('ref', REF_COLOR)}<span style="color:${REF_COLOR}">false</span>`
			if (v === undefined) return `${pill('ref', REF_COLOR)}<span style="color:${REF_COLOR}">undefined</span>`
			if (typeof v === 'number') return `${pill('ref', REF_COLOR)}<span style="color:${REF_COLOR}">${v}</span>`
			return pill('ref', REF_COLOR)
		}
		case ':': return `${pill('obj', OBJ_COLOR)}<span style="color:${DIM_COLOR}">${node.length}</span>`
		case ';': return `${pill('arr', OBJ_COLOR)}<span style="color:${DIM_COLOR}">${node.length}</span>`
		case '^': {
			const target = String(node.left - (node.b64 as number))
			const val = resolveValueAnnotation(node)
			if (!val) return pill('ptr', PTR_COLOR, `→ @${target}`)
			return `${pill('ptr', PTR_COLOR, `→ @${target}`)}${val}`
		}
		case '.': {
			const val = resolveValueAnnotation(node)
			if (!val) return ''
			return `${pill('chain', CHAIN_COLOR)}${val}`
		}
		case '#': {
			const b64 = node.b64 as { count: number; width: number }
			return `${pill('idx', IDX_COLOR)}<span style="color:${DIM_COLOR}">${b64.count}×${b64.width}</span>`
		}
		default: return ''
	}
}

function escHtml(s: string): string {
	return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

import { TAG_COLORS, PILL_INFO, B64_COLOR, DIM_COLOR } from './colors.ts'
import type { ASTNode } from '@creationix/rx'

const textDecoder = new TextDecoder()

/** Look up pill info for a tag, returning label and color. */
function pi(tag: string) { return PILL_INFO[tag] }

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
	return `<span class="rx-pill"${tt} style="--pill-color:${color};background:${color}22;color:${color};border:1px solid ${color}44;border-radius:3px;padding:0 3px;font-size:10px;margin-right:3px;">${label}</span>`
}

/** Emit a pill from a tag character. */
function tagPill(tag: string, title?: string): string {
	const p = pi(tag)
	return p ? pill(p.label, p.color, title) : ''
}

function fmtStr(s: string): string {
	const truncated = s.length > 200 ? s.slice(0, 197) + '...' : s
	return `<span style="color:${pi(',')!.color}">"${escHtml(truncated)}"</span>`
}

function resolveValueAnnotation(node: ASTNode): string {
	try {
		const r = node.resolve
		// Show a pill for the resolved node's type if it differs from the source
		let mid = ''
		if (r !== node && r.tag !== node.tag) {
			mid = tagPill(r.tag)
		}
		const v = r.value
		if (typeof v === 'string') return `${mid}${fmtStr(v)}`
		if (typeof v === 'number' || typeof v === 'boolean') return `${mid}<span style="color:${DIM_COLOR}">${v}</span>`
		if (v === null) return `${mid}<span style="color:${DIM_COLOR}">null</span>`
		if (v === undefined) return `${mid}<span style="color:${DIM_COLOR}">undefined</span>`
		// For containers, add obj/arr pill only if not already shown via mid
		const isArr = Array.isArray(v)
		const cPill = (r.tag !== ':' && r.tag !== ';' && r.tag !== '#')
			? pill(isArr ? 'arr' : 'obj', pi(':')!.color) : ''
		return `${mid}${cPill}<span style="color:${DIM_COLOR}">${r.length}</span>`
	} catch { /* resolve can fail on malformed data */ }
	return ''
}

/** Annotation HTML for a node (shown after the main content). */
export function annotateNode(node: ASTNode): string {
	const p = pi(node.tag)
	if (!p) return ''
	const { color } = p

	switch (node.tag) {
		case '+': return `${tagPill('+')}<span style="color:${color}">${node.b64}</span>`
		case '*': return `${tagPill('*')}<span style="color:${color}">${node.value}</span>`
		case ',': {
			const content = textDecoder.decode(node.data.subarray(node.left - node.size, node.left))
			return `${tagPill(',')}${fmtStr(content)}`
		}
		case "'": {
			const v = node.value
			if (v === null) return `${tagPill("'")}<span style="color:${color}">null</span>`
			if (v === true) return `${tagPill("'")}<span style="color:${color}">true</span>`
			if (v === false) return `${tagPill("'")}<span style="color:${color}">false</span>`
			if (v === undefined) return `${tagPill("'")}<span style="color:${color}">undefined</span>`
			if (typeof v === 'number') return `${tagPill("'")}<span style="color:${color}">${v}</span>`
			return tagPill("'")
		}
		case ':': return `${tagPill(':')}<span style="color:${DIM_COLOR}">${node.length}</span>`
		case ';': return `${tagPill(';')}<span style="color:${DIM_COLOR}">${node.length}</span>`
		case '^': {
			const target = String(node.left - (node.b64 as number))
			const val = resolveValueAnnotation(node)
			if (!val) return tagPill('^', `→ @${target}`)
			return `${tagPill('^', `→ @${target}`)}${val}`
		}
		case '.': {
			const val = resolveValueAnnotation(node)
			if (!val) return ''
			return `${tagPill('.')}${val}`
		}
		case '#': {
			const b64 = node.b64 as { count: number; width: number }
			return `${tagPill('#')}<span style="color:${DIM_COLOR}">${b64.count}×${b64.width}</span>`
		}
		default: return ''
	}
}

function escHtml(s: string): string {
	return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

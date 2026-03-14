/**
 * Unified view state persistence — REXC-encoded in localStorage and URL hash.
 */

import { stringify, parse } from '@creationix/rx'
import type { Mode } from './state.svelte'

export interface FileEntry {
	id: string
	name: string
	contentHash: string
}

export interface ViewState {
	files: FileEntry[]
	current: string
	expanded: Record<string, number[]>  // kept for backward compat, not actively used
	focus: Record<string, string | null>  // kept for backward compat
	mode: Record<string, Mode>
}

const LS_KEY = 'rexc-viewer-state'

export function emptyState(): ViewState {
	return { files: [], current: '', expanded: {}, focus: {}, mode: {} }
}

function migrateMode(m: string): Mode {
	if (m === 'rexc' || m === 'json' || m === 'refs') return 'source'
	if (m === 'inspect') return 'encoding'
	if (m === 'source' || m === 'encoding' || m === 'data') return m as Mode
	return 'source'
}

export function loadState(): ViewState {
	try {
		const raw = localStorage.getItem(LS_KEY)
		if (!raw) return emptyState()
		const obj = parse(raw) as any
		const mode: Record<string, Mode> = {}
		if (obj.mode) {
			for (const [k, v] of Object.entries(obj.mode)) {
				mode[k] = migrateMode(v as string)
			}
		}
		return {
			files: Array.isArray(obj.files) ? obj.files : [],
			current: obj.current ?? '',
			expanded: obj.expanded ?? {},
			focus: obj.focus ?? {},
			mode,
		}
	} catch {
		return emptyState()
	}
}

export function saveState(state: ViewState): void {
	try {
		const rexc = stringify({
			files: state.files,
			current: state.current,
			mode: state.mode,
		})
		localStorage.setItem(LS_KEY, rexc)
	} catch { /* localStorage full or unavailable */ }
}

// --- URL hash ---

export interface HashState {
	file: string
	expanded: number[]
	focus: string | null
	mode: Mode | null
}

export function readHash(): HashState | null {
	try {
		const hash = location.hash.slice(1)
		if (!hash) return null

		const obj = parse(hash) as any
		if (obj && typeof obj === 'object' && 'file' in obj) {
			return {
				file: obj.file ?? '',
				expanded: [],
				focus: null,
				mode: obj.mode ? migrateMode(obj.mode) : null,
			}
		}

		// Legacy: mode=xxx query param format
		const params = new URLSearchParams(hash)
		const m = params.get('mode')
		if (m) {
			return { file: '', expanded: [], focus: null, mode: migrateMode(m) }
		}

		return null
	} catch {
		return null
	}
}

export function writeHash(hs: HashState, push = false): void {
	try {
		const rexc = stringify({
			file: hs.file,
			...(hs.mode ? { mode: hs.mode } : {}),
		})
		const url = '#' + rexc
		if (push) {
			history.pushState(history.state, '', url)
		} else {
			history.replaceState(history.state, '', url)
		}
	} catch { /* ignore encoding errors */ }
}

export function mergeHashIntoState(state: ViewState, hash: HashState): ViewState {
	const merged = { ...state }

	if (hash.file) {
		const found = state.files.find(f => f.contentHash === hash.file)
			?? state.files.find(f => f.name === hash.file)
		if (found) {
			merged.current = found.contentHash
			if (hash.mode) {
				merged.mode = { ...merged.mode, [found.contentHash]: hash.mode }
			}
		}
	} else if (hash.mode && merged.current) {
		merged.mode = { ...merged.mode, [merged.current]: hash.mode }
	}

	return merged
}

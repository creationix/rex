/**
 * Unified view state persistence — REXC-encoded in localStorage and URL hash.
 *
 * localStorage stores the full state:
 *   { files: [...], current, expanded: { [contentHash]: [offsets] }, focus, mode }
 *
 * URL hash stores a portable subset (raw REXC after `#`):
 *   { file: contentHash, expanded: [offsets], focus, mode }
 *
 * On load: read localStorage, overlay URL hash (matching files by content hash first, then name).
 */

import { stringify, parse } from '../../../rex-lang/rexc.ts'
import type { Mode } from './state.svelte'

export interface FileEntry {
	id: string
	name: string
	contentHash: string
}

export interface ViewState {
	files: FileEntry[]
	current: string  // contentHash of active file
	expanded: Record<string, number[]>  // contentHash → byte offsets of expanded nodes
	focus: Record<string, string | null>  // contentHash → focus path
	mode: Record<string, Mode>  // contentHash → view mode
}

const LS_KEY = 'rexc-viewer-state'

export function emptyState(): ViewState {
	return { files: [], current: '', expanded: {}, focus: {}, mode: {} }
}

// --- localStorage (full state, REXC-encoded) ---

export function loadState(): ViewState {
	try {
		const raw = localStorage.getItem(LS_KEY)
		if (!raw) return emptyState()
		const obj = parse(raw) as any
		return {
			files: Array.isArray(obj.files) ? obj.files : [],
			current: obj.current ?? '',
			expanded: obj.expanded ?? {},
			focus: obj.focus ?? {},
			mode: obj.mode ?? {},
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
			expanded: state.expanded,
			focus: state.focus,
			mode: state.mode,
		})
		localStorage.setItem(LS_KEY, rexc)
	} catch { /* localStorage full or unavailable */ }
}

// --- URL hash (portable subset, raw REXC in fragment) ---

export interface HashState {
	file: string  // contentHash
	expanded: number[]
	focus: string | null
	mode: Mode | null
}

export function readHash(): HashState | null {
	try {
		const hash = location.hash.slice(1)
		if (!hash) return null

		// Try parsing as raw REXC
		const obj = parse(hash) as any
		if (obj && typeof obj === 'object' && 'file' in obj) {
			return {
				file: obj.file ?? '',
				expanded: Array.isArray(obj.expanded) ? obj.expanded : [],
				focus: obj.focus ?? null,
				mode: obj.mode ?? null,
			}
		}

		// Legacy: mode=xxx query param format
		const params = new URLSearchParams(hash)
		const m = params.get('mode')
		if (m && ['rexc', 'inspect', 'json', 'refs'].includes(m)) {
			return { file: '', expanded: [], focus: null, mode: m as Mode }
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
			...(hs.expanded.length > 0 ? { expanded: hs.expanded } : {}),
			...(hs.focus ? { focus: hs.focus } : {}),
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

// --- Merge: hash state into saved state ---

export function mergeHashIntoState(state: ViewState, hash: HashState): ViewState {
	const merged = { ...state }

	if (hash.file) {
		// Try to find file by content hash first, then by name
		const found = state.files.find(f => f.contentHash === hash.file)
			?? state.files.find(f => f.name === hash.file)
		if (found) {
			merged.current = found.contentHash
			if (hash.expanded.length > 0) {
				merged.expanded = { ...merged.expanded, [found.contentHash]: hash.expanded }
			}
			if (hash.focus) {
				merged.focus = { ...merged.focus, [found.contentHash]: hash.focus }
			}
			if (hash.mode) {
				merged.mode = { ...merged.mode, [found.contentHash]: hash.mode }
			}
		}
		// If not found by hash or name, the file isn't available locally — hash state is ignored
	} else if (hash.mode && merged.current) {
		// Legacy: just mode, apply to current file
		merged.mode = { ...merged.mode, [merged.current]: hash.mode }
	}

	return merged
}

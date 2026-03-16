/**
 * Unified view state persistence — REXC-encoded in localStorage.
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
	expanded: Record<string, number[]>  // opened nodes keyed by contentHash
	focus: Record<string, number | null>
	mode: Record<string, Mode>
	pane: Record<string, 'data' | 'encoding'>
}

const LS_KEY = 'rexc-viewer-state'

export function emptyState(): ViewState {
	return { files: [], current: '', expanded: {}, focus: {}, mode: {}, pane: {} }
}

function migrateMode(m: string): Mode {
	if (m === 'rexc' || m === 'json' || m === 'refs' || m === 'source') return 'data'
	if (m === 'inspect') return 'encoding'
	if (m === 'encoding' || m === 'data' || m === 'split') return m as Mode
	return 'data'
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
		const expanded: Record<string, number[]> = {}
		if (obj.expanded) {
			for (const [k, v] of Object.entries(obj.expanded)) {
				expanded[k] = Array.isArray(v) ? (v as number[]) : []
			}
		}
		const files: ViewState['files'] = []
		if (Array.isArray(obj.files)) {
			for (const f of obj.files) {
				files.push({ id: f.id, name: f.name, contentHash: f.contentHash })
			}
		}
		const focus: Record<string, number | null> = {}
		if (obj.focus) {
			for (const [k, v] of Object.entries(obj.focus)) {
				focus[k] = typeof v === 'number' ? v : null
			}
		}
		const pane: Record<string, 'data' | 'encoding'> = {}
		if (obj.pane) {
			for (const [k, v] of Object.entries(obj.pane)) {
				pane[k] = v === 'encoding' ? 'encoding' : 'data'
			}
		}
		return {
			files,
			current: obj.current ?? '',
			expanded,
			focus,
			mode,
			pane,
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
			expanded: state.expanded,
			focus: state.focus,
			pane: state.pane,
		})
		localStorage.setItem(LS_KEY, rexc)
	} catch { /* localStorage full or unavailable */ }
}


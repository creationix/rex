/**
 * Multi-document management layer.
 * Sits on top of appState and IndexedDB — tracks open documents as tabs.
 */

import { appState, type Mode } from './state.svelte'
import { listDocs, putDoc, deleteDoc, type DocRecord } from './db.ts'
import { contentHash } from './content-hash.ts'
import { loadState, saveState, readHash, writeHash, mergeHashIntoState, emptyState, type ViewState, type FileEntry } from './viewstate.ts'

export interface DocTab {
	id: string
	name: string
	contentHash: string
	saved: boolean  // false = unsaved scratch document
}

function generateId(): string {
	return crypto.randomUUID()
}

function createDocStore() {
	let tabs = $state<DocTab[]>([])
	let activeId = $state<string>('')
	let loaded = $state(false)

	// Live view state — updated incrementally, serialized on persist
	let vs: ViewState = emptyState()

	// Snapshot of each tab's content (in-memory cache so switching is instant)
	const snapshots = new Map<string, {
		rexcText: string
		jsonText: string
		refsText: string
		refsEnabled: boolean
		mode: Mode
		expandedOffsets: number[]
		focusPath: string | null
	}>()

	function activeHash(): string {
		return tabs.find(t => t.id === activeId)?.contentHash ?? ''
	}

	function snapshotCurrent() {
		if (!activeId) return
		snapshots.set(activeId, {
			rexcText: appState.rexcText,
			jsonText: appState.jsonText,
			refsText: appState.refsText,
			refsEnabled: appState.refsEnabled,
			mode: appState.mode,
			expandedOffsets: appState.expandedOffsets,
			focusPath: appState.focusPath,
		})
		// Update content hash
		const tab = tabs.find(t => t.id === activeId)
		if (tab) tab.contentHash = contentHash(appState.rexcText)
	}

	/** Sync the active tab's ephemeral state into the live ViewState */
	function syncActiveToVs() {
		const ch = activeHash()
		if (!ch || ch === '0') return
		vs.current = ch
		vs.expanded[ch] = appState.expandedOffsets
		vs.focus[ch] = appState.focusPath
		vs.mode[ch] = appState.mode
	}

	/** Rebuild vs.files from tabs (only needed on save/close/new) */
	function syncFilesToVs() {
		vs.files = tabs
			.filter(t => t.saved)
			.map(t => ({ id: t.id, name: t.name, contentHash: t.contentHash }))
	}

	function restoreSnapshot(id: string) {
		const snap = snapshots.get(id)
		appState.restore(snap ?? { rexcText: '', jsonText: '', refsText: '{}', refsEnabled: false, mode: 'rexc', expandedOffsets: [], focusPath: null })
	}

	/** Initialize: load saved docs from IndexedDB, merge viewstate + hash */
	async function init() {
		const saved = await listDocs()
		const savedTabs: DocTab[] = saved.map(d => ({
			id: d.id, name: d.name, contentHash: d.contentHash || contentHash(d.rexcText), saved: true,
		}))

		// Cache snapshots from DB
		for (const d of saved) {
			snapshots.set(d.id, {
				rexcText: d.rexcText,
				jsonText: d.jsonText,
				refsText: d.refsText,
				refsEnabled: d.refsEnabled,
				mode: d.mode as Mode,
				expandedOffsets: [],
				focusPath: null,
			})
		}

		// Load viewstate from localStorage, merge URL hash
		vs = loadState()
		const hash = readHash()
		if (hash) vs = mergeHashIntoState(vs, hash)

		// Apply expanded/focus from viewstate to snapshots
		for (const d of saved) {
			const ch = savedTabs.find(t => t.id === d.id)?.contentHash ?? ''
			const snap = snapshots.get(d.id)
			if (snap && ch) {
				snap.expandedOffsets = vs.expanded[ch] ?? []
				snap.focusPath = vs.focus[ch] ?? null
				const m = vs.mode[ch]
				if (m) snap.mode = m
			}
		}

		// Always start with a scratch tab
		const scratchId = generateId()
		tabs = [{ id: scratchId, name: 'untitled', contentHash: '0', saved: false }, ...savedTabs]

		// Try to restore the active file from viewstate
		if (vs.current) {
			const match = savedTabs.find(t => t.contentHash === vs.current)
			if (match) {
				activeId = match.id
				restoreSnapshot(match.id)
			} else {
				activeId = scratchId
			}
		} else {
			activeId = scratchId
		}

		loaded = true
		syncFilesToVs()
		saveState(vs)
	}

	/** Serialize live ViewState to localStorage */
	function persistViewState() {
		syncActiveToVs()
		saveState(vs)
	}

	/** Update URL hash with current file's state */
	function updateUrlHash(push = false) {
		const ch = activeHash()
		writeHash({
			file: ch,
			expanded: appState.expandedOffsets,
			focus: appState.focusPath,
			mode: appState.mode,
		}, push)
	}

	function switchTab(id: string) {
		if (id === activeId) return
		snapshotCurrent()
		syncActiveToVs()
		activeId = id
		restoreSnapshot(id)
		syncActiveToVs()
		saveState(vs)
		updateUrlHash(true)
	}

	function newTab() {
		snapshotCurrent()
		syncActiveToVs()
		const id = generateId()
		tabs = [{ id, name: 'untitled', contentHash: '0', saved: false }, ...tabs]
		activeId = id
		restoreSnapshot(id) // blank
		saveState(vs)
		updateUrlHash()
	}

	async function saveCurrentAs(name: string) {
		snapshotCurrent()
		const tab = tabs.find(t => t.id === activeId)
		if (!tab) return

		tab.name = name
		tab.saved = true
		tabs = [...tabs] // trigger reactivity

		const snap = snapshots.get(activeId)!
		await putDoc({
			id: activeId,
			name,
			contentHash: tab.contentHash,
			rexcText: snap.rexcText,
			jsonText: snap.jsonText,
			refsText: snap.refsText,
			refsEnabled: snap.refsEnabled,
			mode: snap.mode,
			updatedAt: Date.now(),
		})
		syncFilesToVs()
		syncActiveToVs()
		saveState(vs)
		updateUrlHash()
	}

	async function saveCurrent() {
		const tab = tabs.find(t => t.id === activeId)
		if (!tab || !tab.saved) return
		await saveCurrentAs(tab.name)
	}

	async function closeTab(id: string) {
		const idx = tabs.findIndex(t => t.id === id)
		if (idx === -1) return

		// Clean up closed tab's data from vs
		const closedTab = tabs[idx]
		if (closedTab.contentHash && closedTab.contentHash !== '0') {
			delete vs.expanded[closedTab.contentHash]
			delete vs.focus[closedTab.contentHash]
			delete vs.mode[closedTab.contentHash]
		}

		tabs = tabs.filter(t => t.id !== id)
		snapshots.delete(id)

		// If closing the active tab, switch to nearest
		if (id === activeId) {
			if (tabs.length === 0) {
				newTab()
			} else {
				const nextIdx = Math.min(idx, tabs.length - 1)
				activeId = tabs[nextIdx].id
				restoreSnapshot(activeId)
			}
		}
		syncFilesToVs()
		syncActiveToVs()
		saveState(vs)
		updateUrlHash()
	}

	async function deleteTab(id: string) {
		const tab = tabs.find(t => t.id === id)
		if (tab?.saved) await deleteDoc(id)
		await closeTab(id)
	}

	async function autoSave() {
		const tab = tabs.find(t => t.id === activeId)
		if (!tab?.saved) return
		snapshotCurrent()
		const snap = snapshots.get(activeId)
		if (!snap) return
		await putDoc({
			id: activeId,
			name: tab.name,
			contentHash: tab.contentHash,
			rexcText: snap.rexcText,
			jsonText: snap.jsonText,
			refsText: snap.refsText,
			refsEnabled: snap.refsEnabled,
			mode: snap.mode,
			updatedAt: Date.now(),
		})
		syncActiveToVs()
		saveState(vs)
	}

	return {
		get tabs() { return tabs },
		get activeId() { return activeId },
		get loaded() { return loaded },
		get currentTab() { return tabs.find(t => t.id === activeId) },
		init,
		switchTab,
		newTab,
		saveCurrentAs,
		saveCurrent,
		closeTab,
		deleteTab,
		autoSave,
		persistViewState,
		updateUrlHash,
	}
}

export const docStore = createDocStore()

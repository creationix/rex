/**
 * Multi-document management layer.
 * Sits on top of appState and IndexedDB — tracks open documents as tabs.
 */

import { appState, type Mode, type SourceFormat } from './state.svelte'
import { listDocs, putDoc, deleteDoc, type DocRecord } from './db.ts'
import { contentHash } from './content-hash.ts'
import { loadState, saveState, readHash, writeHash, mergeHashIntoState, emptyState, type ViewState, type FileEntry } from './viewstate.ts'

export interface DocTab {
	id: string
	name: string
	contentHash: string
	saved: boolean
}

function generateId(): string {
	return crypto.randomUUID()
}

/** Migrate old mode values to new ones. */
function migrateMode(m: string): Mode {
	if (m === 'rexc' || m === 'json' || m === 'refs') return 'source'
	if (m === 'inspect') return 'encoding'
	if (m === 'source' || m === 'encoding' || m === 'data') return m as Mode
	return 'source'
}

function createDocStore() {
	let tabs = $state<DocTab[]>([])
	let activeId = $state<string>('')
	let loaded = $state(false)

	let vs: ViewState = emptyState()

	const snapshots = new Map<string, {
		rexcText: string
		jsonText: string
		refsText: string
		refsEnabled: boolean
		mode: Mode
		sourceFormat: SourceFormat
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
			sourceFormat: appState.sourceFormat,
		})
		const tab = tabs.find(t => t.id === activeId)
		if (tab) tab.contentHash = contentHash(appState.rexcText)
	}

	function syncActiveToVs() {
		const ch = activeHash()
		if (!ch || ch === '0') return
		vs.current = ch
		vs.mode[ch] = appState.mode
	}

	function syncFilesToVs() {
		vs.files = tabs
			.filter(t => t.saved)
			.map(t => ({ id: t.id, name: t.name, contentHash: t.contentHash }))
	}

	function restoreSnapshot(id: string) {
		const snap = snapshots.get(id)
		appState.restore(snap ?? { rexcText: '', jsonText: '', refsText: '{}', refsEnabled: false, mode: 'source', sourceFormat: 'rexc' })
	}

	async function init() {
		const saved = await listDocs()
		const savedTabs: DocTab[] = saved.map(d => ({
			id: d.id, name: d.name, contentHash: d.contentHash || contentHash(d.rexcText), saved: true,
		}))

		for (const d of saved) {
			snapshots.set(d.id, {
				rexcText: d.rexcText,
				jsonText: d.jsonText,
				refsText: d.refsText,
				refsEnabled: d.refsEnabled,
				mode: migrateMode(d.mode),
				sourceFormat: (d as any).sourceFormat === 'json' ? 'json' : 'rexc',
			})
		}

		vs = loadState()
		const hash = readHash()
		if (hash) vs = mergeHashIntoState(vs, hash)

		// Apply mode from viewstate to snapshots (with migration)
		for (const d of saved) {
			const ch = savedTabs.find(t => t.id === d.id)?.contentHash ?? ''
			const snap = snapshots.get(d.id)
			if (snap && ch) {
				const m = vs.mode[ch]
				if (m) snap.mode = migrateMode(m)
			}
		}

		if (savedTabs.length === 0) {
			// No saved docs — create a scratch tab
			const scratchId = generateId()
			tabs = [{ id: scratchId, name: 'untitled', contentHash: '0', saved: false }]
			activeId = scratchId
		} else {
			tabs = savedTabs
			if (vs.current) {
				const match = savedTabs.find(t => t.contentHash === vs.current)
				activeId = match ? match.id : savedTabs[0].id
			} else {
				activeId = savedTabs[0].id
			}
			restoreSnapshot(activeId)
		}

		loaded = true
		syncFilesToVs()
		saveState(vs)

		// Background sync so both formats are available for stats
		if (appState.rexcText && !appState.jsonFresh) appState.syncJson()
		else if (appState.jsonText && !appState.rexcFresh) appState.syncRexc()
	}

	function persistViewState() {
		syncActiveToVs()
		saveState(vs)
	}

	function updateUrlHash(push = false) {
		const ch = activeHash()
		writeHash({
			file: ch,
			expanded: [],
			focus: null,
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
		restoreSnapshot(id)
		saveState(vs)
		updateUrlHash()
	}

	async function saveCurrentAs(name: string) {
		snapshotCurrent()
		const tab = tabs.find(t => t.id === activeId)
		if (!tab) return

		tab.name = name
		tab.saved = true
		tabs = [...tabs]

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

		const closedTab = tabs[idx]
		if (closedTab.contentHash && closedTab.contentHash !== '0') {
			delete vs.mode[closedTab.contentHash]
		}

		tabs = tabs.filter(t => t.id !== id)
		snapshots.delete(id)

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
		renameCurrentTab(name: string) {
			const tab = tabs.find(t => t.id === activeId)
			if (tab) { tab.name = name; tabs = [...tabs] }
		},
		persistViewState,
		updateUrlHash,
	}
}

export const docStore = createDocStore()

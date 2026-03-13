import { workerCall, isStale } from './worker.ts'

export type Mode = 'rexc' | 'inspect' | 'json' | 'refs'

function createState() {
	let mode = $state<Mode>('rexc')
	let rexcText = $state('')
	let jsonText = $state('')
	let rexcFresh = $state(true)   // rexc is the canonical source on init
	let jsonFresh = $state(true)   // json is trivially fresh (empty)
	let refsText = $state('{}')
	let refsEnabled = $state(false)
	let converting = $state(false)
	let error = $state<string | null>(null)
	let refs = $state<Record<string, unknown>>({})
	let expandedOffsets = $state<number[]>([])
	let focusPath = $state<string | null>(null)

	const rexcSize = $derived(rexcText.length)
	const jsonSize = $derived(jsonText.length)

	function activeRefs(): Record<string, unknown> {
		return refsEnabled ? refs : {}
	}

	/** Sync rexc→json via worker. No-op if json is already fresh. */
	async function syncJson(): Promise<void> {
		if (jsonFresh) return
		if (!rexcText.trim()) { jsonText = ''; jsonFresh = true; return }
		converting = true
		error = null
		try {
			const { id, promise } = workerCall({ type: 'rexc-to-json', rexc: rexcText.trim(), refs: activeRefs() })
			const result = await promise
			if (!isStale(id)) {
				jsonText = result
				jsonFresh = true
			}
		} catch (e: any) {
			error = e.message
		} finally {
			converting = false
		}
	}

	/** Sync json→rexc via worker. No-op if rexc is already fresh. */
	async function syncRexc(): Promise<void> {
		if (rexcFresh) return
		if (!jsonText.trim()) { rexcText = ''; rexcFresh = true; return }
		converting = true
		error = null
		try {
			const { id, promise } = workerCall({ type: 'json-to-rexc', json: jsonText.trim(), refs: activeRefs() })
			const result = await promise
			if (!isStale(id)) {
				rexcText = result
				rexcFresh = true
			}
		} catch (e: any) {
			error = e.message
		} finally {
			converting = false
		}
	}

	function setRexc(text: string) {
		rexcText = text
		rexcFresh = true
		jsonFresh = false   // json is now stale
		error = null
	}

	function setJson(text: string) {
		jsonText = text
		jsonFresh = true
		rexcFresh = false   // rexc is now stale
		error = null
	}

	/** Restore all content at once (e.g. switching tabs). No staleness cross-contamination. */
	function restore(snap: { rexcText: string, jsonText: string, refsText: string, refsEnabled: boolean, mode: Mode, expandedOffsets: number[], focusPath: string | null }) {
		rexcText = snap.rexcText
		jsonText = snap.jsonText
		rexcFresh = true
		jsonFresh = true
		refsEnabled = snap.refsEnabled
		mode = snap.mode
		expandedOffsets = snap.expandedOffsets
		focusPath = snap.focusPath
		error = null
		setRefs(snap.refsText)
	}

	function setRefs(text: string) {
		refsText = text
		try {
			const trimmed = text.trim()
			if (!trimmed || trimmed === '{}') { refs = {}; return }
			const val = JSON.parse(trimmed)
			if (typeof val === 'object' && val !== null && !Array.isArray(val)) {
				refs = val
			}
		} catch { /* invalid JSON, don't update refs */ }
	}

	async function switchMode(newMode: Mode) {
		if (newMode === mode) return
		// Sync whichever format the target view needs
		if (newMode === 'json') await syncJson()
		if (newMode === 'inspect' || newMode === 'rexc') await syncRexc()
		mode = newMode
	}

	function copyCurrentView(): string {
		switch (mode) {
			case 'rexc':
			case 'inspect':
				return rexcText
			case 'json':
				return jsonText
			case 'refs':
				return refsText
		}
	}

	return {
		get mode() { return mode },
		set mode(v: Mode) { mode = v },
		get rexcText() { return rexcText },
		get jsonText() { return jsonText },
		get refsText() { return refsText },
		get refsEnabled() { return refsEnabled },
		set refsEnabled(v: boolean) { refsEnabled = v },
		get rexcFresh() { return rexcFresh },
		get jsonFresh() { return jsonFresh },
		get converting() { return converting },
		get error() { return error },
		get refs() { return refs },
		get rexcSize() { return rexcSize },
		get jsonSize() { return jsonSize },
		get expandedOffsets() { return expandedOffsets },
		set expandedOffsets(v: number[]) { expandedOffsets = v },
		get focusPath() { return focusPath },
		set focusPath(v: string | null) { focusPath = v },
		setRexc,
		setJson,
		setRefs,
		restore,
		switchMode,
		syncJson,
		syncRexc,
		copyCurrentView,
	}
}

export const appState = createState()

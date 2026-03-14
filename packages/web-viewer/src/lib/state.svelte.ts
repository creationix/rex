import { workerCall, isStale } from './worker.ts'

export type Mode = 'source' | 'encoding' | 'data'
export type SourceFormat = 'rexc' | 'json'

function createState() {
	let mode = $state<Mode>('source')
	let sourceFormat = $state<SourceFormat>('rexc')
	let rexcText = $state('')
	let jsonText = $state('')
	let rexcFresh = $state(true)
	let jsonFresh = $state(true)
	let refsText = $state('{}')
	let refsEnabled = $state(false)
	let refsOpen = $state(false)
	let converting = $state(false)
	let error = $state<string | null>(null)
	let refs = $state<Record<string, unknown>>({})

	const rexcSize = $derived(rexcText.length)
	const jsonSize = $derived(jsonText.length)

	function activeRefs(): Record<string, unknown> {
		return refsEnabled ? refs : {}
	}

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
		jsonFresh = false
		error = null
	}

	function setJson(text: string) {
		jsonText = text
		jsonFresh = true
		rexcFresh = false
		error = null
	}

	function restore(snap: { rexcText: string, jsonText: string, refsText: string, refsEnabled: boolean, mode: Mode, sourceFormat: SourceFormat }) {
		rexcText = snap.rexcText
		jsonText = snap.jsonText
		rexcFresh = true
		jsonFresh = true
		refsEnabled = snap.refsEnabled
		mode = snap.mode
		sourceFormat = snap.sourceFormat
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
		if (newMode === 'encoding' || newMode === 'data') await syncRexc()
		if (newMode === 'source' && sourceFormat === 'json') await syncJson()
		mode = newMode
	}

	function copyCurrentView(): string {
		switch (mode) {
			case 'source':
				return sourceFormat === 'json' ? jsonText : rexcText
			case 'encoding':
			case 'data':
				return rexcText
		}
	}

	function loadFile(name: string, content: string) {
		const trimmed = content.trimStart()
		if (/^[\[{"0-9tfn\-]/.test(trimmed)) {
			try {
				JSON.parse(content)
				sourceFormat = 'json'
				setJson(content)
				syncRexc()
				mode = 'data'
				return
			} catch { /* not JSON */ }
		}
		sourceFormat = 'rexc'
		setRexc(content)
		syncJson()
		mode = 'data'
	}

	return {
		get mode() { return mode },
		set mode(v: Mode) { mode = v },
		get sourceFormat() { return sourceFormat },
		set sourceFormat(v: SourceFormat) { sourceFormat = v },
		get rexcText() { return rexcText },
		get jsonText() { return jsonText },
		get refsText() { return refsText },
		get refsEnabled() { return refsEnabled },
		set refsEnabled(v: boolean) { refsEnabled = v },
		get refsOpen() { return refsOpen },
		set refsOpen(v: boolean) { refsOpen = v },
		get rexcFresh() { return rexcFresh },
		get jsonFresh() { return jsonFresh },
		get converting() { return converting },
		get error() { return error },
		get refs() { return refs },
		get rexcSize() { return rexcSize },
		get jsonSize() { return jsonSize },
		setRexc,
		setJson,
		setRefs,
		restore,
		switchMode,
		syncJson,
		syncRexc,
		copyCurrentView,
		loadFile,
	}
}

export const appState = createState()

import { workerCall, isStale } from './worker.ts'
import { read, makeCursor, inspect, open, type ASTNode } from '@creationix/rx'

export type Mode = 'source' | 'encoding' | 'data' | 'split'
export type SourceFormat = 'rexc' | 'json'

function createState() {
	let mode = $state<Mode>('data')
	let sourceFormat = $state<SourceFormat>('rexc')
	let rexcText = $state.raw('')
	let jsonText = $state.raw('')
	let compactJsonSize = $state(0)
	let rexcFresh = $state(true)
	let jsonFresh = $state(true)
	let refsText = $state('{}')
	let refsEnabled = $state(false)
	let refsOpen = $state(false)
	let converting = $state(false)
	let error = $state<string | null>(null)
	let refs = $state.raw<Record<string, unknown>>({})
	let activePane = $state<'data' | 'encoding'>('data')
	let lastFocusedNodeRight: number | null = null
	let focusSyncListeners: Array<(nodeRight: number, source: 'data' | 'encoding') => void> = []

	// Shared opened set — keyed by node.right, used by both views
	let opened = new Set<number>()
	let openedVersion = $state(0)  // bumped on every change to trigger view updates
	let expandListeners: Array<(nodeRight: number, expanded: boolean) => void> = []

	// Shared parsed trees — regenerated only when rexcText or refs change
	// These are intentionally outside Svelte's reactive system to avoid proxy wrapping
	let parsedVersion = $state(0)
	const parsed: { inspect: ASTNode | null; open: unknown; error: string | null } = {
		inspect: null,
		open: null,
		error: null,
	}

	function rebuildParsed() {
		parsed.inspect = null
		parsed.open = null
		parsed.error = null
		opened = new Set()
		const trimmed = rexcText.trim()
		if (trimmed) {
			try {
				const buf = new TextEncoder().encode(trimmed)
				const r = refsEnabled ? refs : undefined
				parsed.inspect = inspect(buf, r)
				parsed.open = open(buf, r)
			} catch (e: any) {
				parsed.error = e.message
			}
		}
		parsedVersion++
	}

	const rexcSize = $derived(rexcText.length)
	const jsonSize = $derived(jsonText.length)

	function activeRefs(): Record<string, unknown> {
		return refsEnabled ? refs : {}
	}

	async function syncJson(): Promise<void> {
		if (jsonFresh) return
		if (!rexcText.trim()) { jsonText = ''; compactJsonSize = 0; jsonFresh = true; return }
		converting = true
		error = null
		try {
			const { id, promise } = workerCall({ type: 'rexc-to-json', rexc: rexcText.trim(), refs: activeRefs() })
			const { result, compactSize } = await promise
			if (!isStale(id)) {
				jsonText = result
				compactJsonSize = compactSize ?? 0
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
			const { result, compactSize } = await promise
			if (!isStale(id)) {
				rexcText = result
				compactJsonSize = compactSize ?? compactJsonSize
				rexcFresh = true
				rebuildParsed()
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
		jsonText = ''  // free memory — will be regenerated on demand
		compactJsonSize = 0
		error = null
		rebuildParsed()
		// Compute compact JSON size in background
		if (text.trim()) {
			const { id, promise } = workerCall({ type: 'rexc-compact-size', rexc: text.trim(), refs: activeRefs() })
			promise.then(({ compactSize }) => {
				if (!isStale(id)) compactJsonSize = compactSize ?? 0
			}).catch(() => {})
		}
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
		rexcFresh = !!snap.rexcText || !snap.jsonText
		jsonFresh = !!snap.jsonText || !snap.rexcText
		refsEnabled = snap.refsEnabled
		mode = snap.mode
		sourceFormat = snap.sourceFormat
		error = null
		compactJsonSize = 0
		setRefs(snap.refsText)  // calls rebuildParsed via setRefs
		// Compute compact JSON size in background
		if (rexcText.trim()) {
			const { id, promise } = workerCall({ type: 'rexc-compact-size', rexc: rexcText.trim(), refs: activeRefs() })
			promise.then(({ compactSize }) => {
				if (!isStale(id)) compactJsonSize = compactSize ?? 0
			}).catch(() => {})
		}
	}

	function setRefs(text: string) {
		refsText = text
		try {
			const trimmed = text.trim()
			if (!trimmed || trimmed === '{}') { refs = {}; rebuildParsed(); return }
			const val = JSON.parse(trimmed)
			if (typeof val === 'object' && val !== null && !Array.isArray(val)) {
				refs = val
			}
		} catch { /* invalid JSON, don't update refs */ }
		rebuildParsed()
	}

	async function switchMode(newMode: Mode) {
		if (newMode === mode) return
		await syncRexc()
		mode = newMode
	}

	function isValidRexc(text: string): boolean {
		try {
			const buf = new TextEncoder().encode(text)
			const c = makeCursor(buf)
			read(c)
			return c.left === 0
		} catch { return false }
	}

	function loadFile(name: string, content: string) {
		const trimmed = content.trim()
		if (trimmed && isValidRexc(trimmed)) {
			sourceFormat = 'rexc'
			setRexc(content)
			mode = 'data'
		} else {
			sourceFormat = 'json'
			setJson(content)
			syncRexc()
			mode = 'data'
		}
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
		get activePane() { return activePane },
		set activePane(v: 'data' | 'encoding') { activePane = v },
		notifyFocusSync(nodeRight: number, source: 'data' | 'encoding') {
			activePane = source
			lastFocusedNodeRight = nodeRight
			for (const fn of focusSyncListeners) fn(nodeRight, source)
		},
		get lastFocusedNodeRight() { return lastFocusedNodeRight },
		onFocusSync(fn: (nodeRight: number, source: 'data' | 'encoding') => void): () => void {
			focusSyncListeners.push(fn)
			return () => { focusSyncListeners = focusSyncListeners.filter(f => f !== fn) }
		},
		get rexcSize() { return rexcSize },
		get jsonSize() { return jsonSize },
		get compactJsonSize() { return compactJsonSize },
		get opened() { return opened },
		get openedVersion() { return openedVersion },
		isOpened(nodeRight: number) { return opened.has(nodeRight) },
		toggleOpened(nodeRight: number) {
			const expanded = !opened.has(nodeRight)
			if (expanded) opened.add(nodeRight)
			else opened.delete(nodeRight)
			openedVersion++
			for (const fn of expandListeners) fn(nodeRight, expanded)
		},
		setOpened(nodeRight: number) {
			if (opened.has(nodeRight)) return
			opened.add(nodeRight)
			openedVersion++
			for (const fn of expandListeners) fn(nodeRight, true)
		},
		onExpandChange(fn: (nodeRight: number, expanded: boolean) => void): () => void {
			expandListeners.push(fn)
			return () => { expandListeners = expandListeners.filter(f => f !== fn) }
		},
		get parsedVersion() { return parsedVersion },
		get parsedInspect() { return parsed.inspect },
		get parsedOpen() { return parsed.open },
		get parsedError() { return parsed.error },
		setRexc,
		setJson,
		setRefs,
		restore,
		switchMode,
		syncJson,
		syncRexc,
		isValidRexc,
		loadFile,
	}
}

export const appState = createState()

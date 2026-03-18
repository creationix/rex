/**
 * Webview-compatible worker module.
 * In VS Code webviews, Web Workers loaded from vscode-webview-resource:// URLs
 * may fail. We wrap creation in a try-catch so the app still works without
 * search and background JSON conversion.
 */

export type { WorkerRequestBody, WorkerRequest, WorkerResponse, SearchHit, WorkerResult } from './worker'

let worker: Worker | null = null
let workerSeq = 0

try {
	// Avoid Vite's `new Worker(new URL(...))` detection so it doesn't inline import.meta.url
	const workerURL = new URL('../decode-worker.ts', (import.meta as any).url)
	worker = new Worker(workerURL, { type: 'module' })
} catch {
	// Worker unavailable in this context — features that depend on it will be silently disabled
}

export type WorkerResult = { result: string; compactSize?: number }

export function workerCall(req: import('./worker').WorkerRequestBody): { id: number; promise: Promise<WorkerResult> } {
	const id = ++workerSeq
	if (!worker) {
		return { id, promise: Promise.reject(new Error('Worker unavailable')) }
	}
	const w = worker
	const promise = new Promise<WorkerResult>((resolve, reject) => {
		function handler(e: MessageEvent) {
			if (e.data.id !== id) return
			if (!('ok' in e.data)) return
			w.removeEventListener('message', handler)
			if (e.data.ok) resolve({ result: e.data.result, compactSize: e.data.compactSize })
			else reject(new Error(e.data.error))
		}
		w.addEventListener('message', handler)
	})
	w.postMessage(Object.assign({ id }, req))
	return { id, promise }
}

export function workerSearchStream(
	args: { rexc: string; refs: Record<string, unknown>; query: string; limit?: number },
	onHit: (hits: import('./worker').SearchHit[]) => void,
	onDone: (info: { total: number; truncated: boolean }) => void,
	onError: (error: Error) => void,
): { id: number; cancel: () => void } {
	const id = ++workerSeq
	if (!worker) {
		onError(new Error('Worker unavailable'))
		return { id, cancel: () => {} }
	}
	const w = worker
	let active = true
	function handler(e: MessageEvent) {
		if (!active || e.data.id !== id) return
		if ('ok' in e.data) {
			active = false
			w.removeEventListener('message', handler)
			if (!e.data.ok) onError(new Error(e.data.error))
			return
		}
		if (e.data.kind === 'search-hit') {
			onHit(e.data.hits)
			return
		}
		active = false
		w.removeEventListener('message', handler)
		onDone({ total: e.data.total, truncated: e.data.truncated })
	}
	w.addEventListener('message', handler)
	w.postMessage({ id, type: 'search-stream', ...args })
	return {
		id,
		cancel: () => {
			if (!active) return
			active = false
			w.removeEventListener('message', handler)
		},
	}
}

export function isStale(id: number): boolean {
	return workerSeq !== id
}

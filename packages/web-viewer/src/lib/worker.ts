export type WorkerRequestBody =
	| { type: 'rexc-to-json'; rexc: string; refs: Record<string, unknown> }
	| { type: 'json-to-rexc'; json: string; refs: Record<string, unknown> }
	| { type: 'rexc-compact-size'; rexc: string; refs: Record<string, unknown> }
	| { type: 'search-stream'; rexc: string; refs: Record<string, unknown>; query: string; limit?: number }

export type WorkerRequest = WorkerRequestBody & { id: number }

export type WorkerResponse = {
	id: number
} & (
	| { ok: true; result: string; compactSize?: number }
	| { ok: false; error: string }
	| { kind: 'search-hit'; hits: SearchHit[] }
	| { kind: 'search-done'; total: number; truncated: boolean }
)

export type SearchHit = {
	path: string
	segments: Array<string | number>
	preview: string
}

const worker = new Worker(new URL('../decode-worker.ts', import.meta.url), { type: 'module' })
let workerSeq = 0

export type WorkerResult = { result: string; compactSize?: number }

export function workerCall(req: WorkerRequestBody): { id: number; promise: Promise<WorkerResult> } {
	const id = ++workerSeq
	const promise = new Promise<WorkerResult>((resolve, reject) => {
		function handler(e: MessageEvent<WorkerResponse>) {
			if (e.data.id !== id) return
			if (!('ok' in e.data)) return
			worker.removeEventListener('message', handler)
			if (e.data.ok) resolve({ result: e.data.result, compactSize: e.data.compactSize })
			else reject(new Error(e.data.error))
		}
		worker.addEventListener('message', handler)
	})
	worker.postMessage(Object.assign({ id }, req))
	return { id, promise }
}

export function workerSearchStream(
	args: { rexc: string; refs: Record<string, unknown>; query: string; limit?: number },
	onHit: (hits: SearchHit[]) => void,
	onDone: (info: { total: number; truncated: boolean }) => void,
	onError: (error: Error) => void,
): { id: number; cancel: () => void } {
	const id = ++workerSeq
	let active = true
	function handler(e: MessageEvent<WorkerResponse>) {
		if (!active || e.data.id !== id) return
		if ('ok' in e.data) {
			active = false
			worker.removeEventListener('message', handler)
			if (!e.data.ok) onError(new Error(e.data.error))
			return
		}
		if (e.data.kind === 'search-hit') {
			onHit(e.data.hits)
			return
		}
		active = false
		worker.removeEventListener('message', handler)
		onDone({ total: e.data.total, truncated: e.data.truncated })
	}
	worker.addEventListener('message', handler)
	worker.postMessage({ id, type: 'search-stream', ...args } satisfies WorkerRequest)
	return {
		id,
		cancel: () => {
			if (!active) return
			active = false
			worker.removeEventListener('message', handler)
		},
	}
}

/** Returns true if a newer request has superseded this one. */
export function isStale(id: number): boolean {
	return workerSeq !== id
}

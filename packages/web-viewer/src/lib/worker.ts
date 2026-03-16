export type WorkerRequestBody =
	| { type: 'rexc-to-json'; rexc: string; refs: Record<string, unknown> }
	| { type: 'json-to-rexc'; json: string; refs: Record<string, unknown> }
	| { type: 'rexc-compact-size'; rexc: string; refs: Record<string, unknown> }

export type WorkerRequest = WorkerRequestBody & { id: number }

export type WorkerResponse = {
	id: number
} & (
	| { ok: true; result: string; compactSize?: number }
	| { ok: false; error: string }
)

const worker = new Worker(new URL('../decode-worker.ts', import.meta.url), { type: 'module' })
let workerSeq = 0

export type WorkerResult = { result: string; compactSize?: number }

export function workerCall(req: WorkerRequestBody): { id: number; promise: Promise<WorkerResult> } {
	const id = ++workerSeq
	const promise = new Promise<WorkerResult>((resolve, reject) => {
		function handler(e: MessageEvent<WorkerResponse>) {
			if (e.data.id !== id) return
			worker.removeEventListener('message', handler)
			if (e.data.ok) resolve({ result: e.data.result, compactSize: e.data.compactSize })
			else reject(new Error(e.data.error))
		}
		worker.addEventListener('message', handler)
	})
	worker.postMessage(Object.assign({ id }, req))
	return { id, promise }
}

/** Returns true if a newer request has superseded this one. */
export function isStale(id: number): boolean {
	return workerSeq !== id
}

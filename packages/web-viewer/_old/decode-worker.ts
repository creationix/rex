import { parse, stringify } from "../../rex-lang/rexc.ts"

export type WorkerRequestBody =
  | { type: 'rexc-to-json'; rexc: string; refs: Record<string, unknown> }
  | { type: 'json-to-rexc'; json: string; refs: Record<string, unknown> }

export type WorkerRequest = WorkerRequestBody & { id: number }

export type WorkerResponse = {
  id: number
} & (
  | { ok: true; result: string }
  | { ok: false; error: string }
)

self.onmessage = (e: MessageEvent<WorkerRequest>) => {
  const msg = e.data
  try {
    let result: string
    if (msg.type === 'rexc-to-json') {
      result = JSON.stringify(parse(msg.rexc, { refs: msg.refs }), null, 2)
    } else {
      result = stringify(JSON.parse(msg.json), { refs: msg.refs }) ?? ''
    }
    self.postMessage({ id: msg.id, ok: true, result } satisfies WorkerResponse)
  } catch (err: any) {
    self.postMessage({ id: msg.id, ok: false, error: err.message } satisfies WorkerResponse)
  }
}

import { parse, stringify } from "@creationix/rx"

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

self.onmessage = (e: MessageEvent<WorkerRequest>) => {
  const msg = e.data
  try {
    let result: string
    let compactSize: number | undefined
    if (msg.type === 'rexc-to-json') {
      const parsed = parse(msg.rexc, { refs: msg.refs })
      const compact = JSON.stringify(parsed)
      compactSize = compact.length
      result = JSON.stringify(parsed, null, 2)
    } else if (msg.type === 'json-to-rexc') {
      const parsed = JSON.parse(msg.json)
      compactSize = JSON.stringify(parsed).length
      result = stringify(parsed, { refs: msg.refs }) ?? ''
    } else {
      // rexc-compact-size — just compute the compact JSON size without pretty-printing
      const parsed = parse(msg.rexc, { refs: msg.refs })
      compactSize = JSON.stringify(parsed).length
      result = ''
    }
    self.postMessage({ id: msg.id, ok: true, result, compactSize } satisfies WorkerResponse)
  } catch (err: any) {
    self.postMessage({ id: msg.id, ok: false, error: err.message } satisfies WorkerResponse)
  }
}

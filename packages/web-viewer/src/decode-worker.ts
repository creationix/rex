import { parse, stringify } from "@creationix/rx"

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
  | { kind: 'search-hit'; hits: Array<{ path: string; segments: Array<string | number>; preview: string }> }
  | { kind: 'search-done'; total: number; truncated: boolean }
)

function previewFor(value: unknown): string {
  if (value === null) return 'null'
  if (value === undefined) return 'undefined'
  if (typeof value === 'string') return value.length > 120 ? value.slice(0, 117) + '...' : value
  if (typeof value === 'number' || typeof value === 'boolean') return String(value)
  return Array.isArray(value) ? '[array]' : '{object}'
}

function runSearch(msg: Extract<WorkerRequest, { type: 'search-stream' }>) {
  const parsed = parse(msg.rexc, { refs: msg.refs })
  const q = msg.query.trim().toLowerCase()
  if (!q) {
    self.postMessage({ id: msg.id, kind: 'search-done', total: 0, truncated: false } satisfies WorkerResponse)
    return
  }

  const limit = Math.max(1, msg.limit ?? 10000)
  const stack: Array<{ path: string; segments: Array<string | number>; value: unknown }> = [{ path: '$', segments: [], value: parsed }]
  const batch: Array<{ path: string; segments: Array<string | number>; preview: string }> = []
  let total = 0
  let truncated = false

  while (stack.length > 0) {
    const { path, segments, value } = stack.pop()!

    if (Array.isArray(value)) {
      for (let i = value.length - 1; i >= 0; i--) {
        stack.push({ path: `${path}[${i}]`, segments: [...segments, i], value: value[i] })
      }
      continue
    }

    if (value && typeof value === 'object') {
      const obj = value as Record<string, unknown>
      const entries = Object.entries(obj)
      for (let i = entries.length - 1; i >= 0; i--) {
        const [key, child] = entries[i]
        const childPath = `${path}.${key}`
        const childSegments = [...segments, key]
        if (key.toLowerCase().includes(q)) {
          total++
          if (batch.length < 128 && total <= limit) {
            batch.push({ path: childPath, segments: childSegments, preview: previewFor(child) })
          }
          if (total >= limit) {
            truncated = true
            break
          }
        }
        stack.push({ path: childPath, segments: childSegments, value: child })
      }
      if (truncated) break
      if (batch.length >= 128) {
        self.postMessage({ id: msg.id, kind: 'search-hit', hits: batch.splice(0, batch.length) } satisfies WorkerResponse)
      }
      continue
    }

    const text = String(value).toLowerCase()
    if (text.includes(q)) {
      total++
      if (batch.length < 128 && total <= limit) {
        batch.push({ path, segments, preview: previewFor(value) })
      }
      if (total >= limit) {
        truncated = true
        break
      }
    }

    if (batch.length >= 128) {
      self.postMessage({ id: msg.id, kind: 'search-hit', hits: batch.splice(0, batch.length) } satisfies WorkerResponse)
    }
  }

  if (batch.length > 0) {
    self.postMessage({ id: msg.id, kind: 'search-hit', hits: batch } satisfies WorkerResponse)
  }
  self.postMessage({ id: msg.id, kind: 'search-done', total, truncated } satisfies WorkerResponse)
}

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
    } else if (msg.type === 'rexc-compact-size') {
      // rexc-compact-size — just compute the compact JSON size without pretty-printing
      const parsed = parse(msg.rexc, { refs: msg.refs })
      compactSize = JSON.stringify(parsed).length
      result = ''
    } else {
      runSearch(msg)
      return
    }
    self.postMessage({ id: msg.id, ok: true, result, compactSize } satisfies WorkerResponse)
  } catch (err: any) {
    self.postMessage({ id: msg.id, ok: false, error: err.message } satisfies WorkerResponse)
  }
}

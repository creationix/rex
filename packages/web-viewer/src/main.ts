import { BUILTIN_REFS, get, getEntries, getEach, makeContext, resolve } from "../../rex-lang/rexc.ts"
import type { RxNode, RxObject, RxArray, RxChain, RxContext } from "../../rex-lang/rexc.ts"
import { EditorView, basicSetup, json as jsonLang, oneDark } from "./codemirror.ts"
import { RexcTreeView } from "./rexc-tree.ts"
import type { RexcNode, RexcParser } from "./rexc-parser.ts"
import type { WorkerRequestBody, WorkerResponse } from "./decode-worker.ts"

function rxNodeToRexcNode(node: RxNode, key?: string | number, context?: RxContext): RexcNode {
  switch (node.type) {
    case 'primitive': {
      const v = node.value
      if (typeof v === 'string') return { kind: 'string', start: node.left, end: node.right, key: key as string, value: v }
      if (typeof v === 'number') return { kind: 'number', start: node.left, end: node.right, key: key as string, value: v }
      if (typeof v === 'boolean') return { kind: 'boolean', start: node.left, end: node.right, key: key as string, value: v }
      if (v === null) return { kind: 'null', start: node.left, end: node.right, key: key as string }
      return { kind: 'undefined', start: node.left, end: node.right, key: key as string }
    }
    case 'pointer': {
      if (typeof node.target === 'string')
        return { kind: 'reference', start: node.left, end: node.right, key: key as string, refId: node.target }
      const result: RexcNode = { kind: 'pointer', start: node.left, end: node.right, key: key as string, targetOffset: node.target }
      if (context) {
        try {
          // Only resolve cheap targets — peek at the target node type first
          const target = get(context.data, node.target)
          if (target.type === 'primitive' || target.type === 'chain') {
            const resolved = resolve(context, target)
            if (typeof resolved === 'string') {
              result.resolvedValue = resolved
              result.resolvedKind = target.type === 'chain' ? 'chain' : 'string'
            } else if (typeof resolved === 'number') {
              result.resolvedValue = String(resolved)
              result.resolvedKind = 'number'
            } else if (typeof resolved === 'boolean' || resolved === null) {
              result.resolvedValue = String(resolved)
              result.resolvedKind = String(typeof resolved === 'boolean' ? 'boolean' : 'null')
            }
          }
        } catch { }
      }
      return result
    }
    case 'object':
      return { kind: 'object', start: node.left, end: node.right, key: key as string, offset: node.content }
    case 'array':
      return { kind: 'array', start: node.left, end: node.right, key: key as string, offset: node.content }
    case 'chain': {
      let resolvedValue: string | undefined
      if (context) {
        try {
          const resolved = resolve(context, node)
          if (typeof resolved === 'string') resolvedValue = resolved
        } catch { }
      }
      return { kind: 'pathChain', start: node.left, end: node.right, key: key as string, offset: node.content, resolvedValue }
    }
  }
}

const realParser: RexcParser = {
  parseRoot(input) {
    const context = makeContext(input)
    return rxNodeToRexcNode(get(input, input.length), undefined, context)
  },
  parseChildren(input, parent): RexcNode[] {
    const context = makeContext(input)
    if (parent.kind === 'object') {
      const node = get(input, parent.end) as RxObject
      return [...getEntries(context, node)].reverse().map(([key, child]) => rxNodeToRexcNode(child, key, context))
    }
    if (parent.kind === 'array') {
      const node = get(input, parent.end) as RxArray
      return [...getEach(context, node)].reverse().map((child, i) => rxNodeToRexcNode(child, i, context))
    }
    if (parent.kind === 'pathChain') {
      const node = get(input, parent.end) as RxChain
      return [...getEach(context, node)].reverse().map((child, i) => rxNodeToRexcNode(child, i, context))
    }
    if (parent.kind === 'pointer') {
      const target = parent.targetOffset
      if (target < input.length) return [rxNodeToRexcNode(get(input, target), undefined, context)]
    }
    return []
  }
}

// --- DOM elements ---
const rexcContainer = document.getElementById("rexc")!
const jsonWrap = document.getElementById("json-editor")!
const statusEl = document.getElementById("status")!
const rexcSizeEl = document.getElementById("rexc-size")!
const jsonSizeEl = document.getElementById("json-size")!

const rexcSpinner = document.getElementById("rexc-spinner")!
const jsonSpinner = document.getElementById("json-spinner")!

let rexcText = ''
let updating = false

// --- Worker for heavy encode/decode ---
const worker = new Worker('./dist/decode-worker.js', { type: 'module' })
let workerSeq = 0

function workerCall(req: WorkerRequestBody): { id: number; promise: Promise<string> } {
  const id = ++workerSeq
  const promise = new Promise<string>((resolve, reject) => {
    function handler(e: MessageEvent<WorkerResponse>) {
      if (e.data.id !== id) return
      worker.removeEventListener('message', handler)
      if (e.data.ok) resolve(e.data.result)
      else reject(new Error(e.data.error))
    }
    worker.addEventListener('message', handler)
  })
  worker.postMessage(Object.assign({ id }, req))
  return { id, promise }
}

// --- REXC tree view ---
const treeView = new RexcTreeView(rexcContainer, realParser)

function setRexc(text: string) {
  rexcText = text
  treeView.setValue(text)
}

treeView.onPaste = (text) => {
  if (updating) return
  clearHash()
  updating = true
  rexcText = text

  // Phase 1: show the collapsed root node immediately so the UI isn't blank
  treeView.setValue(text, false)
  setStatus(true, "Decoding…")
  markNeutral(rexcContainer)
  markNeutral(jsonWrap)
  updateSizes()

  const value = text.trim()
  if (!value) { setJson(""); setStatus(true, "OK"); markNeutral(rexcContainer); markNeutral(jsonWrap); save(); updating = false; return }

  // Phase 2: expand the tree after the browser paints the root
  requestAnimationFrame(() => treeView.expandAndRestore())

  // Phase 3: full decode runs off-thread in a worker
  jsonSpinner.classList.add('active')
  const { id, promise } = workerCall({ type: 'rexc-to-json', rexc: value, refs: getActiveRefs() })
  promise.then(json => {
    if (workerSeq !== id) return // a newer request superseded this one
    setJson(json)
    setStatus(true, "OK")
    markValid(rexcContainer)
    markValid(jsonWrap)
    save()
  }).catch((e: any) => {
    if (workerSeq !== id) return
    setStatus(false, "REXC: " + e.message)
    treeView.setError(e.message)
    markError(rexcContainer)
    markNeutral(jsonWrap)
  }).finally(() => {
    if (workerSeq !== id) return
    jsonSpinner.classList.remove('active')
    updateSizes()
    updating = false
  })
}

// --- CodeMirror 6 JSON editor ---
const jsonEditor = new EditorView({
  parent: jsonWrap,
  extensions: [
    basicSetup,
    jsonLang(),
    oneDark,
    EditorView.lineWrapping,
    EditorView.updateListener.of(update => {
      if (update.docChanged && !updating) onJsonInput()
    }),
  ],
})

function getJson() { return jsonEditor.state.doc.toString() }
function setJson(text: string) {
  jsonEditor.dispatch({ changes: { from: 0, to: jsonEditor.state.doc.length, insert: text } })
}

// --- Refs modal ---
const refsBtn = document.getElementById("refs-btn")!
const refsToggle = document.getElementById("opt-refsEnabled") as HTMLInputElement
const refsModal = document.getElementById("refs-modal")!
const refsEditor = document.getElementById("refs-editor") as HTMLTextAreaElement
const refsStatusEl = document.getElementById("refs-status")!
const refsClose = document.getElementById("refs-close")!
const refsApply = document.getElementById("refs-apply")!
let currentRefs: Record<string, unknown> = {}

function getActiveRefs() { return refsToggle.checked ? currentRefs : {}; }
refsToggle.addEventListener("change", () => { reencodeFromJson(); save(); });

function parseRefs(text: string) {
  const trimmed = text.trim()
  if (!trimmed) return {}
  const val = JSON.parse(trimmed)
  if (typeof val !== "object" || val === null || Array.isArray(val)) throw new Error("Must be a JSON object")
  for (const k of Object.keys(val)) {
    if (typeof k !== "string") throw new Error("All keys must be strings")
    if (!/^[A-Za-z0-9_-]*$/.test(k)) throw new Error(`Invalid b64 key: ${k}`)
    if (k in BUILTIN_REFS) throw new Error(`Key conflicts with built-in ref: ${k}`)
  }
  return val
}

function updateRefsBtn() {
  const n = Object.keys(currentRefs).length
  refsBtn.textContent = n ? `refs {${n}}` : "refs {}"
}

refsBtn.addEventListener("click", () => {
  refsEditor.value = Object.keys(currentRefs).length ? JSON.stringify(currentRefs, null, 2) : "{}"
  refsStatusEl.textContent = "OK"
  refsStatusEl.className = "refs-status"
  refsModal.classList.add("open")
  refsEditor.focus()
})

refsEditor.addEventListener("input", () => {
  try {
    parseRefs(refsEditor.value)
    refsStatusEl.textContent = "OK"
    refsStatusEl.className = "refs-status"
  } catch (e: any) {
    refsStatusEl.textContent = e.message
    refsStatusEl.className = "refs-status refs-error"
  }
})

function closeRefsModal() { refsModal.classList.remove("open") }
refsClose.addEventListener("click", closeRefsModal)
refsModal.addEventListener("click", (e) => { if (e.target === refsModal) closeRefsModal() })

refsApply.addEventListener("click", () => {
  try {
    currentRefs = parseRefs(refsEditor.value)
    updateRefsBtn()
    closeRefsModal()
    reencodeFromJson()
    save()
  } catch (e: any) {
    refsStatusEl.textContent = e.message
    refsStatusEl.className = "refs-status refs-error"
  }
})

// --- Size display ---
const textEncoder = new TextEncoder()
function humanSize(bytes: number) {
  if (bytes < 1024) return bytes + " B"
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KiB"
  return (bytes / (1024 * 1024)).toFixed(2) + " MiB"
}
function compactJsonSize(text: string) {
  try {
    return textEncoder.encode(JSON.stringify(JSON.parse(text.trim()))).length
  } catch { return null }
}
function sizeLabel(raw: number, compact: number | null) {
  return compact !== null && compact !== raw
    ? humanSize(raw) + " (" + humanSize(compact) + " compact)"
    : humanSize(raw)
}
function updateSizes() {
  const rexcRaw = textEncoder.encode(rexcText).length
  rexcSizeEl.textContent = sizeLabel(rexcRaw, rexcRaw)
  const jsonText = getJson()
  const jsonRaw = textEncoder.encode(jsonText).length
  jsonSizeEl.textContent = sizeLabel(jsonRaw, compactJsonSize(jsonText))
}

// --- Persistence ---
function save() {
  // const opts = Object.fromEntries(allBoolNames.map(n => [n, optEls[n]!.checked]))
  // for (const n of encodeNumNames) (opts as any)[n] = parseInt(optEls[n]!.value, 10) || 0
  // localStorage.setItem("rexc-viewer", JSON.stringify({ rexc: rexcText, json: getJson(), opts, refs: currentRefs, refsEnabled: refsToggle.checked }))
}
function restore() {
  // try {
  //   const s = JSON.parse(localStorage.getItem("rexc-viewer")!)
  //   if (s) {
  //     rexcText = s.rexc || ""
  //     treeView.setValue(rexcText)
  //     updating = true; setJson(s.json || ""); updating = false
  //     if (s.opts) {
  //       for (const n of allBoolNames) if (n in s.opts) optEls[n]!.checked = s.opts[n]
  //       for (const n of encodeNumNames) if (n in s.opts) optEls[n]!.value = s.opts[n]
  //     }
  //     if (s.refs && typeof s.refs === "object") { currentRefs = s.refs; updateRefsBtn() }
  //     if ("refsEnabled" in s) refsToggle.checked = s.refsEnabled
  //     updateSizes()
  //   }
  // } catch { }
}
restore()

// --- URL hash loading ---
let loadedFromHash = false
if (location.hash.length > 1) {
  const hash = location.hash.slice(1)
  const eq = hash.indexOf("=")
  if (eq !== -1) {
    const key = hash.slice(0, eq)
    const val = decodeURIComponent(hash.slice(eq + 1))
    if (key === "rexc") { setRexc(val); updating = true; setJson(""); updating = false; loadedFromHash = true }
    else if (key === "json") { updating = true; setJson(val); updating = false; setRexc(""); loadedFromHash = true }
    if (loadedFromHash) updateSizes()
  }
}

function clearHash() {
  if (loadedFromHash) {
    loadedFromHash = false
    history.replaceState(null, "", location.pathname + location.search)
  }
}

function copyShareUrl(key: string, value: string) {
  const hash = "#" + key + "=" + value.replace(/%/g, "%25").replace(/#/g, "%23").replace(/\n/g, "%0A").replace(/\r/g, "%0D")
  history.replaceState(null, "", location.pathname + location.search + hash)
  loadedFromHash = true
  navigator.clipboard.writeText(location.href).then(() => {
    setStatus(true, "URL copied!")
    setTimeout(() => setStatus(true, "OK"), 1500)
  })
}

document.getElementById("copy-rexc")!.addEventListener("click", () => copyShareUrl("rexc", rexcText))
document.getElementById("copy-json")!.addEventListener("click", () => copyShareUrl("json", getJson()))

// --- Status + validation ---
function setStatus(ok: boolean, msg: string) {
  statusEl.textContent = msg
  statusEl.className = ok ? "ok" : "error"
}

function markValid(el: Element) { el.classList.remove("error"); el.classList.add("valid") }
function markError(el: Element) { el.classList.remove("valid"); el.classList.add("error") }
function markNeutral(el: Element) { el.classList.remove("valid", "error") }

// --- JSON → REXC (via worker) ---
function reencodeFromJson() {
  const value = getJson().trim()
  if (!value) return
  setStatus(true, "Encoding…")
  rexcSpinner.classList.add('active')
  updating = true
  const { id, promise } = workerCall({ type: 'json-to-rexc', json: value, refs: getActiveRefs() })
  promise.then(rexc => {
    if (workerSeq !== id) return
    rexcText = rexc
    treeView.setValue(rexcText)
    setStatus(true, "OK")
    markValid(rexcContainer)
    markValid(jsonWrap)
    save()
  }).catch(() => {
    if (workerSeq !== id) return
  }).finally(() => {
    if (workerSeq !== id) return
    rexcSpinner.classList.remove('active')
    updateSizes()
    updating = false
  })
}

// --- JSON input handler (debounced, via worker) ---
let jsonDebounce: ReturnType<typeof setTimeout> | undefined
function onJsonInput() {
  if (updating) return
  clearHash()
  clearTimeout(jsonDebounce)
  jsonDebounce = setTimeout(() => {
    if (updating) return
    updating = true
    const value = getJson().trim()
    if (!value) { setRexc(""); setStatus(true, "OK"); markNeutral(rexcContainer); markNeutral(jsonWrap); save(); updating = false; return }
    setStatus(true, "Encoding…")
    rexcSpinner.classList.add('active')
    const { id, promise } = workerCall({ type: 'json-to-rexc', json: value, refs: getActiveRefs() })
    promise.then(rexc => {
      if (workerSeq !== id) return
      rexcText = rexc
      treeView.setValue(rexcText)
      setStatus(true, "OK")
      markValid(rexcContainer)
      markValid(jsonWrap)
      save()
    }).catch((e: any) => {
      if (workerSeq !== id) return
      setStatus(false, "JSON: " + e.message)
      markError(jsonWrap)
      markNeutral(rexcContainer)
    }).finally(() => {
      if (workerSeq !== id) return
      rexcSpinner.classList.remove('active')
      updateSizes()
      updating = false
    })
  }, 300)
}

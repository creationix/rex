import { stringify, parse, BUILTIN_REFS, get, getEntries, getEach, makeContext } from "../../rex-lang/rexc.ts"
import type { RxNode, RxObject, RxArray, RxChain } from "../../rex-lang/rexc.ts"
import { EditorView, basicSetup, json as jsonLang, oneDark } from "./codemirror.ts"
import { RexcTreeView } from "./rexc-tree.ts"
import type { RexcNode, RexcParser } from "./rexc-parser.ts"

function rxNodeToRexcNode(node: RxNode, key?: string | number): RexcNode {
  switch (node.type) {
    case 'primitive': {
      const v = node.value
      if (typeof v === 'string') return { kind: 'string', start: node.left, end: node.right, key: key as string, value: v }
      if (typeof v === 'number') return { kind: 'number', start: node.left, end: node.right, key: key as string, value: v }
      if (typeof v === 'boolean') return { kind: 'boolean', start: node.left, end: node.right, key: key as string, value: v }
      if (v === null) return { kind: 'null', start: node.left, end: node.right, key: key as string }
      return { kind: 'undefined', start: node.left, end: node.right, key: key as string }
    }
    case 'pointer':
      if (typeof node.target === 'string')
        return { kind: 'reference', start: node.left, end: node.right, key: key as string, refId: node.target }
      return { kind: 'pointer', start: node.left, end: node.right, key: key as string, targetOffset: node.target }
    case 'object':
      return { kind: 'object', start: node.left, end: node.right, key: key as string, offset: node.content }
    case 'array':
      return { kind: 'array', start: node.left, end: node.right, key: key as string, offset: node.content }
    case 'chain':
      return { kind: 'pathChain', start: node.left, end: node.right, key: key as string, offset: node.content }
  }
}

const realParser: RexcParser = {
  parseRoot(input) {
    return rxNodeToRexcNode(get(input, input.length))
  },
  parseChildren(input, parent): RexcNode[] {
    const context = makeContext(input)
    if (parent.kind === 'object') {
      const node = get(input, parent.end) as RxObject
      return [...getEntries(context, node)].reverse().map(([key, child]) => rxNodeToRexcNode(child, key))
    }
    if (parent.kind === 'array') {
      const node = get(input, parent.end) as RxArray
      return [...getEach(context, node)].reverse().map((child, i) => rxNodeToRexcNode(child, i))
    }
    if (parent.kind === 'pathChain') {
      const node = get(input, parent.end) as RxChain
      return [...getEach(context, node)].reverse().map((child, i) => rxNodeToRexcNode(child, i))
    }
    if (parent.kind === 'pointer') {
      const target = parent.targetOffset
      if (target < input.length) return [rxNodeToRexcNode(get(input, target))]
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

let rexcText = ''
let updating = false

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
  treeView.setValue(text)
  try {
    const value = text.trim()
    if (!value) { setJson(""); setStatus(true, "OK"); markNeutral(rexcContainer); markNeutral(jsonWrap); save(); updating = false; return }
    const decoded = parse(value, getDecodeOpts())
    setJson(JSON.stringify(decoded, null, 2))
    setStatus(true, "OK")
    markValid(rexcContainer)
    markValid(jsonWrap)
    save()
  } catch (e: any) {
    setStatus(false, "REXC: " + e.message)
    treeView.setError(e.message)
    markError(rexcContainer)
    markNeutral(jsonWrap)
  }
  updateSizes()
  updating = false
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

// --- Options ---
const encodeNumNames = ["indexes"]
const decodeOptNames = ["lazy"]
const sharedOptNames = ["reverse"]
const allBoolNames = [...decodeOptNames, ...sharedOptNames]
const allOptNames = [...allBoolNames, ...encodeNumNames]
const optEls = Object.fromEntries(allOptNames.map(n => [n, document.getElementById("opt-" + n)])) as Record<string, HTMLInputElement>

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

// --- Encode/decode opts ---
function getEncodeOpts() {
  const o: any = Object.fromEntries(sharedOptNames.map(n => [n, optEls[n]!.checked]))
  for (const n of encodeNumNames) o[n] = parseInt(optEls[n]!.value, 10) || 0
  o.refs = getActiveRefs()
  return o
}
function getDecodeOpts() {
  const o: any = Object.fromEntries([...decodeOptNames, ...sharedOptNames].map(n => [n, optEls[n]!.checked]))
  o.refs = getActiveRefs()
  return o
}

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

// --- JSON → REXC ---
function reencodeFromJson() {
  try {
    const value = getJson().trim()
    if (!value) return
    const parsed = JSON.parse(value)
    updating = true
    rexcText = stringify(parsed, getEncodeOpts()) ?? ''
    treeView.setValue(rexcText)
    updating = false
    updateSizes()
    save()
  } catch { }
}
for (const el of Object.values(optEls)) {
  el.addEventListener("change", reencodeFromJson)
  el.addEventListener("input", reencodeFromJson)
}

// --- JSON input handler ---
function onJsonInput() {
  if (updating) return
  clearHash()
  updating = true
  try {
    const value = getJson().trim()
    if (!value) { setRexc(""); setStatus(true, "OK"); markNeutral(rexcContainer); markNeutral(jsonWrap); save(); updating = false; return }
    const parsed = JSON.parse(value)
    rexcText = stringify(parsed, getEncodeOpts()) ?? ''
    treeView.setValue(rexcText)
    setStatus(true, "OK")
    markValid(rexcContainer)
    markValid(jsonWrap)
    save()
  } catch (e: any) {
    setStatus(false, "JSON: " + e.message)
    markError(jsonWrap)
    markNeutral(rexcContainer)
  }
  updateSizes()
  updating = false
}

import { stringify, parse, BUILTIN_REFS, fromZigZag } from "../../rex-lang/rexc.ts"
import { EditorView, basicSetup, json as jsonLang, oneDark } from "./codemirror.ts"
import { RexcTreeView } from "./rexc-tree.ts"
import type { RexcNode, RexcParser } from "./rexc-parser.ts"

const textDecoder = new TextDecoder()

function skipB64(input: Uint8Array, offset: number): number {
  while (offset < input.length) {
    const c = input[offset]!
    if ((c >= 0x41 && c <= 0x5A) || (c >= 0x61 && c <= 0x7A) || (c >= 0x30 && c <= 0x39) || c === 0x2D || c === 0x5F) {
      offset++
    } else {
      break
    }
  }
  return offset
}

// Map a b64 ascii code to its 6-bit value, or -1 if it's not a valid b64 char
// Order: 0-9 a-z A-Z - _
function b64Value(input: number): number {
  if (input >= 0x30 && input <= 0x39) return input - 0x30       // '0'-'9' → 0-9
  if (input >= 0x61 && input <= 0x7A) return input - 0x61 + 10  // 'a'-'z' → 10-35
  if (input >= 0x41 && input <= 0x5A) return input - 0x41 + 36  // 'A'-'Z' → 36-61
  if (input === 0x2D) return 62                                  // '-' → 62
  if (input === 0x5F) return 63                                  // '_' → 63
  return -1
}

// start offset is inclusive, end offset is exclusive
function parseB64(input: Uint8Array, start: number, end: number): number {
  let val = 0
  for (let i = start; i < end; i++) {
    val = val * 64 + b64Value(input[i]!)
  }
  return val
}

let parseCache: Record<number, RexcNode> = {}

// --- Stub parser (replace with real implementation) ---
const stubParser: RexcParser = {
  parseRoot(input) {
    parseCache = {}
    const root = parseAny(input, 0, input.length)
    root.end ??= input.length
    return root
  },
  parseChildren(input, parent): RexcNode[] {
    let offset = parent.offset
    const children: RexcNode[] = []
    while (offset < parent.end) {
      let key: string | undefined
      if (parent.kind === "object") {
        const keyNode = parseAny(input, offset, parent.end)
        console.log({ keyNode })
        if (keyNode.kind !== "string") {
          throw new SyntaxError("Expected string key in object at offset " + offset)
        }
        key = keyNode.value
        offset = keyNode.end ?? offset
      }
      console.log({ key })
      const valueNode = parseAny(input, offset, parent.end)
      valueNode.key = key
      children.push(valueNode)
      offset = valueNode.end
    }
    return children
  }
}

function parseAny(input: Uint8Array, start: number, end: number): RexcNode {
  const cached = parseCache[start]
  if (cached) return cached
  const value = parseAnyInner(input, start, end)
  console.log({ start, end, value })
  parseCache[start] = value
  return value
}

function parseAnyInner(input: Uint8Array, offset: number, end: number): RexcNode {
  const start = offset
  const tagOffset = skipB64(input, offset)
  if (tagOffset >= end) {
    throw new SyntaxError("Unexpected end of input while parsing value")
  }
  const tag = input[tagOffset]
  offset = tagOffset + 1
  switch (tag) {
    // ":" object
    case 0x3A: {
      const end = offset + parseB64(input, start, tagOffset)
      return { kind: 'object', start, offset, end }
    }
    // ";" array
    case 0x3B: {
      const end = offset + parseB64(input, start, tagOffset)
      return { kind: 'array', start, offset, end }
    }
    // "." bare string
    case 0x2E: {
      return {
        kind: 'string',
        start, end: offset,
        value: textDecoder.decode(input.subarray(start, tagOffset))
      }
    }
    // "," string
    case 0x2C: {
      const length = parseB64(input, start, tagOffset)
      return {
        kind: 'string',
        start: start, end: offset + length,
        value: textDecoder.decode(input.subarray(offset, offset + length))
      }
    }
    // "+" zigzag integer
    case 0x2B: {
      return {
        kind: 'number',
        start, end: offset,
        value: fromZigZag(parseB64(input, start, tagOffset))
      }
    }
    // "/" Path Chain
    case 0x2F: {
      const end = offset + parseB64(input, start, tagOffset)
      return {
        kind: 'pathChain',
        start, offset, end,
      }
    }
    // "^" pointer
    case 0x5E:
      return {
        kind: 'pointer',
        start, end: offset,
        targetOffset: offset + parseB64(input, start, tagOffset),
      }
    default:
      console.warn("Unknown tag " + String.fromCharCode(tag) + " at offset " + offset)
      return {
        kind: 'error',
        start, end: offset,
        value: JSON.stringify(textDecoder.decode(input.subarray(start, end)))
      }
      throw new Error("TODO: implement real parser instead of stub")
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
const treeView = new RexcTreeView(rexcContainer, stubParser)

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
const encodeBoolNames = ["bareStrings", "pointers", "schemas", "pathChains"]
const encodeNumNames = ["indexes"]
const decodeOptNames = ["lazy"]
const sharedOptNames = ["reverse"]
const allBoolNames = [...encodeBoolNames, ...decodeOptNames, ...sharedOptNames]
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
  const o: any = Object.fromEntries([...encodeBoolNames, ...sharedOptNames].map(n => [n, optEls[n]!.checked]))
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
  const opts = Object.fromEntries(allBoolNames.map(n => [n, optEls[n]!.checked]))
  for (const n of encodeNumNames) (opts as any)[n] = parseInt(optEls[n]!.value, 10) || 0
  localStorage.setItem("rexc-viewer", JSON.stringify({ rexc: rexcText, json: getJson(), opts, refs: currentRefs, refsEnabled: refsToggle.checked }))
}
function restore() {
  try {
    const s = JSON.parse(localStorage.getItem("rexc-viewer")!)
    if (s) {
      rexcText = s.rexc || ""
      treeView.setValue(rexcText)
      updating = true; setJson(s.json || ""); updating = false
      if (s.opts) {
        for (const n of allBoolNames) if (n in s.opts) optEls[n]!.checked = s.opts[n]
        for (const n of encodeNumNames) if (n in s.opts) optEls[n]!.value = s.opts[n]
      }
      if (s.refs && typeof s.refs === "object") { currentRefs = s.refs; updateRefsBtn() }
      if ("refsEnabled" in s) refsToggle.checked = s.refsEnabled
      updateSizes()
    }
  } catch { }
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

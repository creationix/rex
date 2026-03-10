import { stringify, parse, BUILTIN_REFS } from "../../rex-lang/rexc.ts"
import { EditorView, basicSetup, json as jsonLang, oneDark } from "./codemirror.ts"

const rexcEl = document.getElementById("rexc") as HTMLTextAreaElement
const jsonWrap = document.getElementById("json-editor")!
const statusEl = document.getElementById("status")!
const rexcSizeEl = document.getElementById("rexc-size")!
const jsonSizeEl = document.getElementById("json-size")!

let updating = false

// CodeMirror 6 JSON editor
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

const encodeBoolNames = ["bareStrings", "randomAccess", "pointers", "schemas", "pathChains"]
const encodeNumNames = ["indexes"]
const decodeOptNames = ["lazy"]
const sharedOptNames = ["reverse"]
const allBoolNames = [...encodeBoolNames, ...decodeOptNames, ...sharedOptNames]
const allOptNames = [...allBoolNames, ...encodeNumNames]
const optEls = Object.fromEntries(allOptNames.map(n => [n, document.getElementById("opt-" + n)])) as Record<string, HTMLInputElement>

// Refs modal
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

function getEncodeOpts() {
  const o: any = Object.fromEntries([...encodeBoolNames, ...sharedOptNames].map(n => [n, optEls[n].checked]))
  for (const n of encodeNumNames) o[n] = parseInt(optEls[n].value, 10) || 0
  o.refs = getActiveRefs()
  return o
}
function getDecodeOpts() {
  const o: any = Object.fromEntries([...decodeOptNames, ...sharedOptNames].map(n => [n, optEls[n].checked]))
  o.refs = getActiveRefs()
  return o
}

const encoder = new TextEncoder()
function humanSize(bytes: number) {
  if (bytes < 1024) return bytes + " B"
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KiB"
  return (bytes / (1024 * 1024)).toFixed(2) + " MiB"
}
function compactJsonSize(text: string) {
  try {
    const values = splitValues(text.trim(), JSON.parse)
    return encoder.encode(values.map(v => JSON.stringify(v)).join("\n")).length
  } catch { return null }
}
function sizeLabel(raw: number, compact: number | null) {
  return compact !== null && compact !== raw
    ? humanSize(raw) + " (" + humanSize(compact) + " compact)"
    : humanSize(raw)
}
function updateSizes() {
  const rexcRaw = encoder.encode(rexcEl.value).length
  rexcSizeEl.textContent = sizeLabel(rexcRaw, rexcRaw)
  const jsonText = getJson()
  const jsonRaw = encoder.encode(jsonText).length
  jsonSizeEl.textContent = sizeLabel(jsonRaw, compactJsonSize(jsonText))
}

function save() {
  const opts = Object.fromEntries(allBoolNames.map(n => [n, optEls[n].checked]))
  for (const n of encodeNumNames) (opts as any)[n] = parseInt(optEls[n].value, 10) || 0
  localStorage.setItem("rexc-viewer", JSON.stringify({ rexc: rexcEl.value, json: getJson(), opts, refs: currentRefs, refsEnabled: refsToggle.checked }))
}
function restore() {
  try {
    const s = JSON.parse(localStorage.getItem("rexc-viewer")!)
    if (s) {
      rexcEl.value = s.rexc || ""
      updating = true; setJson(s.json || ""); updating = false
      if (s.opts) {
        for (const n of allBoolNames) if (n in s.opts) optEls[n].checked = s.opts[n]
        for (const n of encodeNumNames) if (n in s.opts) optEls[n].value = s.opts[n]
      }
      if (s.refs && typeof s.refs === "object") { currentRefs = s.refs; updateRefsBtn() }
      if ("refsEnabled" in s) refsToggle.checked = s.refsEnabled
      updateSizes()
    }
  } catch { }
}
restore()

// Load from URL hash
let loadedFromHash = false
if (location.hash.length > 1) {
  const hash = location.hash.slice(1)
  const eq = hash.indexOf("=")
  if (eq !== -1) {
    const key = hash.slice(0, eq)
    const val = decodeURIComponent(hash.slice(eq + 1))
    if (key === "rexc") { rexcEl.value = val; updating = true; setJson(""); updating = false; loadedFromHash = true }
    else if (key === "json") { updating = true; setJson(val); updating = false; rexcEl.value = ""; loadedFromHash = true }
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

document.getElementById("copy-rexc")!.addEventListener("click", () => copyShareUrl("rexc", rexcEl.value))
document.getElementById("copy-json")!.addEventListener("click", () => copyShareUrl("json", getJson()))

function setStatus(ok: boolean, msg: string) {
  statusEl.textContent = msg
  statusEl.className = ok ? "ok" : "error"
}

function markValid(el: Element) { el.classList.remove("error"); el.classList.add("valid") }
function markError(el: Element) { el.classList.remove("valid"); el.classList.add("error") }
function markNeutral(el: Element) { el.classList.remove("valid", "error") }

// Split a multi-value document into individual values.
// Greedily accumulates lines until parseFn succeeds, then starts the next value.
function splitValues(text: string, parseFn: (s: string) => unknown) {
  try { return [parseFn(text)] } catch (e) {
    // If it doesn't split into multiple values, throw the original error
    if (!/\n\S/.test(text)) throw e
    var firstError = e as Error
  }
  const chunks = text.split(/\n(?=\S)/)
  const values: unknown[] = []
  let buf = ""
  for (const chunk of chunks) {
    buf += (buf ? "\n" : "") + chunk
    try {
      values.push(parseFn(buf))
      buf = ""
    } catch { }
  }
  if (buf.trim()) {
    if (values.length === 0) throw firstError
    throw new SyntaxError("Trailing unparseable content: " + firstError!.message)
  }
  return values
}

function reencodeFromJson() {
  try {
    const value = getJson().trim()
    if (!value) return
    const values = splitValues(value, JSON.parse)
    updating = true
    rexcEl.value = values.map(v => stringify(v, getEncodeOpts())).join("\n")
    updating = false
    updateSizes()
    save()
  } catch { }
}
for (const el of Object.values(optEls)) {
  el.addEventListener("change", reencodeFromJson)
  el.addEventListener("input", reencodeFromJson)
}

rexcEl.addEventListener("input", () => {
  if (updating) return
  clearHash()
  updating = true
  try {
    const value = rexcEl.value.trim()
    if (!value) { setJson(""); setStatus(true, "OK"); markNeutral(rexcEl); markNeutral(jsonWrap); save(); updating = false; return }
    const values = splitValues(value, v => parse(v, getDecodeOpts()))
    setJson(values.map(v => JSON.stringify(v, null, 2)).join("\n"))
    setStatus(true, "OK")
    markValid(rexcEl)
    markValid(jsonWrap)
    save()
  } catch (e: any) {
    setStatus(false, "REXC: " + e.message)
    markError(rexcEl)
    markNeutral(jsonWrap)
  }
  updateSizes()
  updating = false
})

function onJsonInput() {
  if (updating) return
  clearHash()
  updating = true
  try {
    const value = getJson().trim()
    if (!value) { rexcEl.value = ""; setStatus(true, "OK"); markNeutral(rexcEl); markNeutral(jsonWrap); save(); updating = false; return }
    const values = splitValues(value, JSON.parse)
    rexcEl.value = values.map(v => stringify(v, getEncodeOpts())).join("\n")
    setStatus(true, "OK")
    markValid(rexcEl)
    markValid(jsonWrap)
    save()
  } catch (e: any) {
    setStatus(false, "JSON: " + e.message)
    markError(jsonWrap)
    markNeutral(rexcEl)
  }
  updateSizes()
  updating = false
}

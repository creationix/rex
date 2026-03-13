import type { RexcNode, RexcParser } from './rexc-parser.ts'

interface TreeRow {
  node: RexcNode
  depth: number
  expanded: boolean
}

const ROW_HEIGHT = 24
const INDENT_PX = 16
const OVERSCAN = 4

const KIND_COLORS: Record<string, string> = {
  string: '#ce9178',
  bareString: '#ce9178',
  integer: '#b5cea8',
  decimal: '#b5cea8',
  boolean: '#569cd6',
  null: '#569cd6',
  undefined: '#569cd6',
  object: '#dcdcaa',
  array: '#dcdcaa',
  call: '#dcdcaa',
  reference: '#c586c0',
  pointer: '#c586c0',
  variable: '#9cdcfe',
  self: '#9cdcfe',
  opcode: '#4ec9b0',
  when: '#c586c0',
  unless: '#c586c0',
  alt: '#c586c0',
  all: '#c586c0',
  forIn: '#c586c0',
  forOf: '#c586c0',
  while: '#c586c0',
  set: '#f48771',
  swap: '#f48771',
  delete: '#f48771',
  arrayComp: '#dcdcaa',
  objectComp: '#dcdcaa',
  loopControl: '#c586c0',
}

const KIND_TAGS: Record<string, string> = {
  object: 'OBJ',
  array: 'ARR',
  pointer: 'PTR',
  pathChain: 'CHAIN',
  call: 'CALL',
  when: 'WHEN',
  unless: 'UNLESS',
  alt: 'ALT',
  all: 'ALL',
  forIn: 'FORIN',
  forOf: 'FOROF',
  while: 'WHILE',
  set: 'SET',
  swap: 'SWAP',
  delete: 'DEL',
  arrayComp: 'ARRC',
  objectComp: 'OBJC',
}

const textDecoder = new TextDecoder()

export class RexcTreeView {
  private viewport: HTMLElement
  private scrollContent: HTMLElement
  private content: HTMLElement
  private parser: RexcParser
  private input: Uint8Array = new Uint8Array(0)
  private inputStr: string = '' // kept for clipboard copy
  private rows: TreeRow[] = []
  private placeholder: HTMLElement
  private errorEl: HTMLElement
  private renderStart = -1
  private renderEnd = -1

  /** Called when the user pastes REXC text. */
  onPaste?: (text: string) => void

  constructor(container: HTMLElement, parser: RexcParser) {
    this.parser = parser

    container.style.position = 'relative'
    container.style.overflow = 'hidden'

    this.placeholder = document.createElement('div')
    this.placeholder.textContent = 'Paste REXC here...'
    this.placeholder.style.cssText = 'color:#555; padding:12px; font-size:13px; position:absolute; inset:0; pointer-events:none;'
    container.appendChild(this.placeholder)

    this.errorEl = document.createElement('div')
    this.errorEl.style.cssText = 'color:#f48771; padding:12px; font-size:13px; position:absolute; inset:0; display:none; overflow:auto;'
    container.appendChild(this.errorEl)

    this.viewport = document.createElement('div')
    this.viewport.style.cssText = 'position:absolute; inset:0; overflow:auto;'
    container.appendChild(this.viewport)

    this.scrollContent = document.createElement('div')
    this.scrollContent.style.cssText = 'position:relative; min-width:100%;'
    this.viewport.appendChild(this.scrollContent)

    this.content = document.createElement('div')
    this.content.style.cssText = 'position:absolute; top:0; left:0; right:0;'
    this.scrollContent.appendChild(this.content)

    this.viewport.addEventListener('scroll', () => this.render(), { passive: true })

    // Paste support
    container.setAttribute('tabindex', '0')
    container.style.outline = 'none'
    container.addEventListener('paste', (e) => {
      e.preventDefault()
      const text = (e as ClipboardEvent).clipboardData?.getData('text') || ''
      if (text) this.onPaste?.(text)
    })

    // Ctrl+C / Cmd+C copies full raw text
    container.addEventListener('keydown', (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'c' && this.inputStr) {
        e.preventDefault()
        navigator.clipboard.writeText(this.inputStr)
      }
    })

    // Click to toggle expand/collapse
    this.content.addEventListener('click', (e) => {
      const row = (e.target as HTMLElement).closest('[data-ri]') as HTMLElement
      if (!row) return
      const idx = parseInt(row.dataset.ri!, 10)
      if (!isNaN(idx) && idx < this.rows.length) this.toggleRow(idx)
    })

    // Re-render on resize
    new ResizeObserver(() => {
      this.renderStart = -1 // force re-render
      this.render()
    }).observe(container)
  }

  /** Collect the set of expanded paths (encoded as "key0\0key1\0...") */
  private getExpandedPaths(): Set<string> {
    const paths = new Set<string>()
    const stack: string[] = []
    for (const row of this.rows) {
      // Trim stack to current depth
      stack.length = row.depth
      stack.push(String(row.node.key ?? ''))
      if (row.expanded) {
        paths.add(stack.join('\0'))
      }
    }
    return paths
  }

  /** Re-expand rows matching the given path set */
  private restoreExpandedPaths(paths: Set<string>) {
    const stack: string[] = []
    let i = 0
    while (i < this.rows.length) {
      const row = this.rows[i]!
      stack.length = row.depth
      stack.push(String(row.node.key ?? ''))
      if (this.isExpandable(row.node) && !row.expanded && paths.has(stack.join('\0'))) {
        this.expandRow(i)
      }
      i++
    }
  }

  setValue(input: string, autoExpand = true) {
    const prevPaths = this.getExpandedPaths()
    const prevScroll = this.viewport.scrollTop

    this.inputStr = input
    this.placeholder.style.display = input ? 'none' : ''
    this.errorEl.style.display = 'none'

    if (!input) {
      this.input = new Uint8Array(0)
      this.rows = []
      this.renderStart = -1
      this.render()
      return
    }

    this.input = new TextEncoder().encode(input)

    try {
      const root = this.parser.parseRoot(this.input)
      this.rows = [{ node: root, depth: 0, expanded: false }]
      if (autoExpand) {
        // Auto-expand the root
        if (this.isExpandable(root)) {
          this.expandRow(0)
        }
        // Restore previously expanded paths
        if (prevPaths.size > 0) {
          this.restoreExpandedPaths(prevPaths)
        }
      }
    } catch (e: any) {
      this.rows = []
      this.errorEl.innerHTML = ''
      const msg = document.createElement('div')
      msg.textContent = 'Parse error: ' + e.message
      this.errorEl.appendChild(msg)
      const pre = document.createElement('pre')
      pre.style.cssText = 'color:#888; margin-top:8px; white-space:pre-wrap; word-break:break-all; font-size:12px;'
      pre.textContent = input.length > 2000 ? input.slice(0, 2000) + '…' : input
      this.errorEl.appendChild(pre)
      this.errorEl.style.display = ''
    }

    this.viewport.scrollTop = prevScroll
    this.renderStart = -1
    this.render()
  }

  /** Expand root and restore previously expanded paths. */
  expandAndRestore() {
    const root = this.rows[0]
    if (root && !root.expanded && this.isExpandable(root.node)) {
      const prevPaths = this.getExpandedPaths()
      this.expandRow(0)
      if (prevPaths.size > 0) {
        this.restoreExpandedPaths(prevPaths)
      }
      this.renderStart = -1
      this.render()
    }
  }

  setError(msg: string) {
    this.errorEl.innerHTML = ''
    if (msg) {
      const msgEl = document.createElement('div')
      msgEl.textContent = msg
      this.errorEl.appendChild(msgEl)
      if (this.inputStr) {
        const pre = document.createElement('pre')
        pre.style.cssText = 'color:#888; margin-top:8px; white-space:pre-wrap; word-break:break-all; font-size:12px;'
        const text = this.inputStr
        pre.textContent = text.length > 2000 ? text.slice(0, 2000) + '…' : text
        this.errorEl.appendChild(pre)
      }
    }
    this.errorEl.style.display = msg ? '' : 'none'
  }

  private appendTag(parent: HTMLElement, text: string, color: string) {
    const span = document.createElement('span')
    span.style.cssText = `color:${color};font-size:9px;font-weight:600;letter-spacing:0.5px;background:${color}18;border-radius:3px;padding:1px 4px;margin-left:6px;vertical-align:middle;`
    span.textContent = text
    parent.appendChild(span)
  }

  private isExpandable(node: RexcNode): boolean {
    return node.kind === 'object' || node.kind === 'array' || node.kind === "pathChain" || node.kind === "pointer"
  }

  private expandRow(idx: number) {
    const row = this.rows[idx]
    if (!this.isExpandable(row.node) || row.expanded) return
    const children = this.parser.parseChildren(this.input, row.node)
    const childRows: TreeRow[] = children.map(node => ({ node, depth: row.depth + 1, expanded: false }))
    this.rows.splice(idx + 1, 0, ...childRows)
    row.expanded = true
  }

  private toggleRow(idx: number) {
    const row = this.rows[idx]
    if (!this.isExpandable(row.node)) return

    if (row.expanded) {
      // Collapse: remove all descendants
      let end = idx + 1
      while (end < this.rows.length && this.rows[end].depth > row.depth) end++
      this.rows.splice(idx + 1, end - idx - 1)
      row.expanded = false
    } else {
      this.expandRow(idx)
    }

    this.renderStart = -1
    this.render()
  }

  private render() {
    const viewportH = this.viewport.clientHeight
    const totalH = this.rows.length * ROW_HEIGHT
    this.scrollContent.style.height = totalH + 'px'

    const scrollTop = this.viewport.scrollTop
    const start = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN)
    const end = Math.min(this.rows.length, Math.ceil((scrollTop + viewportH) / ROW_HEIGHT) + OVERSCAN)

    // Skip if the visible range hasn't changed
    if (start === this.renderStart && end === this.renderEnd) return
    this.renderStart = start
    this.renderEnd = end

    const frag = document.createDocumentFragment()
    for (let i = start; i < end; i++) {
      frag.appendChild(this.buildRow(i))
    }

    this.content.style.transform = `translateY(${start * ROW_HEIGHT}px)`
    this.content.replaceChildren(frag)
  }

  private buildRow(idx: number): HTMLElement {
    const { node, depth, expanded } = this.rows[idx]

    const div = document.createElement('div')
    div.dataset.ri = String(idx)
    div.style.cssText = `height:${ROW_HEIGHT}px;line-height:${ROW_HEIGHT}px;padding-left:${8 + depth * INDENT_PX}px;padding-right:12px;white-space:nowrap;font-family:'SF Mono','Fira Code','Cascadia Code',monospace;font-size:13px;cursor:${this.isExpandable(node) ? 'pointer' : 'default'};`

    // Hover highlight
    div.onmouseenter = () => { div.style.background = '#2a2d2e' }
    div.onmouseleave = () => { div.style.background = '' }

    // Arrow
    if (this.isExpandable(node)) {
      const arrow = document.createElement('span')
      arrow.style.cssText = 'display:inline-block;width:16px;color:#888;font-size:10px;text-align:center;'
      arrow.textContent = expanded ? '▼' : '▶'
      div.appendChild(arrow)
    } else {
      const spacer = document.createElement('span')
      spacer.style.cssText = 'display:inline-block;width:16px;'
      div.appendChild(spacer)
    }

    // Key
    if (node.key !== undefined) {
      const keySpan = document.createElement('span')
      keySpan.style.color = typeof node.key === 'number' ? '#6b9955' : '#9cdcfe'
      keySpan.textContent = String(node.key)
      div.appendChild(keySpan)

      const sep = document.createElement('span')
      sep.style.color = '#888'
      sep.textContent = ' : '
      div.appendChild(sep)
    }

    const color = KIND_COLORS[node.kind] || '#d4d4d4'

    if (node.kind === 'pointer' && node.resolvedValue != null) {
      // Value first, then PTR / CHAIN tags
      const innerKind = node.resolvedKind || 'string'
      const innerColor = KIND_COLORS[innerKind] || KIND_COLORS['string']!
      const needsQuotes = innerKind === 'string' || innerKind === 'chain'
      const valSpan = document.createElement('span')
      valSpan.style.color = innerColor
      valSpan.textContent = needsQuotes ? `"${node.resolvedValue}"` : node.resolvedValue
      div.appendChild(valSpan)
      this.appendTag(div, 'PTR', KIND_COLORS['pointer']!)
      if (innerKind === 'chain') {
        this.appendTag(div, 'CHAIN', KIND_COLORS['pathChain'] || KIND_COLORS['string']!)
      }
    } else if (node.kind === 'pathChain') {
      // Value first, then CHAIN tag
      const valSpan = document.createElement('span')
      valSpan.style.color = KIND_COLORS['string']!
      valSpan.textContent = node.resolvedValue != null ? `"${node.resolvedValue}"` : '?'
      div.appendChild(valSpan)
      this.appendTag(div, 'CHAIN', color)
    } else if (this.isExpandable(node)) {
      this.appendTag(div, KIND_TAGS[node.kind] || node.kind.toUpperCase(), color)
      if (node.childCount !== undefined) {
        const countSpan = document.createElement('span')
        countSpan.style.cssText = 'color:#888;font-size:11px;margin-left:4px;'
        countSpan.textContent = String(node.childCount)
        div.appendChild(countSpan)
      }
    } else {
      const span = document.createElement('span')
      span.style.color = color
      span.textContent = node.value ?? node.value ?? node.kind
      div.appendChild(span)
    }

    // Raw bytes preview (dimmed, truncated from left — rexc reads right-to-left)
    // For containers, show only the metadata (content..end), not the children area.
    if (node.end != null) {
      const previewStart = node.offset ?? node.start
      const previewEnd = node.end
      const nodeLen = previewEnd - previewStart
      if (nodeLen > 0) {
        const previewLen = Math.min(nodeLen, 40)
        const raw = textDecoder.decode(this.input.subarray(previewEnd - previewLen, previewEnd))
        const rawSpan = document.createElement('span')
        rawSpan.style.cssText = 'color:#444;margin-left:12px;font-size:11px;'
        rawSpan.textContent = nodeLen > 40 ? '…' + raw : raw
        div.appendChild(rawSpan)

        // Full untruncated rexc as tooltip
        const full = textDecoder.decode(this.input.subarray(previewStart, previewEnd))
        div.title = full
      }
    }

    return div
  }
}

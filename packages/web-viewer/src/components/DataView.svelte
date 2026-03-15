<script lang="ts">
	import { untrack } from 'svelte'
	import { appState } from '../lib/state.svelte'
	import { encode, open, inspect, type ASTNode } from '@creationix/rx'
	import { TAG_COLORS } from '../lib/colors.ts'
	import { docStore } from '../lib/docs.svelte'

	const ROW_HEIGHT = 24
	const INDENT_PX = 16
	const OVERSCAN = 4

	type DataRow = {
		depth: number
		key: string | number | undefined
		value: unknown
		inspectNode: ASTNode | null
		isContainer: boolean
		expanded: boolean
		path: string
	}

	let viewport = $state<HTMLDivElement | null>(null)
	let rows = $state<DataRow[]>([])
	let visibleStart = $state(0)
	let visibleEnd = $state(0)
	let errorMsg = $state<string | null>(null)
	let lastBuiltText = ''
	let filterText = $state('')
	let rootInspect = $state<ASTNode | null>(null)
	let ctxMenu = $state<{ x: number; y: number; row: DataRow } | null>(null)

	const totalHeight = $derived(rows.length * ROW_HEIGHT)
	const visibleRows = $derived(rows.slice(visibleStart, visibleEnd))

	function isContainer(v: unknown): boolean {
		return v !== null && typeof v === 'object'
	}

	function buildTree(text: string) {
		if (text === lastBuiltText) return
		lastBuiltText = text
		errorMsg = null
		filterText = ''
		if (!text.trim()) {
			rows = []
			rootInspect = null
			return
		}
		try {
			const buf = new TextEncoder().encode(text.trim())
			const root = open(buf, appState.refsEnabled ? appState.refs : undefined)
			rootInspect = inspect(buf, appState.refsEnabled ? appState.refs : undefined)

			const newRows: DataRow[] = []
			if (isContainer(root)) {
				newRows.push({ depth: 0, key: undefined, value: root, inspectNode: rootInspect!, isContainer: true, expanded: true, path: '$' })
				addContainerChildren(newRows, root, rootInspect!, 1, '$')
			} else {
				newRows.push({ depth: 0, key: undefined, value: root, inspectNode: rootInspect, isContainer: false, expanded: false, path: '$' })
			}
			rows = newRows
		} catch (e: any) {
			errorMsg = e.message
			rows = []
			rootInspect = null
		}
	}

	function addContainerChildren(target: DataRow[], value: unknown, inode: ASTNode, depth: number, parentPath: string) {
		// Resolve pointers/chains to the actual container node
		const resolved = inode.resolve

		if (Array.isArray(value)) {
			const len = (value as any).length as number
			for (let i = 0; i < len; i++) {
				const child = (value as any)[i]
				const path = `${parentPath}[${i}]`
				const container = isContainer(child)
				const childInode = resolved.tag === ';' ? resolved.index(i) ?? null : null
				target.push({ depth, key: i, value: child, inspectNode: childInode, isContainer: container, expanded: false, path })
			}
		} else if (value && typeof value === 'object') {
			if (resolved.tag === ':' || resolved.tag === ';') {
				for (const [keyNode, valNode] of resolved.entries()) {
					const key = String(keyNode.value)
					const child = (value as any)[key]
					const path = `${parentPath}.${key}`
					const container = isContainer(child)
					target.push({ depth, key, value: child, inspectNode: valNode ?? null, isContainer: container, expanded: false, path })
				}
			}
		}
	}

	function toggleExpand(idx: number) {
		const row = rows[idx]
		if (!row || !row.isContainer) return
		const newRows = [...rows]
		if (row.expanded) {
			let end = idx + 1
			while (end < newRows.length && newRows[end].depth > row.depth) end++
			newRows.splice(idx + 1, end - idx - 1)
			newRows[idx] = { ...row, expanded: false }
		} else {
			const children: DataRow[] = []
			if (row.inspectNode) {
				addContainerChildren(children, row.value, row.inspectNode, row.depth + 1, row.path)
			}
			newRows.splice(idx + 1, 0, ...children)
			newRows[idx] = { ...row, expanded: true }
		}
		rows = newRows
	}

	function applyFilter(prefix: string) {
		filterText = prefix
		if (!prefix || !rootInspect || rootInspect.tag !== ':') {
			// Rebuild from scratch
			buildTree(appState.rexcText)
			return
		}
		const buf = new TextEncoder().encode(appState.rexcText.trim())
		const root = open(buf, appState.refsEnabled ? appState.refs : undefined)
		const newRows: DataRow[] = []
		for (const [keyNode, valNode] of rootInspect.filteredKeys(prefix)) {
			const key = String(keyNode.value)
			const child = (root as any)[key]
			const container = isContainer(child)
			newRows.push({ depth: 0, key, value: child, inspectNode: valNode ?? null, isContainer: container, expanded: false, path: `$.${key}` })
		}
		rows = newRows
	}

	function formatValue(v: unknown): string {
		if (v === null) return 'null'
		if (v === undefined) return 'undefined'
		if (typeof v === 'string') {
			const display = v.length > 200 ? v.slice(0, 197) + '...' : v
			return `"${display}"`
		}
		if (typeof v === 'number' || typeof v === 'boolean') return String(v)
		return Array.isArray(v) ? '[...]' : '{...}'
	}

	function valueColor(v: unknown): string {
		if (typeof v === 'string') return '#ce9178'
		if (typeof v === 'number') return '#b5cea8'
		if (typeof v === 'boolean' || v === null || v === undefined) return '#569cd6'
		return '#d4d4d4'
	}

	function onScroll() {
		if (!viewport) return
		const scrollTop = viewport.scrollTop
		const viewportH = viewport.clientHeight
		visibleStart = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN)
		visibleEnd = Math.min(rows.length, Math.ceil((scrollTop + viewportH) / ROW_HEIGHT) + OVERSCAN)
	}

	function handleClick(e: MouseEvent) {
		if (ctxMenu) { ctxMenu = null; return }
		let el = e.target as HTMLElement | null
		while (el && el !== viewport) {
			if (el.dataset.row != null) {
				toggleExpand(parseInt(el.dataset.row))
				return
			}
			el = el.parentElement
		}
	}

	function handleContextMenu(e: MouseEvent) {
		let el = e.target as HTMLElement | null
		while (el && el !== viewport) {
			if (el.dataset.row != null) {
				e.preventDefault()
				const row = rows[parseInt(el.dataset.row)]
				if (row) ctxMenu = { x: e.clientX, y: e.clientY, row }
				return
			}
			el = el.parentElement
		}
	}

	async function copyAsJson() {
		if (!ctxMenu) return
		try {
			await navigator.clipboard.writeText(JSON.stringify(ctxMenu.row.value, null, 2))
		} catch {}
		ctxMenu = null
	}

	async function copyAsRexc() {
		if (!ctxMenu) return
		try {
			const buf = encode(ctxMenu.row.value)
			await navigator.clipboard.writeText(new TextDecoder().decode(buf))
		} catch {}
		ctxMenu = null
	}

	function extractAsDocument() {
		if (!ctxMenu) return
		const value = ctxMenu.row.value
		const rexc = new TextDecoder().decode(encode(value))
		const json = JSON.stringify(value)
		docStore.newTab()
		appState.restore({ rexcText: rexc, jsonText: json, refsText: '{}', refsEnabled: false, mode: 'data', sourceFormat: 'rexc' })
		ctxMenu = null
	}

	$effect(() => {
		const text = appState.rexcText
		untrack(() => buildTree(text))
	})

	$effect(() => {
		if (viewport) {
			onScroll()
			const observer = new ResizeObserver(() => onScroll())
			observer.observe(viewport)
			return () => observer.disconnect()
		}
	})

	const PILL_LABELS: Record<string, string> = {
		',': 'str', '.': 'chain', '^': 'ptr', ':': 'obj', ';': 'arr',
		'#': 'idx', '+': 'int', '*': 'dec', "'": 'ref',
	}

	type Pill = { label: string; color: string }

	function getPills(node: ASTNode | null): Pill[] {
		if (!node) return []
		const pills: Pill[] = []
		const tag = node.tag
		const label = PILL_LABELS[tag]
		const color = TAG_COLORS[tag]
		if (label && color) pills.push({ label, color })
		// If it's a pointer or chain, also show the resolved type
		if (tag === '^' || tag === '.') {
			try {
				const r = node.resolve
				if (r && r !== node && r.tag !== tag) {
					const rLabel = PILL_LABELS[r.tag]
					const rColor = TAG_COLORS[r.tag]
					if (rLabel && rColor) pills.push({ label: rLabel, color: rColor })
				}
			} catch { /* resolve can fail */ }
		}
		return pills
	}

	const showFilter = $derived(rootInspect?.tag === ':' && rows.length > 20)
</script>

<div class="h-full flex flex-col bg-[#0a0a0a]">
	{#if errorMsg}
		<div class="p-4 text-sm text-[#f48771]">Parse error: {errorMsg}</div>
	{:else if rows.length === 0}
		<div class="p-4 text-sm text-[#444]">No data. Switch to Source view to paste data.</div>
	{:else}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			bind:this={viewport}
			onscroll={() => { ctxMenu = null; onScroll() }}
			onclick={handleClick}
			oncontextmenu={handleContextMenu}
			class="flex-1 overflow-auto"
		>
			<div style="height: {totalHeight}px; position: relative;">
				<div style="transform: translateY({visibleStart * ROW_HEIGHT}px);">
					{#each visibleRows as row, i (visibleStart + i)}
						{@const idx = visibleStart + i}
						<div
							data-row={idx}
							class="flex items-center hover:bg-[#1a1a1a] cursor-default"
							style="height: {ROW_HEIGHT}px; line-height: {ROW_HEIGHT}px; padding-left: {8 + row.depth * INDENT_PX}px;"
						>
							<span class="font-[var(--font-mono)] text-[13px] whitespace-nowrap">
								{#if row.isContainer}
									<span class="inline-block w-4 text-center text-[10px] text-[#555] cursor-pointer">{row.expanded ? '\u25BC' : '\u25B6'}</span>
								{:else}
									<span class="inline-block w-4"></span>
								{/if}
								{#if row.key !== undefined}
									<span style="color: {typeof row.key === 'number' ? '#b5cea8' : '#9cdcfe'}">{row.key}</span>
									<span class="text-[#555]">: </span>
								{/if}
								{#if !row.isContainer}
									<span style="color: {valueColor(row.value)}">{formatValue(row.value)}</span>
								{/if}
								{#each getPills(row.inspectNode) as p}
									<span class="ml-1.5 inline-block text-[10px] leading-[16px] rounded px-[3px]" style="background:{p.color}22;color:{p.color};border:1px solid {p.color}44;">{p.label}</span>
								{/each}
								{#if row.isContainer && row.inspectNode}
									<span class="ml-1 text-[11px]" style="color: #555">{row.inspectNode.resolve.entryCount}</span>
								{/if}
							</span>
						</div>
					{/each}
				</div>
			</div>
		</div>
	{/if}
</div>

{#if ctxMenu}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="fixed inset-0 z-50" onclick={() => ctxMenu = null} oncontextmenu={(e) => { e.preventDefault(); ctxMenu = null }}>
		<div
			class="absolute bg-[#1e1e1e] border border-[#333] rounded shadow-lg py-1 text-[13px] font-[var(--font-mono)]"
			style="left: {ctxMenu.x}px; top: {ctxMenu.y}px;"
		>
			<button class="block w-full text-left px-3 py-1 text-[#ccc] hover:bg-[#094771] hover:text-white whitespace-nowrap" onclick={copyAsJson}>Copy as JSON</button>
			<button class="block w-full text-left px-3 py-1 text-[#ccc] hover:bg-[#094771] hover:text-white whitespace-nowrap" onclick={copyAsRexc}>Copy as REXC</button>
			<div class="border-t border-[#333] my-1"></div>
			<button class="block w-full text-left px-3 py-1 text-[#ccc] hover:bg-[#094771] hover:text-white whitespace-nowrap" onclick={extractAsDocument}>Extract as new document</button>
		</div>
	</div>
{/if}

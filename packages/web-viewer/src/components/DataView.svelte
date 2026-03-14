<script lang="ts">
	import { untrack } from 'svelte'
	import { appState } from '../lib/state.svelte'
	import { encode, open, inspect, handle, type ASTNode } from '@creationix/rx'

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
		knownCount: number | null  // null = never expanded, don't know yet
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

	const totalHeight = $derived(rows.length * ROW_HEIGHT)
	const visibleRows = $derived(rows.slice(visibleStart, visibleEnd))

	function isContainer(v: unknown): boolean {
		return v !== null && typeof v === 'object'
	}

	function getInspectNode(value: unknown, buf: Uint8Array): ASTNode | null {
		if (!isContainer(value)) return null
		const h = handle(value)
		if (!h) return null
		return inspect(buf, appState.refsEnabled ? appState.refs : undefined)
		// TODO: this re-inspects the whole buffer. Ideally we'd have a way to inspect at an offset.
		// For now, the inspect root is shared and we navigate to the right node via index().
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
				addContainerChildren(newRows, root, rootInspect!, 0, '')
			} else {
				newRows.push({ depth: 0, key: undefined, value: root, inspectNode: null, isContainer: false, expanded: false, knownCount: null, path: '$' })
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
				target.push({ depth, key: i, value: child, inspectNode: childInode, isContainer: container, expanded: false, knownCount: container ? peekChildCount(child, childInode) : 0, path })
			}
		} else if (value && typeof value === 'object') {
			if (resolved.tag === ':' || resolved.tag === ';') {
				for (const [keyNode, valNode] of resolved.entries()) {
					const key = String(keyNode.value)
					const child = (value as any)[key]
					const path = `${parentPath}.${key}`
					const container = isContainer(child)
					target.push({ depth, key, value: child, inspectNode: valNode ?? null, isContainer: container, expanded: false, knownCount: container ? peekChildCount(child, valNode) : 0, path })
				}
			}
		}
	}

	const PEEK_LIMIT = 10

	/** Peek at the child count of a container value via its inspect node.
	 *  Returns the exact count if <= PEEK_LIMIT, otherwise null (unknown). */
	function peekChildCount(value: unknown, inode: ASTNode | null): number | null {
		if (!inode) return null
		const resolved = inode.resolve
		if (Array.isArray(value)) {
			// For arrays, check first PEEK_LIMIT+1 elements
			let count = 0
			for (let i = 0; i <= PEEK_LIMIT; i++) {
				if ((value as any)[i] === undefined && i > 0) {
					// Could be a sparse array or end — check via inspect
					break
				}
				count++
			}
			// If we found exactly PEEK_LIMIT+1, it's a big array — unknown
			if (count > PEEK_LIMIT) return null
			return count
		}
		// For objects, iterate entries up to PEEK_LIMIT+1
		if (resolved.tag === ':' || resolved.tag === ';') {
			let count = 0
			for (const _ of resolved.entries()) {
				count++
				if (count > PEEK_LIMIT) return null
			}
			return count
		}
		return null
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
			newRows[idx] = { ...row, expanded: true, knownCount: children.length }
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
			newRows.push({ depth: 0, key, value: child, inspectNode: valNode ?? null, isContainer: container, expanded: false, knownCount: null, path: `$.${key}` })
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
		let el = e.target as HTMLElement | null
		while (el && el !== viewport) {
			if (el.dataset.row != null) {
				toggleExpand(parseInt(el.dataset.row))
				return
			}
			el = el.parentElement
		}
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
			onscroll={onScroll}
			onclick={handleClick}
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
								{#if row.isContainer}
									{@const bracket = Array.isArray(row.value) ? ['[', ']'] : ['{', '}']}
									{@const countStr = row.knownCount !== null ? String(row.knownCount) : '...'}
									<span style="color: #dcdcaa">{bracket[0]}{countStr}{bracket[1]}</span>
								{:else}
									<span style="color: {valueColor(row.value)}">{formatValue(row.value)}</span>
								{/if}
							</span>
						</div>
					{/each}
				</div>
			</div>
		</div>
	{/if}
</div>

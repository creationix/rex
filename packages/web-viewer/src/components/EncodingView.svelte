<script lang="ts">
	import { untrack } from 'svelte'
	import { appState } from '../lib/state.svelte'
	import { inspect, type ASTNode } from '@creationix/rx'
	import { renderNode, annotateNode } from '../lib/rexc-bytes.ts'
	import { TAG_COLORS, DIM_COLOR } from '../lib/colors.ts'

	const ROW_HEIGHT = 22
	const INDENT_PX = 16
	const OVERSCAN = 4
	const CONTAINER_TAGS = new Set([':', ';', '.', '*'])

	type EncRow = {
		node: ASTNode
		depth: number
		collapsed: boolean
	}

	let viewport = $state<HTMLDivElement | null>(null)
	let rows = $state<EncRow[]>([])
	let visibleStart = $state(0)
	let visibleEnd = $state(0)
	let errorMsg = $state<string | null>(null)
	let flashIdx = $state<number | null>(null)
	let lastBuiltText = ''
	let collapsed = new Set<number>()  // node.left values of collapsed containers
	let filterText = $state('')
	let rootNode = $state<ASTNode | null>(null)

	const totalHeight = $derived(rows.length * ROW_HEIGHT)
	const visibleRows = $derived(rows.slice(visibleStart, visibleEnd))
	const gutterWidth = $derived(rows.length > 0 ? Math.max(4, String(rows[rows.length - 1]?.node.right ?? 0).length) : 4)

	function isContainerTag(tag: string): boolean {
		return CONTAINER_TAGS.has(tag)
	}

	function buildRows(root: ASTNode) {
		const newRows: EncRow[] = []
		walk(root, 0, newRows)
		rows = newRows
	}

	function walk(node: ASTNode, depth: number, target: EncRow[]) {
		const isContainer = isContainerTag(node.tag)
		const isCollapsed = isContainer && collapsed.has(node.left)
		target.push({ node, depth, collapsed: isCollapsed })
		if (isContainer && !isCollapsed) {
			for (const child of node) {
				walk(child, depth + 1, target)
			}
		}
	}

	function buildTree(text: string) {
		if (text === lastBuiltText) return
		lastBuiltText = text
		errorMsg = null
		filterText = ''
		collapsed = new Set()
		if (!text.trim()) {
			rows = []
			rootNode = null
			return
		}
		try {
			const buf = new TextEncoder().encode(text.trim())
			const root = inspect(buf, appState.refsEnabled ? appState.refs : undefined)
			rootNode = root
			buildRows(root)
		} catch (e: any) {
			errorMsg = e.message
			rows = []
			rootNode = null
		}
	}

	function toggleFold(idx: number) {
		const row = rows[idx]
		if (!row || !isContainerTag(row.node.tag)) return
		const offset = row.node.left
		if (collapsed.has(offset)) {
			collapsed.delete(offset)
		} else {
			collapsed.add(offset)
		}
		// Rebuild rows from root
		if (rootNode) buildRows(rootNode)
	}

	function jumpToOffset(targetOffset: number) {
		const idx = rows.findIndex(r => r.node.left === targetOffset || r.node.right === targetOffset)
		if (idx >= 0 && viewport) {
			const scrollTarget = idx * ROW_HEIGHT - viewport.clientHeight / 2 + ROW_HEIGHT / 2
			viewport.scrollTo({ top: Math.max(0, scrollTarget), behavior: 'smooth' })
			flashIdx = idx
			setTimeout(() => { flashIdx = null }, 2000)
		}
	}

	function handleClick(e: MouseEvent) {
		let el = e.target as HTMLElement | null
		while (el && el !== viewport) {
			if (el.dataset.action === 'fold' && el.dataset.row != null) {
				toggleFold(parseInt(el.dataset.row))
				return
			}
			if (el.dataset.action === 'jump' && el.dataset.offset != null) {
				jumpToOffset(parseInt(el.dataset.offset))
				return
			}
			if (el.dataset.row != null && !el.dataset.action) {
				const idx = parseInt(el.dataset.row)
				const row = rows[idx]
				if (row && isContainerTag(row.node.tag)) {
					toggleFold(idx)
				}
				return
			}
			el = el.parentElement
		}
	}

	function applyFilter(prefix: string) {
		filterText = prefix
		if (!prefix || !rootNode || rootNode.tag !== ':') {
			if (rootNode) buildRows(rootNode)
			return
		}
		const newRows: EncRow[] = []
		for (const [keyNode, valNode] of rootNode.filteredKeys(prefix)) {
			newRows.push({ node: keyNode, depth: 0, collapsed: false })
			newRows.push({ node: valNode, depth: 0, collapsed: false })
		}
		rows = newRows
	}

	function onScroll() {
		if (!viewport) return
		const scrollTop = viewport.scrollTop
		const viewportH = viewport.clientHeight
		visibleStart = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN)
		visibleEnd = Math.min(rows.length, Math.ceil((scrollTop + viewportH) / ROW_HEIGHT) + OVERSCAN)
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

	const showFilter = $derived(rootNode?.tag === ':' && rows.length > 20)
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
						{@const node = row.node}
						{@const isC = isContainerTag(node.tag)}
						{@const ann = annotateNode(node)}
						{@const tagColor = TAG_COLORS[node.tag] || '#d4d4d4'}
						<div
							data-row={idx}
							class="flex items-center hover:bg-[#1a1a1a] group {flashIdx === idx ? 'enc-flash' : ''}"
							style="height: {ROW_HEIGHT}px; line-height: {ROW_HEIGHT}px;"
						>
							<!-- Gutter: byte offset -->
							<div
								class="shrink-0 text-right pr-2 select-none text-[11px] text-[#444] font-[var(--font-mono)]"
								style="width: {gutterWidth * 8 + 16}px;"
							>
								{node.left}
							</div>

							<!-- Content -->
							<div
								class="flex-1 min-w-0 whitespace-nowrap font-[var(--font-mono)] text-[13px]"
								style="padding-left: {row.depth * INDENT_PX}px;"
							>
								<!-- Fold arrow -->
								{#if isC}
									<span
										data-action="fold"
										data-row={idx}
										class="inline-block w-4 text-center text-[10px] text-[#555] cursor-pointer hover:text-white"
									>{row.collapsed ? '\u25B6' : '\u25BC'}</span>
								{:else}
									<span class="inline-block w-4"></span>
								{/if}

								<!-- Node content -->
								{#if node.tag === '^'}
									{@const target = node.left - (node.b64 as number)}
									<span
										data-action="jump"
										data-offset={target}
										class="cursor-pointer hover:underline"
									>{@html renderNode(node)}</span>
								{:else}
									{@html renderNode(node)}
								{/if}

								<!-- Annotation -->
								{#if node.tag === '^' && ann}
									{@const ptrTarget = node.left - (node.b64 as number)}
									<span
										data-action="jump"
										data-offset={ptrTarget}
										class="ml-2 text-[11px] cursor-pointer hover:underline"
									>{@html ann}</span>
								{:else if ann}
									<span class="ml-2 text-[11px]">{@html ann}</span>
								{/if}

								<!-- Collapsed summary -->
								{#if isC && row.collapsed}
									<span class="ml-1 text-[11px]" style="color: {DIM_COLOR}">…</span>
								{/if}
							</div>
						</div>
					{/each}
				</div>
			</div>
		</div>
	{/if}
</div>

<style>
	@keyframes enc-highlight {
		0%, 30% { background-color: rgba(255, 200, 50, 0.25); }
		100% { background-color: transparent; }
	}
	:global(.enc-flash) {
		animation: enc-highlight 2s ease-out;
	}
</style>

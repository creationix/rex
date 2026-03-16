<script lang="ts">
	import { untrack } from 'svelte'
	import { appState } from '../lib/state.svelte'
	import { docStore } from '../lib/docs.svelte'
	import type { ASTNode } from '@creationix/rx'
	import { workerCall } from '../lib/worker.ts'
	import { stringify } from '@creationix/rx'
	import { renderNode, annotateNode } from '../lib/rexc-bytes.ts'
	import { TAG_COLORS, DIM_COLOR } from '../lib/colors.ts'
	import { stringify as b64, sizeof as b64sizeof } from '@creationix/rx/b64'
	import WelcomePage from './WelcomePage.svelte'

	const ROW_HEIGHT = 22
	const INDENT_PX = 16
	const OVERSCAN = 4
	const CONTAINER_TAGS = new Set([':', ';', '.', '*'])

	type EncRow = {
		node: ASTNode
		depth: number
		opened: boolean
	}

	let viewport = $state<HTMLDivElement | null>(null)
	let rows = $state.raw<EncRow[]>([])
	let visibleStart = $state(0)
	let visibleEnd = $state(0)
	let errorMsg = $state<string | null>(null)
	let lastParsedVersion = -1
	let filterText = $state('')
	let rootNode = $state.raw<ASTNode | null>(null)
	let focusIdx = $state<number | null>(null)
	let ctxMenu = $state<{ x: number; y: number; node: ASTNode } | null>(null)
	type HistoryEntry = { nodeRight: number; scrollTop: number }
	let jumpHistory: HistoryEntry[] = []

	const totalHeight = $derived(rows.length * ROW_HEIGHT)
	const visibleRows = $derived(rows.slice(visibleStart, Math.min(visibleEnd, rows.length)))
	const gutterDigits = $derived(rootNode ? Math.max(1, b64sizeof(rootNode.right)) : 1)
	function fmtOffset(n: number): string { return b64(n).padStart(gutterDigits, '0') }

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
		const isOpened = !isContainer || appState.isOpened(node.right)
		target.push({ node, depth, opened: isOpened })
		if (isContainer && isOpened) {
			for (const child of node) {
				walk(child, depth + 1, target)
			}
		}
	}

	function buildTree() {
		const version = appState.parsedVersion
		if (version === lastParsedVersion) return
		lastParsedVersion = version
		errorMsg = appState.parsedError
		filterText = ''
		jumpHistory = []
		focusIdx = null
		const root = appState.parsedInspect
		if (!root) {
			rows = []
			rootNode = null
			return
		}
		rootNode = root
		appState.setOpened(root.right)  // root node is always open by default
		buildRows(root)
		// Restore focus from previous view if possible
		const lastRight = appState.lastFocusedNodeRight
		if (lastRight != null && rows.length > 0) {
			let idx = rows.findIndex(r => r.node.right === lastRight)
			if (idx < 0) {
				let bestIdx = -1, bestDepth = -1
				for (let i = 0; i < rows.length; i++) {
					const n = rows[i].node
					if (lastRight >= n.left - n.size && lastRight <= n.right && rows[i].depth > bestDepth) {
						bestIdx = i; bestDepth = rows[i].depth
					}
				}
				idx = bestIdx
			}
			focusIdx = idx >= 0 ? idx : 0
		} else if (rows.length > 0) {
			focusIdx = 0
		}
	}

	function toggleFold(idx: number) {
		const row = rows[idx]
		if (!row || !isContainerTag(row.node.tag)) return
		const wasOpened = row.opened
		appState.toggleOpened(row.node.right)
		if (wasOpened) {
			// Collapse: remove children (all rows after idx with depth > row.depth)
			let end = idx + 1
			while (end < rows.length && rows[end].depth > row.depth) end++
			const updated = [...rows]
			updated[idx] = { ...row, opened: false }
			updated.splice(idx + 1, end - idx - 1)
			rows = updated
		} else {
			// Expand: insert children after idx
			const children: EncRow[] = []
			walk(row.node, row.depth, children)
			// walk includes the node itself at [0], remove it — we only want its children
			children.shift()
			const updated = [...rows]
			updated[idx] = { ...row, opened: true }
			updated.splice(idx + 1, 0, ...children)
			rows = updated
		}
	}

	const isActive = $derived(appState.mode !== 'split' || appState.activePane === 'encoding')

	function setFocus(idx: number, { sync = true, scroll = true } = {}) {
		if (idx < 0 || idx >= rows.length) return
		focusIdx = idx
		if (scroll) scrollToIdx(idx)
		if (sync) appState.notifyFocusSync(rows[idx].node.right, 'encoding')
	}

	function handleExternalFocus(nodeRight: number) {
		// Exact match in current rows
		let idx = rows.findIndex(r => r.node.right === nodeRight)
		if (idx < 0) {
			// No exact match — find the nearest visible ancestor (contains the offset)
			let bestIdx = -1
			let bestDepth = -1
			for (let i = 0; i < rows.length; i++) {
				const n = rows[i].node
				if (nodeRight >= n.left - n.size && nodeRight <= n.right && rows[i].depth > bestDepth) {
					bestIdx = i
					bestDepth = rows[i].depth
				}
			}
			idx = bestIdx
		}
		if (idx >= 0) {
			focusIdx = idx
			scrollToIdx(idx)
		}
	}

	function scrollToIdx(idx: number) {
		if (!viewport) return
		const rowTop = idx * ROW_HEIGHT
		const viewportH = viewport.clientHeight
		const centered = Math.max(0, rowTop - viewportH / 2 + ROW_HEIGHT / 2)
		viewport.scrollTo({ top: centered })
		onScroll()
	}

	function jumpToOffset(targetOffset: number, pushHistory: boolean = true) {
		const match = (r: EncRow) => r.node.right === targetOffset
		// Push current focus to history before jumping
		if (pushHistory && focusIdx != null && focusIdx < rows.length) {
			jumpHistory.push({ nodeRight: rows[focusIdx].node.right, scrollTop: viewport?.scrollTop ?? 0 })
		}
		// Expand ancestors to reveal the target if needed
		if (rootNode && !rows.some(match)) {
			expandPathTo(rootNode, targetOffset)
			buildRows(rootNode)
		}
		// Expand the target itself if it's a container
		const row = rows.find(match)
		if (row && isContainerTag(row.node.tag) && !appState.isOpened(row.node.right)) {
			appState.setOpened(row.node.right)
			if (rootNode) buildRows(rootNode)
		}
		// Focus the target
		const idx = rows.findIndex(match)
		if (idx >= 0) {
			setFocus(idx)
		}
	}

	function goBack() {
		if (jumpHistory.length === 0) return
		const entry = jumpHistory.pop()!
		const idx = rows.findIndex(r => r.node.right === entry.nodeRight)
		if (idx >= 0) {
			focusIdx = idx
			// Restore scroll position, but verify the row is visible
			if (viewport) {
				const maxScroll = Math.max(0, rows.length * ROW_HEIGHT - viewport.clientHeight)
				const scrollTop = Math.min(entry.scrollTop, maxScroll)
				viewport.scrollTo({ top: scrollTop })
				onScroll()
				// If the row ended up outside the viewport, scroll to it
				const rowTop = idx * ROW_HEIGHT
				const rowBottom = rowTop + ROW_HEIGHT
				const currentTop = viewport.scrollTop
				const currentBottom = currentTop + viewport.clientHeight
				if (rowTop < currentTop || rowBottom > currentBottom) {
					scrollToIdx(idx)
				}
			}
			appState.notifyFocusSync(entry.nodeRight, 'encoding')
		}
	}

	function expandPathTo(node: ASTNode, targetOffset: number): boolean {
		if (node.right === targetOffset) return true
		if (!isContainerTag(node.tag)) return false
		if (targetOffset < node.left - node.size || targetOffset > node.right) return false
		for (const child of node) {
			if (expandPathTo(child, targetOffset)) {
				appState.setOpened(node.right)
				return true
			}
		}
		return false
	}

	function findParentIdx(idx: number): number | null {
		const row = rows[idx]
		if (!row || row.depth === 0) return null
		for (let i = idx - 1; i >= 0; i--) {
			if (rows[i].depth < row.depth) return i
		}
		return null
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Tab' && appState.mode === 'split') {
			e.preventDefault()
			appState.activePane = appState.activePane === 'encoding' ? 'data' : 'encoding'
			return
		}
		if (rows.length === 0) return
		switch (e.key) {
			case 'ArrowDown': {
				e.preventDefault()
				setFocus(focusIdx == null ? 0 : Math.min(focusIdx + 1, rows.length - 1))
				break
			}
			case 'ArrowUp': {
				e.preventDefault()
				setFocus(focusIdx == null ? 0 : Math.max(focusIdx - 1, 0))
				break
			}
			case 'ArrowRight': {
				e.preventDefault()
				if (focusIdx == null) { setFocus(0); break }
				const row = rows[focusIdx]
				if (!row) break
				if (isContainerTag(row.node.tag)) {
					if (!row.opened) {
						toggleFold(focusIdx)
					} else if (focusIdx + 1 < rows.length) {
						setFocus(focusIdx + 1)
					}
				}
				break
			}
			case 'ArrowLeft': {
				e.preventDefault()
				if (focusIdx == null) { setFocus(0); break }
				const row = rows[focusIdx]
				if (!row) break
				if (isContainerTag(row.node.tag) && row.opened) {
					toggleFold(focusIdx)
				} else {
					const parentIdx = findParentIdx(focusIdx)
					if (parentIdx != null) setFocus(parentIdx)
				}
				break
			}
			case 'Enter': {
				e.preventDefault()
				if (focusIdx == null) break
				const row = rows[focusIdx]
				if (!row) break
				if (row.node.tag === '^') {
					// Follow pointer
					const target = row.node.left - (row.node.b64 as number)
					jumpToOffset(target)
				} else if (isContainerTag(row.node.tag)) {
					toggleFold(focusIdx)
					// Re-find after rebuild
					const newIdx = rows.findIndex(r => r.node.right === row.node.right)
					if (newIdx >= 0) focusIdx = newIdx
				}
				break
			}
			case 'Backspace': {
				e.preventDefault()
				goBack()
				break
			}
			case 'PageDown': {
				e.preventDefault()
				const pageSize = viewport ? Math.floor(viewport.clientHeight / ROW_HEIGHT) : 20
				setFocus(Math.min((focusIdx ?? 0) + pageSize, rows.length - 1))
				break
			}
			case 'PageUp': {
				e.preventDefault()
				const pageSize = viewport ? Math.floor(viewport.clientHeight / ROW_HEIGHT) : 20
				setFocus(Math.max((focusIdx ?? 0) - pageSize, 0))
				break
			}
			case 'Home': {
				e.preventDefault()
				setFocus(0)
				break
			}
			case 'End': {
				e.preventDefault()
				setFocus(rows.length - 1)
				break
			}
		}
	}

	function handleClick(e: MouseEvent) {
		let el = e.target as HTMLElement | null
		// Check if the fold triangle was clicked directly
		let foldClicked = false
		while (el && el !== viewport) {
			if (el.dataset.action === 'fold' && el.dataset.row != null) {
				foldClicked = true
				break
			}
			if (el.dataset.action === 'jump' && el.dataset.offset != null) {
				jumpToOffset(parseInt(el.dataset.offset))
				return
			}
			if (el.dataset.row != null) break
			el = el.parentElement
		}
		if (!el || el === viewport) return
		const idx = parseInt(el.dataset.row!)
		const row = rows[idx]
		if (!row) return
		const wasFocused = focusIdx === idx && isActive
		// Push history before changing focus
		if (focusIdx != null && focusIdx !== idx) {
			jumpHistory.push({ nodeRight: rows[focusIdx].node.right, scrollTop: viewport?.scrollTop ?? 0 })
		}
		setFocus(idx, { scroll: false })
		// Only toggle if: fold triangle was clicked directly, or row was already focused
		if (isContainerTag(row.node.tag) && (foldClicked || wasFocused)) {
			toggleFold(idx)
			const newIdx = rows.findIndex(r => r.node.right === row.node.right)
			if (newIdx >= 0) focusIdx = newIdx
		}
	}

	function handleContextMenu(e: MouseEvent) {
		let el = e.target as HTMLElement | null
		while (el && el !== viewport) {
			if (el.dataset.row != null) {
				e.preventDefault()
				const row = rows[parseInt(el.dataset.row)]
				if (row) {
					// For index nodes (#), use the parent container instead
					let node = row.node
					if (node.tag === '#') {
						const parentIdx = findParentIdx(parseInt(el.dataset.row))
						if (parentIdx != null) node = rows[parentIdx].node
					}
					ctxMenu = { x: e.clientX, y: e.clientY, node }
				}
				return
			}
			el = el.parentElement
		}
	}

	async function copyAsRexc() {
		if (!ctxMenu) return
		try {
			await navigator.clipboard.writeText(stringify(ctxMenu.node.value) ?? '')
		} catch {}
		ctxMenu = null
	}

	async function copyAsJson() {
		if (!ctxMenu) return
		try {
			await navigator.clipboard.writeText(JSON.stringify(ctxMenu.node.value, null, 2))
		} catch {}
		ctxMenu = null
	}

	async function extractAsDocument() {
		if (!ctxMenu) return
		const value = ctxMenu.node.value
		ctxMenu = null
		// Re-encode via worker to resolve pointers
		const json = JSON.stringify(value)
		const { promise } = workerCall({ type: 'json-to-rexc', json, refs: {} })
		const { result: rexc } = await promise
		docStore.newTab()
		appState.restore({ rexcText: rexc, jsonText: '', refsText: '{}', refsEnabled: false, mode: 'encoding', sourceFormat: 'rexc' })
	}

	function applyFilter(prefix: string) {
		filterText = prefix
		if (!prefix || !rootNode || rootNode.tag !== ':') {
			if (rootNode) buildRows(rootNode)
			return
		}
		const newRows: EncRow[] = []
		for (const [keyNode, valNode] of rootNode.filteredKeys(prefix)) {
			newRows.push({ node: keyNode, depth: 0, opened: true })
			newRows.push({ node: valNode, depth: 0, opened: true })
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
		appState.parsedVersion  // track version changes
		untrack(() => buildTree())
	})

	$effect(() => {
		if (viewport) {
			onScroll()
			const observer = new ResizeObserver(() => onScroll())
			observer.observe(viewport)
			return () => observer.disconnect()
		}
	})

	// Register for focus sync from other views (only active in split mode)
	$effect(() => {
		return appState.onFocusSync((nodeRight, source) => {
			if (source !== 'encoding' && appState.mode === 'split') handleExternalFocus(nodeRight)
		})
	})

	// Rebuild rows when expand state changes from other view (split mode only)
	$effect(() => {
		return appState.onExpandChange((nodeRight, expanded) => {
			if (appState.mode !== 'split' || appState.activePane === 'encoding') return
			const idx = rows.findIndex(r => r.node.right === nodeRight)
			if (idx < 0) return
			if (expanded) {
				// Expand: insert children
				const children: EncRow[] = []
				walk(rows[idx].node, rows[idx].depth, children)
				children.shift()
				const updated = [...rows]
				updated[idx] = { ...rows[idx], opened: true }
				updated.splice(idx + 1, 0, ...children)
				rows = updated
			} else {
				// Collapse: remove children
				let end = idx + 1
				while (end < rows.length && rows[end].depth > rows[idx].depth) end++
				const updated = [...rows]
				updated[idx] = { ...rows[idx], opened: false }
				updated.splice(idx + 1, end - idx - 1)
				rows = updated
			}
		})
	})

	const showFilter = $derived(rootNode?.tag === ':' && rows.length > 20)
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
	class="h-full flex flex-col bg-[#0a0a0a] outline-none"
	tabindex="0"
	onkeydown={handleKeydown}
>
	{#if errorMsg}
		<div class="p-4 text-sm text-[#f48771]">Parse error: {errorMsg}</div>
	{:else if rows.length === 0}
		<WelcomePage />
	{:else}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			bind:this={viewport}
			onscroll={() => { ctxMenu = null; onScroll() }}
			onclick={(e) => { if (ctxMenu) { ctxMenu = null; return } handleClick(e) }}
			oncontextmenu={handleContextMenu}
			class="flex-1 overflow-auto"
		>
			<div style="height: {totalHeight + 4}px; position: relative; padding-top: 4px;">
				<div style="transform: translateY({visibleStart * ROW_HEIGHT}px);">
					{#each visibleRows as row, i (visibleStart + i)}
						{@const idx = visibleStart + i}
						{@const node = row.node}
						{@const isC = isContainerTag(node.tag)}
						{@const ann = annotateNode(node)}
						{@const tagColor = TAG_COLORS[node.tag] || '#d4d4d4'}
						<div
							data-row={idx}
							class="flex items-center group {focusIdx === idx ? (isActive ? 'bg-[#1e1e30]' : 'bg-[#181820]') : 'hover:bg-[#131313]'}"
							style="height: {ROW_HEIGHT}px; line-height: {ROW_HEIGHT}px;"
						>
							<!-- Gutter: byte offset -->
							<div
								class="shrink-0 text-right pr-2 pl-2 select-none text-[11px] font-mono {focusIdx === idx ? (isActive ? 'text-[#888]' : 'text-[#666]') : 'text-[#444]'}"
								style="width: calc({gutterDigits}ch + 1rem);"
							>
								{fmtOffset(node.right)}
							</div>

							<!-- Content -->
							<div
								class="flex-1 min-w-0 whitespace-nowrap font-mono text-[13px]"
								style="padding-left: {row.depth * INDENT_PX}px;"
							>
								<!-- Fold arrow -->
								{#if isC}
									<span
										data-action="fold"
										data-row={idx}
										class="inline-block w-4 text-center text-[10px] text-[#555] cursor-pointer hover:text-white"
									>{row.opened ? '\u25BC' : '\u25B6'}</span>
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
								{#if isC && !row.opened}
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

{#if ctxMenu}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="fixed inset-0 z-50" onclick={() => ctxMenu = null} oncontextmenu={(e) => { e.preventDefault(); ctxMenu = null }}>
		<div
			class="absolute bg-[#1e1e1e] border border-[#333] rounded shadow-lg py-1 text-[13px] font-mono"
			style="left: {ctxMenu.x}px; top: {ctxMenu.y}px;"
		>
			<button class="block w-full text-left px-3 py-1 text-[#ccc] hover:bg-[#094771] hover:text-white whitespace-nowrap" onclick={copyAsJson}>Copy as JSON</button>
			<button class="block w-full text-left px-3 py-1 text-[#ccc] hover:bg-[#094771] hover:text-white whitespace-nowrap" onclick={copyAsRexc}>Copy as REXC</button>
			<div class="border-t border-[#333] my-1"></div>
			<button class="block w-full text-left px-3 py-1 text-[#ccc] hover:bg-[#094771] hover:text-white whitespace-nowrap" onclick={extractAsDocument}>Extract as new document</button>
		</div>
	</div>
{/if}

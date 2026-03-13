<script lang="ts">
	import { untrack } from 'svelte'
	import { appState } from '../lib/state.svelte'
	import { docStore } from '../lib/docs.svelte'
	import { realParser } from '../lib/rexc-bridge.ts'
	import { KIND_COLORS, KIND_TAGS } from '../lib/colors.ts'
	import { colorizeBytes } from '../lib/rexc-bytes.ts'
	import type { RexcNode } from '../lib/rexc-parser.ts'

	const ROW_HEIGHT = 24
	const INDENT_PX = 16
	const OVERSCAN = 4

	type TreeNode = RexcNode & { depth: number, expanded: boolean, keyExpanded?: boolean, isKeyDetail?: boolean, selectorPath: (string | number)[] }

	let viewport = $state<HTMLDivElement | null>(null)
	let rows = $state<TreeNode[]>([])
	let input = $state(new Uint8Array(0))
	let visibleStart = $state(0)
	let visibleEnd = $state(0)
	let errorMsg = $state<string | null>(null)
	let lastBuiltText = ''
	let focusIdx = $state<number | null>(null)
	let flashIdx = $state<number | null>(null)

	const totalHeight = $derived(rows.length * ROW_HEIGHT)
	const visibleRows = $derived(rows.slice(visibleStart, visibleEnd))

	function isExpandable(node: RexcNode): boolean {
		if (node.kind === 'object' || node.kind === 'array' || node.kind === 'pathChain') return true
		if (node.kind === 'pointer') return node.resolvedKind === 'object' || node.resolvedKind === 'array'
		return false
	}

	function hasArrow(node: RexcNode): boolean {
		return node.kind === 'object' || node.kind === 'array' ||
			(node.kind === 'pointer' && (node.resolvedKind === 'object' || node.resolvedKind === 'array'))
	}

	function augment(node: RexcNode, depth: number, parentPath: (string | number)[], tag?: 'isKeyDetail', pathSegment?: string): TreeNode {
		const seg = pathSegment ?? (node.key !== undefined ? node.key : null)
		const selectorPath = seg !== null ? [...parentPath, seg] : parentPath
		return { ...node, depth, expanded: false, selectorPath, ...(tag ? { [tag]: true } : {}) } as TreeNode
	}

	/** Augment children with appropriate path segments based on parent kind */
	function augmentChildren(children: RexcNode[], parent: TreeNode): TreeNode[] {
		if (parent.kind === 'pathChain') {
			return children.map((c, i) => augment(c, parent.depth + 1, parent.selectorPath, undefined, `#${i}`))
		}
		return children.map(c => augment(c, parent.depth + 1, parent.selectorPath))
	}

	function buildTree(text: string) {
		if (text === lastBuiltText) return
		lastBuiltText = text
		errorMsg = null
		if (!text.trim()) {
			rows = []
			input = new Uint8Array(0)
			return
		}
		input = new TextEncoder().encode(text)
		try {
			const expandSet = new Set<number>(appState.expandedOffsets)

			const root = augment(realParser.parseRoot(input), 0, [])
			const newRows: TreeNode[] = [root]
			root.expanded = true
			newRows.push(...augmentChildren(realParser.parseChildren(input, root), root))

			if (expandSet.size > 0) {
				let i = 1
				while (i < newRows.length) {
					const node = newRows[i]
					if (expandSet.has(node.start) && isExpandable(node) && !node.expanded) {
						const children = realParser.parseChildren(input, node)
						newRows.splice(i + 1, 0, ...augmentChildren(children, node))
						newRows[i] = { ...newRows[i], expanded: true }
					}
					i++
				}
			}

			rows = newRows
		} catch (e: any) {
			errorMsg = e.message
			rows = []
		}
	}

	function removeChildren(arr: TreeNode[], idx: number, tag: 'isKeyDetail' | null) {
		const baseDepth = arr[idx].depth
		let pos = idx + 1
		while (pos < arr.length && arr[pos].depth > baseDepth) {
			const row = arr[pos]
			const match = tag === null ? !row.isKeyDetail : row[tag]
			if (match && row.depth === baseDepth + 1) {
				let end = pos + 1
				while (end < arr.length && arr[end].depth > row.depth) end++
				arr.splice(pos, end - pos)
			} else {
				pos++
			}
		}
	}

	function closeOtherPills(arr: TreeNode[], idx: number, keep: 'expanded' | 'keyExpanded') {
		const node = arr[idx]
		if (keep !== 'expanded' && node.expanded) {
			removeChildren(arr, idx, null)
			arr[idx] = { ...arr[idx], expanded: false }
		}
		if (keep !== 'keyExpanded' && node.keyExpanded) {
			removeChildren(arr, idx, 'isKeyDetail')
			arr[idx] = { ...arr[idx], keyExpanded: false }
		}
	}

	function findInsertPos(arr: TreeNode[], idx: number): number {
		const baseDepth = arr[idx].depth
		let pos = idx + 1
		while (pos < arr.length && arr[pos].depth > baseDepth && arr[pos].isKeyDetail) {
			let end = pos + 1
			while (end < arr.length && arr[end].depth > arr[pos].depth) end++
			pos = end
		}
		return pos
	}

	function getExpandedOffsets(): number[] {
		return rows.filter(r => r.expanded).map(r => r.start)
	}

	function syncExpandedOffsets() {
		appState.expandedOffsets = getExpandedOffsets().filter(o => o !== 0)
		docStore.persistViewState()
		docStore.updateUrlHash()
	}

	let lastNavOffset: number | null = null

	function saveNavState(idx: number) {
		const offset = rows[idx]?.start ?? 0
		const state = { byteOffset: offset, expanded: getExpandedOffsets() }
		if (offset === lastNavOffset) {
			history.replaceState(state, '')
		} else {
			history.pushState(state, '')
			lastNavOffset = offset
		}
	}

	function toggleExpand(idx: number) {
		const node = rows[idx]
		if (!node || !isExpandable(node)) return
		if (!jumping) saveNavState(idx)
		const newRows = [...rows]
		if (node.expanded) {
			removeChildren(newRows, idx, null)
			newRows[idx] = { ...newRows[idx], expanded: false }
		} else {
			closeOtherPills(newRows, idx, 'expanded')
			const parent = newRows[idx]
			const children = realParser.parseChildren(input, parent)
			newRows.splice(findInsertPos(newRows, idx), 0, ...augmentChildren(children, parent))
			newRows[idx] = { ...newRows[idx], expanded: true }
		}
		rows = newRows
		syncExpandedOffsets()
	}

	function toggleKeyExpand(idx: number) {
		const node = rows[idx]
		if (!node?.keyInfo || node.keyInfo.kind === 'plain') return
		if (!jumping) saveNavState(idx)
		const newRows = [...rows]
		if (node.keyExpanded) {
			removeChildren(newRows, idx, 'isKeyDetail')
			newRows[idx] = { ...newRows[idx], keyExpanded: false }
		} else {
			closeOtherPills(newRows, idx, 'keyExpanded')
			const keyNode = realParser.parseKeyNode(input, newRows[idx].keyInfo!)
			if (keyNode) {
				const children = realParser.parseChildren(input, keyNode)
				if (children.length > 0) {
					newRows.splice(idx + 1, 0, ...children.map((c, ci) => augment(c, node.depth + 1, node.selectorPath, 'isKeyDetail', `#key.${ci}`)))
				} else {
					newRows.splice(idx + 1, 0, augment(keyNode, node.depth + 1, node.selectorPath, 'isKeyDetail', '#key'))
				}
			}
			newRows[idx] = { ...newRows[idx], keyExpanded: true }
		}
		rows = newRows
		syncExpandedOffsets()
	}

	let jumping = false
	const delay = (ms: number) => new Promise(r => setTimeout(r, ms))

	function scrollToRow(idx: number, behavior: ScrollBehavior = 'instant') {
		if (!viewport) return
		const scrollTarget = idx * ROW_HEIGHT - viewport.clientHeight / 2 + ROW_HEIGHT / 2
		viewport.scrollTo({ top: Math.max(0, scrollTarget), behavior })
		if (behavior === 'instant') onScroll()
	}

	function waitForScroll(): Promise<void> {
		return new Promise(resolve => {
			if (!viewport) return resolve()
			let timer: ReturnType<typeof setTimeout>
			const done = () => { viewport!.removeEventListener('scrollend', done); clearTimeout(timer); resolve() }
			viewport.addEventListener('scrollend', done, { once: true })
			timer = setTimeout(done, 500)
		})
	}

	function expandRow(idx: number) {
		const node = rows[idx]
		if (!node || node.expanded || !isExpandable(node)) return
		const newRows = [...rows]
		const parent = newRows[idx]
		const children = realParser.parseChildren(input, parent)
		newRows.splice(findInsertPos(newRows, idx), 0, ...augmentChildren(children, parent))
		newRows[idx] = { ...newRows[idx], expanded: true }
		rows = newRows
	}

	function findRowByByteOffset(byteOffset: number): number {
		for (let i = 0; i < rows.length; i++) {
			if (rows[i].start === byteOffset || rows[i].end === byteOffset) return i
		}
		return 0
	}

	function focusRow(idx: number) {
		focusIdx = idx
		flashIdx = idx
		setTimeout(() => { flashIdx = null }, 3000)
	}

	function scrollToRowAndHighlight(idx: number) {
		scrollToRow(idx)
		focusRow(idx)
	}

	async function jumpToOffset(targetOffset: number) {
		if (jumping) return
		jumping = true

		try {
			const sourceIdx = visibleStart + Math.floor((visibleEnd - visibleStart) / 2)
			const sourceOffset = rows[sourceIdx]?.start ?? 0
			history.pushState({ byteOffset: sourceOffset, expanded: getExpandedOffsets() }, '')

			let idx = 0

			for (let depth = 0; depth < 100; depth++) {
				const node = rows[idx]
				if (!node) break
				if (node.start === targetOffset || node.end === targetOffset) break
				if (!isExpandable(node)) break

				scrollToRow(idx)
				focusIdx = idx
				await delay(300)

				if (!node.expanded) {
					expandRow(idx)
					await delay(200)
				}

				const baseDepth = rows[idx].depth
				let found = false
				for (let j = idx + 1; j < rows.length && rows[j].depth > baseDepth; j++) {
					const child = rows[j]
					if (child.isKeyDetail || child.depth !== baseDepth + 1) continue
					if (targetOffset >= child.start && targetOffset <= child.end) {
						idx = j
						found = true
						break
					}
				}
				if (!found) break
			}

			scrollToRow(idx, 'smooth')
			await waitForScroll()
			expandRow(idx)

			const destOffset = rows[idx]?.start ?? 0
			history.pushState({ byteOffset: destOffset, expanded: getExpandedOffsets() }, '')
			lastNavOffset = destOffset

			focusRow(idx)
		} finally {
			jumping = false
		}
	}

	// --- Single delegated click handler ---
	function handleTreeClick(e: MouseEvent) {
		// Walk up from target to find data-action element
		let el = e.target as HTMLElement | null
		let action: string | null = null
		let rowIdx: number | null = null

		while (el && el !== viewport) {
			if (el.dataset.action) {
				action = el.dataset.action
				break
			}
			if (el.dataset.row != null && rowIdx === null) {
				rowIdx = parseInt(el.dataset.row)
			}
			el = el.parentElement
		}

		// Also find the row index from any ancestor
		if (rowIdx === null && el) {
			let rowEl = el as HTMLElement | null
			while (rowEl && rowEl !== viewport) {
				if (rowEl.dataset.row != null) {
					rowIdx = parseInt(rowEl.dataset.row)
					break
				}
				rowEl = rowEl.parentElement
			}
		}
		// Walk up more if we found action but not row
		if (rowIdx === null) {
			let rowEl = (e.target as HTMLElement | null)
			while (rowEl && rowEl !== viewport) {
				if (rowEl.dataset.row != null) {
					rowIdx = parseInt(rowEl.dataset.row)
					break
				}
				rowEl = rowEl.parentElement
			}
		}

		if (rowIdx == null) return
		focusIdx = rowIdx

		if (action === 'expand') {
			toggleExpand(rowIdx)
		} else if (action === 'key-expand') {
			toggleKeyExpand(rowIdx)
		} else if (action === 'jump') {
			const offset = parseInt(el?.dataset.offset ?? '0')
			jumpToOffset(offset)
		} else if (hasArrow(rows[rowIdx])) {
			// Default: clicking the row toggles expand for arrow rows
			toggleExpand(rowIdx)
		}
	}

	function onScroll() {
		if (!viewport) return
		const scrollTop = viewport.scrollTop
		const viewportH = viewport.clientHeight
		visibleStart = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN)
		visibleEnd = Math.min(rows.length, Math.ceil((scrollTop + viewportH) / ROW_HEIGHT) + OVERSCAN)
	}

	function getByteRange(node: RexcNode): [number, number] {
		const start = ('offset' in node && node.offset != null) ? node.offset : node.start
		return [start, node.end]
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

	$effect(() => {
		function onPopState(e: PopStateEvent) {
			if (e.state?.byteOffset == null) return

			if (e.state.expanded && input.length > 0) {
				const expandSet = new Set<number>(e.state.expanded)
				try {
					const root = augment(realParser.parseRoot(input), 0, [])
					const newRows: TreeNode[] = [root]
					let i = 0
					while (i < newRows.length) {
						const node = newRows[i]
						if (expandSet.has(node.start) && isExpandable(node) && !node.expanded) {
							const children = realParser.parseChildren(input, node)
							newRows.splice(i + 1, 0, ...augmentChildren(children, node))
							newRows[i] = { ...newRows[i], expanded: true }
						}
						i++
					}
					rows = newRows
				} catch { /* fall through */ }
			}

			const idx = findRowByByteOffset(e.state.byteOffset)
			scrollToRowAndHighlight(idx)
		}
		window.addEventListener('popstate', onPopState)
		return () => window.removeEventListener('popstate', onPopState)
	})

	const PILL = "inline-block ml-0.5 px-1 py-0 rounded text-[9px] font-semibold tracking-wide"
	const PILL_BTN = PILL + " cursor-pointer hover:brightness-125"
</script>

<div class="h-full flex flex-col bg-[#0a0a0a]">
	{#if errorMsg}
		<div class="p-4 text-sm text-[#f48771]">
			<div>Parse error: {errorMsg}</div>
			{#if appState.rexcText}
				<pre class="mt-2 text-xs text-[#444] whitespace-pre-wrap break-all">{appState.rexcText.slice(0, 2000)}{appState.rexcText.length > 2000 ? '...' : ''}</pre>
			{/if}
		</div>
	{:else if rows.length === 0}
		<div class="p-4 text-sm text-[#444]">No data. Switch to REXC view to paste data.</div>
	{:else}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			bind:this={viewport}
			onscroll={onScroll}
			onclick={handleTreeClick}
			class="flex-1 overflow-auto"
		>
			<div style="height: {totalHeight}px; position: relative;">
				<div style="transform: translateY({visibleStart * ROW_HEIGHT}px);">
					{#each visibleRows as node, i (visibleStart + i)}
						{@const idx = visibleStart + i}
						{@const [byteStart, byteEnd] = getByteRange(node)}
						<div
							data-row={idx}
							class="flex items-center hover:bg-[#1a1a1a] group {flashIdx === idx ? 'highlight-flash' : ''} {focusIdx === idx ? 'row-focused' : ''}"
							style="height: {ROW_HEIGHT}px; line-height: {ROW_HEIGHT}px;"
						>
							<!-- Tree cell -->
							<div
								class="flex-1 min-w-0 whitespace-nowrap font-[var(--font-mono)] text-[13px] cursor-default"
								style="padding-left: {8 + node.depth * INDENT_PX}px;"
							>
								{#if hasArrow(node)}
									<span class="inline-block w-4 text-center text-[10px] text-[#555] cursor-pointer">{node.expanded ? '\u25BC' : '\u25B6'}</span>
								{:else}
									<span class="inline-block w-4"></span>
								{/if}

								{#if node.isKeyDetail}
									<span class="text-[#555] italic text-[11px]">key</span>
									<span class="text-[#555]"> = </span>
								{/if}
								{#if node.key !== undefined}
									{#if node.keyInfo?.kind === 'pointer'}
										<span style="color: #9cdcfe">{node.key}</span>
										<span data-action="jump" data-offset={node.keyInfo.targetOffset} class={PILL_BTN} style="color: {KIND_COLORS['pointer']}; background: {KIND_COLORS['pointer']}18" title="ptr@{node.keyInfo.targetOffset}">PTR</span>
									{:else if node.keyInfo?.kind === 'chain'}
										<span style="color: #9cdcfe">{node.key}</span>
										<span data-action="key-expand" class={PILL_BTN} style="color: {KIND_COLORS['pathChain']}; background: {KIND_COLORS['pathChain']}18">{node.keyExpanded ? '\u25BC' : ''} CHAIN</span>
									{:else}
										<span style="color: {typeof node.key === 'number' ? '#6b9955' : '#9cdcfe'}">{node.key}</span>
									{/if}
									<span class="text-[#555]"> : </span>
								{/if}

								{#if node.kind === 'pointer' && (node.resolvedKind === 'object' || node.resolvedKind === 'array')}
									<span data-action="jump" data-offset={node.targetOffset} class={PILL_BTN} style="color: {KIND_COLORS['pointer']}; background: {KIND_COLORS['pointer']}18" title="ptr@{node.targetOffset}">PTR</span>
									<span class={PILL} style="color: {KIND_COLORS[node.resolvedKind] || '#d4d4d4'}; background: {(KIND_COLORS[node.resolvedKind] || '#d4d4d4')}18">{KIND_TAGS[node.resolvedKind] || node.resolvedKind.toUpperCase()}</span>
								{:else if node.kind === 'pointer' && node.resolvedValue != null}
									{@const innerKind = node.resolvedKind || 'string'}
									{@const innerColor = KIND_COLORS[innerKind] || KIND_COLORS['string']}
									{@const needsQuotes = innerKind === 'string' || innerKind === 'chain'}
									<span style="color: {innerColor}">{needsQuotes ? `"${node.resolvedValue}"` : node.resolvedValue}</span>
									<span data-action="jump" data-offset={node.targetOffset} class={PILL_BTN} style="color: {KIND_COLORS['pointer']}; background: {KIND_COLORS['pointer']}18" title="ptr@{node.targetOffset}">PTR</span>
									{#if innerKind === 'chain'}
										<span class={PILL} style="color: {KIND_COLORS['pathChain']}; background: {KIND_COLORS['pathChain']}18">CHAIN</span>
									{/if}
								{:else if node.kind === 'pathChain'}
									<span style="color: {KIND_COLORS['string']}">{node.resolvedValue != null ? `"${node.resolvedValue}"` : '?'}</span>
									<span data-action="expand" class={PILL_BTN} style="color: {KIND_COLORS['pathChain']}; background: {KIND_COLORS['pathChain']}18">{node.expanded ? '\u25BC' : ''} CHAIN</span>
								{:else if node.kind === 'pointer'}
									<span data-action="jump" data-offset={node.targetOffset} class={PILL_BTN} style="color: {KIND_COLORS['pointer']}; background: {KIND_COLORS['pointer']}18" title="ptr@{node.targetOffset}">PTR</span>
								{:else if hasArrow(node)}
									<span class={PILL} style="color: {KIND_COLORS[node.kind] || '#d4d4d4'}; background: {(KIND_COLORS[node.kind] || '#d4d4d4')}18">{KIND_TAGS[node.kind] || node.kind.toUpperCase()}</span>
								{:else}
									{@const color = KIND_COLORS[node.kind] || '#d4d4d4'}
									<span style="color: {color}">{'value' in node ? node.value : node.kind}</span>
								{/if}
							</div>

							<!-- Bytes cell -->
							<div
								class="shrink-0 pr-3 whitespace-nowrap font-[var(--font-mono)] text-[11px] overflow-hidden max-w-[40%] transition-opacity {focusIdx === idx ? 'bytes-active' : 'bytes-dim'}"
								style="direction: rtl; text-overflow: ellipsis;"
								title={new TextDecoder().decode(input.subarray(byteStart, byteEnd))}
							>
								<span style="direction: ltr; unicode-bidi: bidi-override;">{@html colorizeBytes(input, byteStart, byteEnd, node.kind)}</span>
							</div>
						</div>
					{/each}
				</div>
			</div>
		</div>
	{/if}
</div>

<style>
	:global(.row-focused) {
		background-color: rgba(255, 255, 255, 0.04);
	}
	@keyframes flash-highlight {
		0%, 40% { background-color: rgba(255, 200, 50, 0.25); }
		100% { background-color: rgba(255, 255, 255, 0.04); }
	}
	:global(.highlight-flash) {
		animation: flash-highlight 3s ease-out;
	}
	:global(.bytes-dim) {
		opacity: 0.25;
	}
	:global(.group:hover .bytes-dim) {
		opacity: 0.75;
	}
	:global(.bytes-active) {
		opacity: 1;
	}
</style>

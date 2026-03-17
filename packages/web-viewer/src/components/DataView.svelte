<script lang="ts">
	import { untrack } from "svelte";
	import { appState } from "../lib/state.svelte";
	import {
		workerCall,
		workerSearchStream,
		type SearchHit,
	} from "../lib/worker.ts";
	import { stringify, b64Stringify, b64Sizeof } from "@creationix/rx";
	import type { ASTNode } from "@creationix/rx";
	import { TAG_COLORS } from "../lib/colors.ts";
	import WelcomePage from "./WelcomePage.svelte";
	import { docStore } from "../lib/docs.svelte";

	const ROW_HEIGHT = 24;
	const INDENT_PX = 16;
	const OVERSCAN = 4;
	const MAX_PREFIX_RESULTS = 2000;

	type DataRow = {
		depth: number;
		key: string | number | undefined;
		value: unknown;
		inspectNode: ASTNode | null;
		keyNode: ASTNode | null;
		isContainer: boolean;
		expanded: boolean;
		path: string;
	};

	let viewport = $state<HTMLDivElement | null>(null);
	let rows = $state.raw<DataRow[]>([]);
	let visibleStart = $state(0);
	let visibleEnd = $state(0);
	let errorMsg = $state<string | null>(null);
	let lastParsedVersion = -1;
	let filterText = $state("");
	let prefixTruncated = $state(false);
	let rootInspect = $state.raw<ASTNode | null>(null);
	let ctxMenu = $state<{ x: number; y: number; row: DataRow } | null>(null);
	let focusIdx = $state<number | null>(null);
	let searchHits = $state.raw<SearchHit[]>([]);
	let searchCursor = $state(-1);
	let searchBusy = $state(false);
	let searchTruncated = $state(false);
	let searchTotal = $state(0);
	let searchError = $state<string | null>(null);
	let loadedQuery = "";
	let searchSeq = 0;
	let cancelSearch: (() => void) | null = null;
	type HistoryEntry = { path: string; scrollTop: number };
	let jumpHistory: HistoryEntry[] = [];

	const totalHeight = $derived(rows.length * ROW_HEIGHT);
	const visibleRows = $derived(
		rows.slice(visibleStart, Math.min(visibleEnd, rows.length)),
	);
	const gutterDigits = $derived(
		rootInspect ? Math.max(1, b64Sizeof(rootInspect.right)) : 1,
	);
	const searchQueryTrim = $derived(appState.searchQuery.trim());
	const prefixMode = $derived(searchQueryTrim.startsWith("^"));
	function fmtOffset(n: number): string {
		return b64Stringify(n).padStart(gutterDigits, "0");
	}

	function isContainer(v: unknown): boolean {
		return v !== null && typeof v === "object";
	}

	function buildTree() {
		const version = appState.parsedVersion;
		if (version === lastParsedVersion) return;
		lastParsedVersion = version;
		errorMsg = appState.parsedError;
		filterText = "";
		jumpHistory = [];
		focusIdx = null;
		rootValue = appState.parsedOpen;
		rootInspect = appState.parsedInspect;
		if (!rootValue || !rootInspect) {
			rows = [];
			rootInspect = null;
			rootValue = null;
			return;
		}
		appState.setOpened(rootInspect.right); // root node is always open
		buildRows(rootValue, rootInspect);
		// Restore focus from previous view if possible
		const lastRight = appState.lastFocusedNodeRight;
		if (lastRight != null && rows.length > 0) {
			let idx = rows.findIndex((r) => r.inspectNode?.right === lastRight);
			if (idx < 0) {
				let bestIdx = -1,
					bestDepth = -1;
				for (let i = 0; i < rows.length; i++) {
					const n = rows[i].inspectNode;
					if (
						n &&
						lastRight >= n.left - n.size &&
						lastRight <= n.right &&
						rows[i].depth > bestDepth
					) {
						bestIdx = i;
						bestDepth = rows[i].depth;
					}
				}
				idx = bestIdx;
			}
			focusIdx = idx >= 0 ? idx : 0;
		} else if (rows.length > 0) {
			focusIdx = 0;
		}
	}

	let rootValue: unknown = null;

	function buildRows(root: unknown, rootNode: ASTNode) {
		const newRows: DataRow[] = [];
		const container = isContainer(root);
		const expanded = container && appState.isOpened(rootNode.right);
		newRows.push({
			depth: 0,
			key: undefined,
			value: root,
			inspectNode: rootNode,
			keyNode: null,
			isContainer: container,
			expanded,
			path: "$",
		});
		if (expanded) walk(root, rootNode, 1, "$", newRows);
		rows = newRows;
	}

	function walk(
		value: unknown,
		inode: ASTNode,
		depth: number,
		parentPath: string,
		target: DataRow[],
	) {
		const resolved = inode.resolve;

		if (Array.isArray(value)) {
			const len = (value as any).length as number;
			for (let i = 0; i < len; i++) {
				const child = (value as any)[i];
				const path = `${parentPath}[${i}]`;
				const container = isContainer(child);
				const childInode =
					resolved.tag === ";" ? (resolved.index(i) ?? null) : null;
				const expanded =
					container &&
					childInode != null &&
					appState.isOpened(childInode.right);
				target.push({
					depth,
					key: i,
					value: child,
					inspectNode: childInode,
					keyNode: null,
					isContainer: container,
					expanded,
					path,
				});
				if (expanded && childInode) {
					walk(child, childInode, depth + 1, path, target);
				}
			}
		} else if (value && typeof value === "object") {
			if (resolved.tag === ":" || resolved.tag === ";") {
				for (const [keyNode, valNode] of resolved.entries()) {
					const key = String(keyNode.value);
					const child = (value as any)[key];
					const path = `${parentPath}.${key}`;
					const container = isContainer(child);
					const expanded =
						container && valNode != null && appState.isOpened(valNode.right);
					target.push({
						depth,
						key,
						value: child,
						inspectNode: valNode ?? null,
						keyNode: keyNode,
						isContainer: container,
						expanded,
						path,
					});
					if (expanded && valNode) {
						walk(child, valNode, depth + 1, path, target);
					}
				}
			}
		}
	}

	function toggleExpand(idx: number) {
		const row = rows[idx];
		if (!row || !row.isContainer || !row.inspectNode) return;
		const wasExpanded = row.expanded;
		appState.toggleOpened(row.inspectNode.right);
		if (wasExpanded) {
			// Collapse: remove children (all rows after idx with depth > row.depth)
			let end = idx + 1;
			while (end < rows.length && rows[end].depth > row.depth) end++;
			const updated = [...rows];
			updated[idx] = { ...row, expanded: false };
			updated.splice(idx + 1, end - idx - 1);
			rows = updated;
		} else {
			// Expand: insert children after idx
			const children: DataRow[] = [];
			walk(row.value, row.inspectNode, row.depth + 1, row.path, children);
			const updated = [...rows];
			updated[idx] = { ...row, expanded: true };
			updated.splice(idx + 1, 0, ...children);
			rows = updated;
		}
	}

	const isActive = $derived(
		appState.mode !== "split" || appState.activePane === "data",
	);

	function setFocus(idx: number, { sync = true, scroll = true } = {}) {
		if (idx < 0 || idx >= rows.length) return;
		focusIdx = idx;
		if (scroll) scrollToIdx(idx);
		if (sync) {
			const row = rows[idx];
			// Prefer the key node for sync — it comes first in rexc read order
			const syncNode = row?.keyNode ?? row?.inspectNode;
			if (syncNode) {
				appState.notifyFocusSync(syncNode.right, "data");
			}
		}
	}

	function handleExternalFocus(nodeRight: number) {
		// Exact match on inspectNode.right
		let idx = rows.findIndex((r) => r.inspectNode?.right === nodeRight);
		if (idx < 0) {
			// Find the deepest row whose inspectNode range contains the target
			let bestIdx = -1;
			let bestDepth = -1;
			for (let i = 0; i < rows.length; i++) {
				const n = rows[i].inspectNode;
				if (
					n &&
					nodeRight >= n.left - n.size &&
					nodeRight <= n.right &&
					rows[i].depth > bestDepth
				) {
					bestIdx = i;
					bestDepth = rows[i].depth;
				}
			}
			idx = bestIdx;
		}
		if (idx >= 0 && idx !== focusIdx) {
			focusIdx = idx;
			scrollToIdx(idx);
		}
	}

	function scrollToIdx(idx: number) {
		if (!viewport) return;
		const rowTop = idx * ROW_HEIGHT;
		const viewportH = viewport.clientHeight;
		const centered = Math.max(0, rowTop - viewportH / 2 + ROW_HEIGHT / 2);
		viewport.scrollTo({ top: centered });
		onScroll();
	}

	function findParentIdx(idx: number): number | null {
		const row = rows[idx];
		if (!row || row.depth === 0) return null;
		for (let i = idx - 1; i >= 0; i--) {
			if (rows[i].depth < row.depth) return i;
		}
		return null;
	}

	function goBack() {
		if (jumpHistory.length === 0) return;
		const entry = jumpHistory.pop()!;
		const idx = rows.findIndex((r) => r.path === entry.path);
		if (idx >= 0) {
			focusIdx = idx;
			if (viewport) {
				const maxScroll = Math.max(
					0,
					rows.length * ROW_HEIGHT - viewport.clientHeight,
				);
				const scrollTop = Math.min(entry.scrollTop, maxScroll);
				viewport.scrollTo({ top: scrollTop });
				onScroll();
				const rowTop = idx * ROW_HEIGHT;
				const rowBottom = rowTop + ROW_HEIGHT;
				const currentTop = viewport.scrollTop;
				const currentBottom = currentTop + viewport.clientHeight;
				if (rowTop < currentTop || rowBottom > currentBottom) {
					scrollToIdx(idx);
				}
			}
			const row = rows[idx];
			if (row?.inspectNode) {
				appState.notifyFocusSync(row.inspectNode.right, "data");
			}
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
			e.preventDefault();
			appState.requestSearchFocus();
			return;
		}
		if (e.key === "Tab" && appState.mode === "split") {
			e.preventDefault();
			appState.activePane =
				appState.activePane === "data" ? "encoding" : "data";
			return;
		}
		if (rows.length === 0) return;
		switch (e.key) {
			case "ArrowDown": {
				e.preventDefault();
				setFocus(
					focusIdx == null ? 0 : Math.min(focusIdx + 1, rows.length - 1),
				);
				break;
			}
			case "ArrowUp": {
				e.preventDefault();
				setFocus(focusIdx == null ? 0 : Math.max(focusIdx - 1, 0));
				break;
			}
			case "ArrowRight": {
				e.preventDefault();
				if (focusIdx == null) {
					setFocus(0);
					break;
				}
				const row = rows[focusIdx];
				if (!row) break;
				if (row.isContainer) {
					if (!row.expanded) {
						toggleExpand(focusIdx);
					} else if (focusIdx + 1 < rows.length) {
						setFocus(focusIdx + 1);
					}
				}
				break;
			}
			case "ArrowLeft": {
				e.preventDefault();
				if (focusIdx == null) {
					setFocus(0);
					break;
				}
				const row = rows[focusIdx];
				if (!row) break;
				if (row.isContainer && row.expanded) {
					toggleExpand(focusIdx);
				} else {
					const parentIdx = findParentIdx(focusIdx);
					if (parentIdx != null) setFocus(parentIdx);
				}
				break;
			}
			case "Enter": {
				e.preventDefault();
				if (focusIdx == null) break;
				const row = rows[focusIdx];
				if (!row) break;
				if (row.isContainer) {
					toggleExpand(focusIdx);
				}
				break;
			}
			case "Backspace": {
				e.preventDefault();
				goBack();
				break;
			}
			case "PageDown": {
				e.preventDefault();
				const pageSize = viewport
					? Math.floor(viewport.clientHeight / ROW_HEIGHT)
					: 20;
				setFocus(Math.min((focusIdx ?? 0) + pageSize, rows.length - 1));
				break;
			}
			case "PageUp": {
				e.preventDefault();
				const pageSize = viewport
					? Math.floor(viewport.clientHeight / ROW_HEIGHT)
					: 20;
				setFocus(Math.max((focusIdx ?? 0) - pageSize, 0));
				break;
			}
			case "Home": {
				e.preventDefault();
				setFocus(0);
				break;
			}
			case "End": {
				e.preventDefault();
				setFocus(rows.length - 1);
				break;
			}
		}
	}

	function rowSearchText(row: DataRow): string {
		const key = row.key == null ? "" : String(row.key);
		const value = row.isContainer ? "" : formatValue(row.value);
		return `${row.path} ${key} ${value}`.toLowerCase();
	}

	function findMatch(direction: 1 | -1) {
		const q = appState.searchQuery.trim().toLowerCase();
		if (q.startsWith("^")) return;
		if (!q) return;
		if (loadedQuery !== q) startGlobalSearch(q);
		if (searchHits.length === 0) {
			if (searchBusy) return;
			let idx = focusIdx ?? (direction === 1 ? -1 : 0);
			for (let step = 0; step < rows.length; step++) {
				idx = (idx + direction + rows.length) % rows.length;
				if (rowSearchText(rows[idx]).includes(q)) {
					setFocus(idx);
					return;
				}
			}
			return;
		}
		const len = searchHits.length;
		let next = searchCursor;
		if (next < 0) next = direction === 1 ? 0 : len - 1;
		else next = (next + direction + len) % len;
		searchCursor = next;
		jumpToSearchHit(searchHits[next]);
	}

	function expandPathSegments(segments: Array<string | number>): number | null {
		if (!rootInspect || !rootValue) return null;
		appState.setOpened(rootInspect.right);
		let node: ASTNode | null = rootInspect;
		let value: any = rootValue;
		for (const seg of segments) {
			if (!node) return null;
			if (typeof seg === "number") {
				const childNode: ASTNode | null =
					node.resolve.tag === ";" ? (node.resolve.index(seg) ?? null) : null;
				if (!childNode) return null;
				node = childNode;
				value = value?.[seg];
			} else {
				let childNode: ASTNode | null = null;
				if (node.resolve.tag === ":" || node.resolve.tag === ";") {
					for (const [keyNode, valNode] of node.resolve.entries()) {
						if (String(keyNode.value) === seg) {
							childNode = valNode;
							break;
						}
					}
				}
				if (!childNode) return null;
				node = childNode;
				value = value?.[seg];
			}
			if (node && isContainer(value)) appState.setOpened(node.right);
		}
		buildRows(rootValue, rootInspect);
		onScroll();
		return node?.right ?? null;
	}

	function jumpToSearchHit(hit: SearchHit) {
		let idx = rows.findIndex((r) => r.path === hit.path);
		if (idx < 0) {
			const targetRight = expandPathSegments(hit.segments);
			if (targetRight != null)
				idx = rows.findIndex((r) => r.inspectNode?.right === targetRight);
		}
		if (idx >= 0) setFocus(idx);
	}

	function resetGlobalSearch() {
		cancelSearch?.();
		cancelSearch = null;
		searchHits = [];
		searchCursor = -1;
		searchBusy = false;
		searchTruncated = false;
		searchTotal = 0;
		searchError = null;
		loadedQuery = "";
	}

	function startGlobalSearch(query: string) {
		cancelSearch?.();
		searchHits = [];
		searchCursor = -1;
		searchBusy = true;
		searchTruncated = false;
		searchTotal = 0;
		searchError = null;
		loadedQuery = query;
		const rexc = appState.rexcText.trim();
		if (!rexc) {
			searchBusy = false;
			return;
		}
		const seq = ++searchSeq;
		const refs = appState.refsEnabled ? appState.refs : {};
		const { cancel } = workerSearchStream(
			{ rexc, refs, query, limit: 20000 },
			(hits: SearchHit[]) => {
				if (seq !== searchSeq) return;
				if (hits.length === 0) return;
				searchHits = [...searchHits, ...hits];
				searchTotal += hits.length;
			},
			(info: { total: number; truncated: boolean }) => {
				if (seq !== searchSeq) return;
				searchBusy = false;
				searchTruncated = info.truncated;
				searchTotal = info.total;
			},
			(error: Error) => {
				if (seq !== searchSeq) return;
				searchBusy = false;
				searchError = `Background search failed (${error.message}); using visible-row fallback.`;
			},
		);
		cancelSearch = cancel;
	}

	function applyFilter(prefix: string) {
		filterText = prefix;
		prefixTruncated = false;
		if (!prefix || !rootInspect || rootInspect.tag !== ":") {
			// Rebuild from scratch — force version mismatch
			lastParsedVersion = -1;
			buildTree();
			return;
		}
		const root = appState.parsedOpen;
		if (!root) return;
		const newRows: DataRow[] = [];
		for (const [keyNode, valNode] of rootInspect.filteredKeys(prefix)) {
			if (newRows.length >= MAX_PREFIX_RESULTS) {
				prefixTruncated = true;
				break;
			}
			const key = String(keyNode.value);
			const child = (root as any)[key];
			const container = isContainer(child);
			newRows.push({
				depth: 0,
				key,
				value: child,
				inspectNode: valNode ?? null,
				keyNode: keyNode,
				isContainer: container,
				expanded: false,
				path: `$.${key}`,
			});
		}
		rows = newRows;
	}

	let handledSearchNonce = -1;
	$effect(() => {
		const q = appState.searchQuery;
		const trimmed = q.trim();
		if (trimmed.startsWith("^")) {
			const prefix = trimmed.slice(1);
			resetGlobalSearch();
			if (prefix.length > 0) applyFilter(prefix);
			else if (filterText) applyFilter("");
		} else if (filterText) {
			applyFilter("");
			if (trimmed) startGlobalSearch(trimmed.toLowerCase());
			else resetGlobalSearch();
		} else if (trimmed) {
			startGlobalSearch(trimmed.toLowerCase());
		} else {
			resetGlobalSearch();
		}
	});

	$effect(() => {
		const nonce = appState.searchNonce;
		if (nonce === handledSearchNonce) return;
		handledSearchNonce = nonce;
		findMatch(appState.searchDirection);
	});

	$effect(() => {
		return () => cancelSearch?.();
	});

	function formatValue(v: unknown): string {
		if (v === null) return "null";
		if (v === undefined) return "undefined";
		if (typeof v === "string") {
			const display = v.length > 200 ? v.slice(0, 197) + "..." : v;
			return `"${display}"`;
		}
		if (typeof v === "number" || typeof v === "boolean") return String(v);
		return Array.isArray(v) ? "[...]" : "{...}";
	}

	function valueColor(v: unknown): string {
		if (typeof v === "string") return TAG_COLORS[","];
		if (typeof v === "number") return TAG_COLORS["+"];
		if (typeof v === "boolean" || v === null || v === undefined)
			return TAG_COLORS["'"];
		return TAG_COLORS[":"];
	}

	function onScroll() {
		if (!viewport) return;
		const scrollTop = viewport.scrollTop;
		const viewportH = viewport.clientHeight;
		visibleStart = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
		visibleEnd = Math.min(
			rows.length,
			Math.ceil((scrollTop + viewportH) / ROW_HEIGHT) + OVERSCAN,
		);
	}

	function handleClick(e: MouseEvent) {
		if (ctxMenu) {
			ctxMenu = null;
			return;
		}
		let el = e.target as HTMLElement | null;
		let foldClicked = false;
		while (el && el !== viewport) {
			if (el.dataset.action === "fold") {
				foldClicked = true;
			}
			if (el.dataset.row != null) break;
			el = el.parentElement;
		}
		if (!el || el === viewport || el.dataset.row == null) return;
		const idx = parseInt(el.dataset.row);
		const row = rows[idx];
		if (!row) return;
		const wasFocused = focusIdx === idx && isActive;
		// Push history before changing focus
		if (focusIdx != null && focusIdx !== idx) {
			const focusedRow = rows[focusIdx];
			if (focusedRow) {
				jumpHistory.push({
					path: focusedRow.path,
					scrollTop: viewport?.scrollTop ?? 0,
				});
			}
		}
		setFocus(idx, { scroll: false });
		// Only toggle if: fold triangle was clicked directly, or row was already focused
		if (row.isContainer && (foldClicked || wasFocused)) {
			toggleExpand(idx);
		}
	}

	function handleContextMenu(e: MouseEvent) {
		let el = e.target as HTMLElement | null;
		while (el && el !== viewport) {
			if (el.dataset.row != null) {
				e.preventDefault();
				const row = rows[parseInt(el.dataset.row)];
				if (row) ctxMenu = { x: e.clientX, y: e.clientY, row };
				return;
			}
			el = el.parentElement;
		}
	}

	async function copyAsJson() {
		if (!ctxMenu) return;
		try {
			await navigator.clipboard.writeText(
				JSON.stringify(ctxMenu.row.value, null, 2),
			);
		} catch {}
		ctxMenu = null;
	}

	async function copyAsRexc() {
		if (!ctxMenu) return;
		try {
			await navigator.clipboard.writeText(stringify(ctxMenu.row.value) ?? "");
		} catch {}
		ctxMenu = null;
	}

	async function extractAsDocument() {
		if (!ctxMenu) return;
		const value = ctxMenu.row.value;
		ctxMenu = null;
		// Re-encode via worker to resolve pointers
		const json = JSON.stringify(value);
		const { promise } = workerCall({ type: "json-to-rexc", json, refs: {} });
		const { result: rexc } = await promise;
		docStore.newTab();
		appState.restore({
			rexcText: rexc,
			jsonText: "",
			refsText: "{}",
			refsEnabled: false,
			mode: "data",
			sourceFormat: "rexc",
		});
	}

	$effect(() => {
		appState.parsedVersion; // track version changes
		untrack(() => buildTree());
	});

	$effect(() => {
		if (viewport) {
			onScroll();
			const observer = new ResizeObserver(() => onScroll());
			observer.observe(viewport);
			return () => observer.disconnect();
		}
	});

	const PILL_LABELS: Record<string, string> = {
		",": "str",
		".": "chain",
		"^": "ptr",
		":": "obj",
		";": "arr",
		"#": "idx",
		"+": "int",
		"*": "dec",
		"'": "ref",
	};

	type Pill = { label: string; color: string };

	function getPills(node: ASTNode | null): Pill[] {
		if (!node) return [];
		const pills: Pill[] = [];
		const tag = node.tag;
		const label = PILL_LABELS[tag];
		const color = TAG_COLORS[tag];
		if (label && color) pills.push({ label, color });
		// If it's a pointer or chain, also show the resolved type
		if (tag === "^" || tag === ".") {
			try {
				const r = node.resolve;
				if (r && r !== node && r.tag !== tag) {
					const rLabel = PILL_LABELS[r.tag];
					const rColor = TAG_COLORS[r.tag];
					if (rLabel && rColor) pills.push({ label: rLabel, color: rColor });
				}
			} catch {
				/* resolve can fail */
			}
		}
		return pills;
	}

	// Register for focus sync from other views
	$effect(() => {
		return appState.onFocusSync((nodeRight, source) => {
			if (source !== "data" && appState.mode === "split")
				handleExternalFocus(nodeRight);
		});
	});

	// Rebuild rows when expand state changes from other view (split mode only)
	$effect(() => {
		return appState.onExpandChange((nodeRight, expanded) => {
			if (appState.mode !== "split" || appState.activePane === "data") return;
			const idx = rows.findIndex((r) => r.inspectNode?.right === nodeRight);
			if (idx < 0) return;
			const row = rows[idx];
			if (expanded) {
				const children: DataRow[] = [];
				if (row.inspectNode)
					walk(row.value, row.inspectNode, row.depth + 1, row.path, children);
				const updated = [...rows];
				updated[idx] = { ...row, expanded: true };
				updated.splice(idx + 1, 0, ...children);
				rows = updated;
			} else {
				let end = idx + 1;
				while (end < rows.length && rows[end].depth > row.depth) end++;
				const updated = [...rows];
				updated[idx] = { ...row, expanded: false };
				updated.splice(idx + 1, end - idx - 1);
				rows = updated;
			}
		});
	});

	const showFilter = $derived(rootInspect?.tag === ":" && rows.length > 20);
</script>

<div
	class="h-full flex flex-col bg-[#0a0a0a] outline-none"
	tabindex="0"
	role="tree"
	aria-label="Data tree"
	aria-activedescendant={focusIdx != null ? `data-row-${focusIdx}` : undefined}
	onkeydown={handleKeydown}
>
	{#if errorMsg}
		<div class="p-4 text-sm text-[#f48771]">Parse error: {errorMsg}</div>
	{:else}
		{#if searchQueryTrim && !prefixMode}
			<div
				class="px-3 py-1.5 text-[11px] text-[#888] border-b border-[#222] bg-[#111]"
			>
				{#if searchError}
					{searchError}
				{:else if searchBusy}
					Searching all nodes for "{searchQueryTrim}"...
				{:else if searchTotal > 0}
					{searchTotal}{searchTruncated ? "+" : ""} matches for "{searchQueryTrim}".
					Use Enter/Shift+Enter or arrows to jump.
				{:else}
					No matches for "{searchQueryTrim}".
				{/if}
			</div>
		{/if}
		{#if prefixTruncated}
			<div
				class="px-3 py-1.5 text-[11px] text-[#dcdcaa] border-b border-[#333] bg-[#171717]"
			>
				Showing first {MAX_PREFIX_RESULTS} prefix matches. Add more characters to
				narrow results.
			</div>
		{/if}
		{#if rows.length === 0}
			{#if filterText}
				<div class="p-4 text-sm text-[#888]">
					No matches for prefix "{filterText}".
				</div>
			{:else}
				<WelcomePage />
			{/if}
		{:else}
			<!-- svelte-ignore a11y_click_events_have_key_events -->
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div
				bind:this={viewport}
				onscroll={() => {
					ctxMenu = null;
					onScroll();
				}}
				onclick={handleClick}
				oncontextmenu={handleContextMenu}
				class="flex-1 overflow-auto"
			>
				<div
					style="height: {totalHeight +
						4}px; position: relative; padding-top: 4px;"
				>
					<div style="transform: translateY({visibleStart * ROW_HEIGHT}px);">
						{#each visibleRows as row, i (visibleStart + i)}
							{@const idx = visibleStart + i}
							<div
								id={`data-row-${idx}`}
								data-row={idx}
								role="treeitem"
								aria-level={row.depth + 1}
								aria-expanded={row.isContainer ? row.expanded : undefined}
								aria-selected={focusIdx === idx}
								class="flex items-center cursor-default {focusIdx === idx
									? isActive
										? 'bg-[#1e1e30]'
										: 'bg-[#181820]'
									: 'hover:bg-[#131313]'}"
								style="height: {ROW_HEIGHT}px; line-height: {ROW_HEIGHT}px;"
							>
								<!-- Gutter: byte offset -->
								<div
									class="shrink-0 text-right pr-2 pl-2 select-none text-[11px] font-mono {focusIdx ===
									idx
										? isActive
											? 'text-[#888]'
											: 'text-[#666]'
										: 'text-[#444]'}"
									style="width: calc({gutterDigits}ch + 1rem);"
								>
									{fmtOffset((row.keyNode ?? row.inspectNode)?.right ?? 0)}
								</div>
								<span
									class="font-mono text-[13px] whitespace-nowrap"
									style="padding-left: {row.depth * INDENT_PX}px;"
								>
									{#if row.isContainer}
										<button
											type="button"
											data-action="fold"
											data-row={idx}
											aria-label={row.expanded
												? "Collapse node"
												: "Expand node"}
											class="inline-block w-4 text-center text-[10px] text-[#555] cursor-pointer"
											>{row.expanded ? "\u25BC" : "\u25B6"}</button
										>
									{:else}
										<span class="inline-block w-4"></span>
									{/if}
									{#if row.key !== undefined}
										<span
											style="color: {typeof row.key === 'number'
												? TAG_COLORS['+']
												: TAG_COLORS['key']}">{row.key}</span
										>
										{#each getPills(row.keyNode) as p}
											<span
												class="ml-1.5 inline-block text-[10px] leading-[16px] rounded px-[3px]"
												style="background:{p.color}22;color:{p.color};border:1px solid {p.color}44;"
												>{p.label}</span
											>
										{/each}
										<span class="text-[#555]">: </span>
									{/if}
									{#if !row.isContainer}
										<span style="color: {valueColor(row.value)}"
											>{formatValue(row.value)}</span
										>
									{/if}
									{#each getPills(row.inspectNode) as p}
										<span
											class="ml-1.5 inline-block text-[10px] leading-[16px] rounded px-[3px]"
											style="background:{p.color}22;color:{p.color};border:1px solid {p.color}44;"
											>{p.label}</span
										>
									{/each}
									{#if row.isContainer && row.inspectNode}
										<span class="ml-1 text-[11px]" style="color: #555"
											>{row.inspectNode.resolve.entryCount}</span
										>
									{/if}
								</span>
							</div>
						{/each}
					</div>
				</div>
			</div>
		{/if}
	{/if}
</div>

{#if ctxMenu}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="fixed inset-0 z-50"
		onclick={() => (ctxMenu = null)}
		oncontextmenu={(e) => {
			e.preventDefault();
			ctxMenu = null;
		}}
	>
		<div
			class="absolute bg-[#1e1e1e] border border-[#333] rounded shadow-lg py-1 text-[13px] font-mono"
			style="left: {ctxMenu.x}px; top: {ctxMenu.y}px;"
		>
			<button
				class="block w-full text-left px-3 py-1 text-[#ccc] hover:bg-[#094771] hover:text-white whitespace-nowrap"
				onclick={copyAsJson}>Copy as JSON</button
			>
			<button
				class="block w-full text-left px-3 py-1 text-[#ccc] hover:bg-[#094771] hover:text-white whitespace-nowrap"
				onclick={copyAsRexc}>Copy as REXC</button
			>
			<div class="border-t border-[#333] my-1"></div>
			<button
				class="block w-full text-left px-3 py-1 text-[#ccc] hover:bg-[#094771] hover:text-white whitespace-nowrap"
				onclick={extractAsDocument}>Extract as new document</button
			>
		</div>
	</div>
{/if}

<script lang="ts">
	import EncodingView from "./components/EncodingView.svelte";
	import DataTreeView from "./components/DataView.svelte";
	import { appState, type Mode } from "./lib/state.svelte";

	let fileName = $state("");
	let dataPane = $state<HTMLDivElement | null>(null);
	let encodingPane = $state<HTMLDivElement | null>(null);

	const modes: { id: Mode; label: string }[] = [
		{ id: "data", label: "DATA" },
		{ id: "split", label: "SPLIT" },
		{ id: "encoding", label: "ENCODING" },
	];

	function switchTo(id: Mode) {
		appState.mode = id;
	}

	$effect(() => {
		const pane = appState.activePane;
		if (appState.mode !== "split") return;
		const target = pane === "data" ? dataPane : encodingPane;
		const focusable = target?.querySelector<HTMLElement>("[tabindex]");
		focusable?.focus();
	});

	function humanSize(bytes: number) {
		if (bytes < 1024) return bytes + " B";
		if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KiB";
		return (bytes / (1024 * 1024)).toFixed(2) + " MiB";
	}

	const compactJsonSize = $derived(appState.compactJsonSize);

	let searchInput = $state<HTMLInputElement | null>(null);

	function onSearchInput(e: Event) {
		const target = e.currentTarget as HTMLInputElement | null;
		if (target) appState.searchQuery = target.value;
	}

	function onSearchKeydown(e: KeyboardEvent) {
		if (e.key === "Enter") {
			e.preventDefault();
			appState.requestSearch(e.shiftKey ? -1 : 1);
		}
	}

	$effect(() => {
		appState.searchFocusNonce;
		searchInput?.focus();
		searchInput?.select();
	});

	// Listen for file content from VS Code extension
	window.addEventListener("message", (e) => {
		const msg = e.data;
		if (msg.type === "load") {
			fileName = msg.name ?? "";
			appState.loadFile(msg.name ?? "file", msg.content);
		}
	});

	// Signal to VS Code that the webview is ready
	const vscode = (window as any).acquireVsCodeApi?.();
	vscode?.postMessage({ type: "ready" });
</script>

<div class="vscode flex flex-col h-screen">
	<header
		class="flex items-center justify-between px-2 sm:px-4 py-2 border-b border-border bg-bg-toolbar gap-2"
	>
		<div class="flex items-center gap-2 sm:gap-4 min-w-0">
			<h1 class="text-sm font-semibold text-text-bright tracking-tight">
				RX Viewer
			</h1>
			{#if fileName}
				<span
					class="hidden sm:inline text-xs text-text-placeholder truncate max-w-48"
					>{fileName}</span
				>
			{/if}
			<div
				class="flex rounded-md border border-border bg-bg-deep overflow-hidden"
			>
				{#each modes as m}
					<button
						onclick={() => switchTo(m.id)}
						class="px-3 py-1 text-xs font-medium transition-colors
						{appState.mode === m.id
							? 'bg-btn-primary-bg text-btn-primary-text cursor-pointer'
							: 'text-text-dim hover:text-text-bright cursor-pointer'}"
					>
						{m.label}
					</button>
				{/each}
			</div>
		</div>
		<div class="flex items-center gap-1 sm:gap-3 min-w-0">
			<input
				bind:this={searchInput}
				type="text"
				value={appState.searchQuery}
				oninput={onSearchInput}
				onkeydown={onSearchKeydown}
				placeholder="Find in tree (prefix: ^key)"
				class="w-28 sm:w-44 md:w-56 px-2 py-1 text-[11px] sm:text-xs bg-bg-deep border border-border rounded text-text-bright outline-none focus:border-border-focus"
			/>
			<button
				type="button"
				onclick={() => appState.requestSearch(-1)}
				class="text-[10px] sm:text-xs px-1.5 py-1 rounded border border-border text-text-dim hover:text-text-bright"
				aria-label="Find previous">&#9664;</button
			>
			<button
				type="button"
				onclick={() => appState.requestSearch(1)}
				class="text-[10px] sm:text-xs px-1.5 py-1 rounded border border-border text-text-dim hover:text-text-bright"
				aria-label="Find next">&#9654;</button
			>
			{#if appState.rexcSize > 0}
				<span class="hidden sm:inline text-xs text-text-label">RX</span>
				<span class="hidden sm:inline text-xs text-text-muted"
					>{humanSize(appState.rexcSize)}</span
				>
				{#if compactJsonSize > 0}
					<span class="hidden sm:inline text-xs text-text-label"
						>JSON</span
					>
					<span class="hidden sm:inline text-xs text-text-muted"
						>{humanSize(compactJsonSize)}</span
					>
				{/if}
			{/if}
		</div>
	</header>

	<main
		class="flex-1 min-h-0 min-w-0 relative {appState.mode === 'split'
			? 'flex'
			: ''}"
	>
		{#if appState.mode === "encoding"}
			<EncodingView />
		{:else if appState.mode === "data"}
			<DataTreeView />
		{:else if appState.mode === "split"}
			<div
				bind:this={dataPane}
				class="min-w-0 flex-1"
			>
				<DataTreeView />
			</div>
			<div class="w-px bg-border"></div>
			<div
				bind:this={encodingPane}
				class="min-w-0 flex-1"
			>
				<EncodingView />
			</div>
		{/if}
		{#if appState.error}
			<div
				class="absolute bottom-0 left-0 right-0 px-4 py-2 bg-bg-error border-t border-border-error text-xs text-error"
			>
				{appState.error}
			</div>
		{/if}
	</main>
</div>

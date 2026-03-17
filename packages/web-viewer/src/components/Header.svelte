<script lang="ts">
	import ModeToggle from './ModeToggle.svelte'
	import HelpOverlay from './HelpOverlay.svelte'
	import { appState } from '../lib/state.svelte'
	import { docStore } from '../lib/docs.svelte'

	let helpOpen = $state(false)
	let searchInput = $state<HTMLInputElement | null>(null)

	function humanSize(bytes: number) {
		if (bytes < 1024) return bytes + ' B'
		if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KiB'
		return (bytes / (1024 * 1024)).toFixed(2) + ' MiB'
	}

	const compactJsonSize = $derived(appState.compactJsonSize)

	function onSearchInput(e: Event) {
		const target = e.currentTarget as HTMLInputElement | null
		if (target) appState.searchQuery = target.value
	}

	function onSearchKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault()
			appState.requestSearch(e.shiftKey ? -1 : 1)
		}
	}

	$effect(() => {
		appState.searchFocusNonce
		searchInput?.focus()
		searchInput?.select()
	})
</script>

<header class="flex items-center justify-between px-2 sm:px-4 py-2 border-b border-[#333] bg-[#111] gap-2">
	<div class="flex items-center gap-2 sm:gap-4 min-w-0">
		<h1 class="text-sm font-semibold text-white tracking-tight">REXC Viewer</h1>
		{#if docStore.currentTab}
			<span class="hidden sm:inline text-xs text-[#555] truncate max-w-48">{docStore.currentTab.name}{docStore.currentTab.saved ? '' : ' *'}</span>
		{/if}
		<ModeToggle />
	</div>
	<div class="flex items-center gap-1 sm:gap-3 min-w-0">
		<input
			bind:this={searchInput}
			type="text"
			value={appState.searchQuery}
			oninput={onSearchInput}
			onkeydown={onSearchKeydown}
			placeholder="Find in tree (prefix: ^key)"
			class="w-28 sm:w-44 md:w-56 px-2 py-1 text-[11px] sm:text-xs bg-[#0a0a0a] border border-[#333] rounded text-white outline-none focus:border-[#555]"
		/>
		<button
			type="button"
			onclick={() => appState.requestSearch(-1)}
			class="text-[10px] sm:text-xs px-1.5 py-1 rounded border border-[#333] text-[#888] hover:text-white"
			aria-label="Find previous"
		>◀</button>
		<button
			type="button"
			onclick={() => appState.requestSearch(1)}
			class="text-[10px] sm:text-xs px-1.5 py-1 rounded border border-[#333] text-[#888] hover:text-white"
			aria-label="Find next"
		>▶</button>
		{#if appState.rexcSize > 0}
			<span class="hidden sm:inline text-xs text-[#444]">RX</span>
			<span class="hidden sm:inline text-xs text-[#666]">{humanSize(appState.rexcSize)}</span>
			{#if compactJsonSize > 0}
				<span class="hidden sm:inline text-xs text-[#444]">JSON</span>
				<span class="hidden sm:inline text-xs text-[#666]">{humanSize(compactJsonSize)}</span>
			{/if}
		{/if}
		<button
			type="button"
			onclick={() => { helpOpen = true }}
			class="text-xs px-2.5 py-1 rounded-md border border-[#333] bg-[#111] text-[#888] hover:text-white hover:border-[#555] transition-colors cursor-pointer"
		>
			?
		</button>
	</div>
</header>

<HelpOverlay bind:open={helpOpen} />

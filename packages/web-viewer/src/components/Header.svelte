<script lang="ts">
	import ModeToggle from './ModeToggle.svelte'
	import HelpOverlay from './HelpOverlay.svelte'
	import { appState } from '../lib/state.svelte'
	import { docStore } from '../lib/docs.svelte'

	let helpOpen = $state(false)

	function humanSize(bytes: number) {
		if (bytes < 1024) return bytes + ' B'
		if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KiB'
		return (bytes / (1024 * 1024)).toFixed(2) + ' MiB'
	}

	const compactJsonSize = $derived(appState.compactJsonSize)
</script>

<header class="flex items-center justify-between px-4 py-2 border-b border-[#333] bg-[#111]">
	<div class="flex items-center gap-4">
		<h1 class="text-sm font-semibold text-white tracking-tight">REXC Viewer</h1>
		{#if docStore.currentTab}
			<span class="text-xs text-[#555]">{docStore.currentTab.name}{docStore.currentTab.saved ? '' : ' *'}</span>
		{/if}
		<ModeToggle />
	</div>
	<div class="flex items-center gap-3">
		{#if appState.rexcSize > 0}
			<span class="text-xs text-[#444]">RX</span>
			<span class="text-xs text-[#666]">{humanSize(appState.rexcSize)}</span>
			{#if compactJsonSize > 0}
				<span class="text-xs text-[#444]">JSON</span>
				<span class="text-xs text-[#666]">{humanSize(compactJsonSize)}</span>
			{/if}
		{/if}
		<button
			onclick={() => { helpOpen = true }}
			class="text-xs px-2.5 py-1 rounded-md border border-[#333] bg-[#111] text-[#888] hover:text-white hover:border-[#555] transition-colors cursor-pointer"
		>
			?
		</button>
	</div>
</header>

<HelpOverlay bind:open={helpOpen} />

<script lang="ts">
	import { appState, type Mode } from '../lib/state.svelte'
	import { docStore } from '../lib/docs.svelte'

	const modes: { id: Mode; label: string }[] = [
		{ id: 'data', label: 'DATA' },
		{ id: 'split', label: 'SPLIT' },
		{ id: 'encoding', label: 'ENCODING' },
	]

	async function switchTo(id: Mode) {
		if (id === appState.mode) return
		await appState.switchMode(id)
		docStore.persistViewState()
		docStore.updateUrlHash(true)
	}
</script>

<div class="flex rounded-md border border-[#333] bg-[#0a0a0a] overflow-hidden">
	{#each modes as m}
		{@const hasDoc = !!docStore.activeId}
		<button
			onclick={() => switchTo(m.id)}
			disabled={!hasDoc}
			class="px-3 py-1 text-xs font-medium transition-colors
				{!hasDoc
					? 'text-[#444] cursor-default'
					: appState.mode === m.id
						? 'bg-white text-black cursor-pointer'
						: 'text-[#888] hover:text-white cursor-pointer'}"
		>
			{m.label}
		</button>
	{/each}
</div>

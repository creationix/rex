<script lang="ts">
	import { appState } from '../lib/state.svelte'

	function humanSize(bytes: number) {
		if (bytes < 1024) return bytes + ' B'
		if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KiB'
		return (bytes / (1024 * 1024)).toFixed(2) + ' MiB'
	}

	const allFresh = $derived(appState.rexcFresh && appState.jsonFresh)

	const compactJsonSize = $derived(() => {
		if (!appState.jsonFresh || !appState.jsonText) return 0
		try {
			return JSON.stringify(JSON.parse(appState.jsonText)).length
		} catch { return 0 }
	})

	const ratio = $derived(() => {
		const compact = compactJsonSize()
		if (allFresh && compact > 0 && appState.rexcSize > 0) {
			return ((appState.rexcSize / compact) * 100).toFixed(1) + '%'
		}
		return null
	})

	const savings = $derived(() => {
		const compact = compactJsonSize()
		if (allFresh && compact > 0 && appState.rexcSize > 0) {
			const diff = compact - appState.rexcSize
			if (diff > 0) return humanSize(diff) + ' saved'
			if (diff < 0) return humanSize(-diff) + ' larger'
		}
		return null
	})
</script>

<div class="flex gap-6 px-4 py-2.5 border-t border-[#222] bg-[#0a0a0a] text-xs text-[#666]">
	<div class="flex gap-1.5">
		<span class="text-[#444]">REXC</span>
		<span class="{appState.rexcFresh ? 'text-[#999]' : 'text-[#444] italic'}">{humanSize(appState.rexcSize)}{appState.rexcFresh ? '' : ' ~'}</span>
	</div>
	<div class="flex gap-1.5">
		<span class="text-[#444]">JSON</span>
		{#if appState.jsonFresh && compactJsonSize() > 0 && compactJsonSize() < appState.jsonSize}
			<span class="text-[#999]">{humanSize(compactJsonSize())}</span>
			<span class="text-[#444]">({humanSize(appState.jsonSize)} pretty)</span>
		{:else}
			<span class="{appState.jsonFresh ? 'text-[#999]' : 'text-[#444] italic'}">{humanSize(appState.jsonSize)}{appState.jsonFresh ? '' : ' ~'}</span>
		{/if}
	</div>
	{#if ratio()}
		<div class="flex gap-1.5">
			<span class="text-[#444]">Ratio</span>
			<span class="text-[#999]">{ratio()}</span>
		</div>
	{/if}
	{#if savings()}
		<div class="flex gap-1.5">
			<span class="text-[#4ec9b0]">{savings()}</span>
		</div>
	{/if}
</div>

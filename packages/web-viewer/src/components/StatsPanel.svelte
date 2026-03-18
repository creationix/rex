<script lang="ts">
	import { appState } from '../lib/state.svelte'

	function humanSize(bytes: number) {
		if (bytes < 1024) return bytes + ' B'
		if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KiB'
		return (bytes / (1024 * 1024)).toFixed(2) + ' MiB'
	}

	const allFresh = $derived(appState.rexcFresh && appState.jsonFresh)
	const compactJsonSize = $derived(appState.compactJsonSize)

	const ratio = $derived.by(() => {
		if (allFresh && compactJsonSize > 0 && appState.rexcSize > 0) {
			return ((appState.rexcSize / compactJsonSize) * 100).toFixed(1) + '%'
		}
		return null
	})

	const savings = $derived.by(() => {
		if (allFresh && compactJsonSize > 0 && appState.rexcSize > 0) {
			const diff = compactJsonSize - appState.rexcSize
			if (diff > 0) return humanSize(diff) + ' saved'
			if (diff < 0) return humanSize(-diff) + ' larger'
		}
		return null
	})
</script>

<div class="flex gap-6 px-4 py-2.5 border-t border-border-subtle bg-bg-deep text-xs text-text-muted">
	<div class="flex gap-1.5">
		<span class="text-text-label">RX</span>
		<span class="{appState.rexcFresh ? 'text-text-dim' : 'text-text-label italic'}">{humanSize(appState.rexcSize)}{appState.rexcFresh ? '' : ' ~'}</span>
	</div>
	<div class="flex gap-1.5">
		<span class="text-text-label">JSON</span>
		{#if appState.jsonFresh && compactJsonSize > 0 && compactJsonSize < appState.jsonSize}
			<span class="text-text-dim">{humanSize(compactJsonSize)}</span>
			<span class="text-text-label">({humanSize(appState.jsonSize)} pretty)</span>
		{:else}
			<span class="{appState.jsonFresh ? 'text-text-dim' : 'text-text-label italic'}">{humanSize(appState.jsonSize)}{appState.jsonFresh ? '' : ' ~'}</span>
		{/if}
	</div>
	{#if ratio}
		<div class="flex gap-1.5">
			<span class="text-text-label">Ratio</span>
			<span class="text-text-dim">{ratio}</span>
		</div>
	{/if}
	{#if savings}
		<div class="flex gap-1.5">
			<span class="text-accent">{savings}</span>
		</div>
	{/if}
</div>

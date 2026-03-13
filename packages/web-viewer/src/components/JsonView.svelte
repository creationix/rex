<script lang="ts">
	import CodeMirrorEditor from './CodeMirrorEditor.svelte'
	import StatsPanel from './StatsPanel.svelte'
	import Spinner from './Spinner.svelte'
	import { appState } from '../lib/state.svelte'

	let debounceTimer: ReturnType<typeof setTimeout> | undefined
	let localJson = $state(appState.jsonText)
	let editing = $state(false)

	function handleChange(value: string) {
		localJson = value
		editing = true
		clearTimeout(debounceTimer)
		debounceTimer = setTimeout(() => {
			appState.setJson(value)
			editing = false
			// Sync rexc in background so stats panel has fresh rexc size
			appState.syncRexc().catch(() => {})
		}, 300)
	}

	// Sync external changes (e.g. tab switch) into local, but not while editing
	$effect(() => {
		if (!editing) localJson = appState.jsonText
	})

	const stale = $derived(!appState.jsonFresh)
</script>

<div class="h-full flex flex-col bg-[#0a0a0a]">
	<div class="flex-1 min-h-0 relative">
		<div class="h-full transition-opacity {stale ? 'opacity-40 pointer-events-none' : ''}">
			<CodeMirrorEditor value={localJson} onchange={handleChange} readonly={stale} />
		</div>
		{#if stale && appState.converting}
			<Spinner />
		{/if}
	</div>
	<StatsPanel />
</div>

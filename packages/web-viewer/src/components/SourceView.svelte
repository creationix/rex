<script lang="ts">
	import { untrack } from 'svelte'
	import { appState, type SourceFormat } from '../lib/state.svelte'
	import { docStore } from '../lib/docs.svelte'
	import StatsPanel from './StatsPanel.svelte'

	let localRexc = $state(appState.rexcText)
	let localJson = $state(appState.jsonText)

	// Sync from appState when it changes externally (e.g. tab switch).
	// Use untrack on the local var so we don't subscribe to it, and only
	// assign when the value actually changed (avoids round-tripping user input).
	$effect(() => { const t = appState.rexcText; if (t !== untrack(() => localRexc)) localRexc = t })
	$effect(() => { const t = appState.jsonText;  if (t !== untrack(() => localJson))  localJson = t })

	function handleRexcInput(e: Event) {
		const text = (e.target as HTMLTextAreaElement).value
		localRexc = text
		appState.setRexc(text)
		// Background sync so JSON size is available in stats
		appState.syncJson()
	}

	function handleJsonInput(e: Event) {
		const text = (e.target as HTMLTextAreaElement).value
		localJson = text
		appState.setJson(text)
		appState.syncRexc()
	}

	async function handlePaste(e: ClipboardEvent) {
		const text = e.clipboardData?.getData('text') ?? ''
		if (!text) return
		const wasEmpty = !appState.rexcText.trim() && !appState.jsonText.trim()
		// Auto-detect: try parsing as rexc first — if the span covers the whole input, it's rexc
		if (!appState.isValidRexc(text.trim())) {
			// Not valid rexc — treat as JSON
			if (appState.sourceFormat !== 'json') {
				appState.sourceFormat = 'json'
			}
			localJson = text
			appState.setJson(text)
			appState.syncRexc()
			e.preventDefault()
			if (wasEmpty) requestAnimationFrame(() => appState.switchMode('data'))
			return
		}
		// rexc paste — let the textarea handle it, then switch if was empty
		if (wasEmpty) {
			requestAnimationFrame(() => appState.switchMode('data'))
		}
	}

	async function toggleFormat() {
		const newFormat: SourceFormat = appState.sourceFormat === 'rexc' ? 'json' : 'rexc'
		if (newFormat === 'json') {
			await appState.syncJson()
			localJson = appState.jsonText
		} else {
			await appState.syncRexc()
			localRexc = appState.rexcText
		}
		appState.sourceFormat = newFormat
		docStore.persistViewState()
	}
</script>

<div class="h-full flex flex-col bg-[#0a0a0a]">
	<!-- Format toggle bar -->
	<div class="flex items-center gap-2 px-4 py-1.5 border-b border-[#222]">
		<div class="flex rounded-md border border-[#333] bg-[#0a0a0a] overflow-hidden">
			<button
				onclick={toggleFormat}
				class="px-2 py-0.5 text-[10px] font-medium transition-colors cursor-pointer
					{appState.sourceFormat === 'rexc' ? 'bg-white text-black' : 'text-[#888] hover:text-white'}"
			>REXC</button>
			<button
				onclick={toggleFormat}
				class="px-2 py-0.5 text-[10px] font-medium transition-colors cursor-pointer
					{appState.sourceFormat === 'json' ? 'bg-white text-black' : 'text-[#888] hover:text-white'}"
			>JSON</button>
		</div>
		{#if appState.error}
			<span class="text-xs text-[#f48771] truncate">{appState.error}</span>
		{/if}
		{#if appState.converting}
			<span class="text-xs text-[#555] italic">converting...</span>
		{/if}
	</div>

	<!-- Editor -->
	<div class="flex-1 min-h-0 relative">
		{#if appState.sourceFormat === 'json'}
			<textarea
				value={localJson}
				oninput={handleJsonInput}
				class="w-full h-full resize-none bg-transparent text-[#ccc] px-4 py-3 outline-none font-[var(--font-mono)] text-sm"
				spellcheck="false"
				autocomplete="off"
				autocapitalize="off"
				placeholder="Paste JSON here..."
			></textarea>
		{:else}
			<!-- svelte-ignore a11y_autofocus -->
			<textarea
				value={localRexc}
				oninput={handleRexcInput}
				onpaste={handlePaste}
				class="w-full h-full resize-none bg-transparent text-[#ccc] px-4 py-3 outline-none font-[var(--font-mono)] text-sm"
				spellcheck="false"
				autocomplete="off"
				autocapitalize="off"
				placeholder="Paste REXC or JSON here..."
			></textarea>
		{/if}
	</div>

	<StatsPanel />
</div>

<script lang="ts">
	import { appState } from '../lib/state.svelte'
	import { parseRefs } from '../lib/refs.ts'

	let validationError = $state<string | null>(null)
	let localRefs = $state(appState.refsText)

	function handleInput(e: Event) {
		const value = (e.target as HTMLTextAreaElement).value
		localRefs = value
		appState.setRefs(value)
		try {
			parseRefs(value)
			validationError = null
		} catch (e: any) {
			validationError = e.message
		}
	}

	// Sync external changes (e.g. tab switch) into local
	$effect(() => {
		localRefs = appState.refsText
	})
</script>

<div class="h-full flex flex-col bg-[#0a0a0a]">
	<div class="flex items-center gap-3 px-4 py-2 border-b border-[#222] text-xs">
		<label class="flex items-center gap-2 cursor-pointer">
			<input
				type="checkbox"
				checked={appState.refsEnabled}
				onchange={(e) => { appState.refsEnabled = (e.target as HTMLInputElement).checked }}
				class="accent-[#4ec9b0]"
			/>
			<span class="text-[#888]">Enable refs</span>
		</label>
		{#if validationError}
			<span class="text-[#f48771]">{validationError}</span>
		{:else}
			<span class="text-[#444]">{Object.keys(appState.refs).length} keys</span>
		{/if}
	</div>
	<div class="flex-1 min-h-0">
		<textarea
			value={localRefs}
			oninput={handleInput}
			class="w-full h-full resize-none bg-transparent text-[#ccc] px-4 py-3 outline-none font-[var(--font-mono)] text-sm"
			spellcheck="false"
			autocomplete="off"
			autocapitalize="off"
			placeholder="Paste refs JSON here..."
		></textarea>
	</div>
</div>

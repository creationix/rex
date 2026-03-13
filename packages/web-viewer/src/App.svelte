<script lang="ts">
	import Header from './components/Header.svelte'
	import Sidebar from './components/Sidebar.svelte'
	import RexcView from './components/RexcView.svelte'
	import InspectView from './components/InspectView.svelte'
	import JsonView from './components/JsonView.svelte'
	import RefsView from './components/RefsView.svelte'
	import { appState } from './lib/state.svelte'
	import { docStore } from './lib/docs.svelte'

	docStore.init()

	// Auto-save on content changes (debounced)
	let saveTimer: ReturnType<typeof setTimeout> | null = null
	$effect(() => {
		// Touch reactive deps
		appState.rexcText
		appState.jsonText
		// Debounced auto-save for saved documents
		if (saveTimer) clearTimeout(saveTimer)
		saveTimer = setTimeout(() => docStore.autoSave(), 2000)
	})
</script>

<div class="flex flex-col h-screen">
	<Header />
	<div class="flex flex-1 min-h-0">
		<Sidebar />
		<main class="flex-1 min-w-0 relative">
			{#if appState.mode === 'rexc'}
				<RexcView />
			{:else if appState.mode === 'inspect'}
				<InspectView />
			{:else if appState.mode === 'json'}
				<JsonView />
			{:else if appState.mode === 'refs'}
				<RefsView />
			{/if}
			{#if appState.error}
				<div class="absolute bottom-0 left-0 right-0 px-4 py-2 bg-[#2a1215] border-t border-[#f4877133] text-xs text-[#f48771]">
					{appState.error}
				</div>
			{/if}
		</main>
	</div>
</div>

<script lang="ts">
	import Header from './components/Header.svelte'
	import Sidebar from './components/Sidebar.svelte'
	import EncodingView from './components/EncodingView.svelte'
	import DataView from './components/DataView.svelte'
	import { appState } from './lib/state.svelte'
	import { docStore } from './lib/docs.svelte'

	docStore.init()

	let dragging = $state(false)
	let dataPane = $state<HTMLDivElement | null>(null)
	let encodingPane = $state<HTMLDivElement | null>(null)

	// When activePane changes in split mode, move DOM focus to that pane
	$effect(() => {
		const pane = appState.activePane
		if (appState.mode !== 'split') return
		const target = pane === 'data' ? dataPane : encodingPane
		const focusable = target?.querySelector<HTMLElement>('[tabindex]')
		focusable?.focus()
	})

	function handleDragOver(e: DragEvent) {
		e.preventDefault()
		dragging = true
	}

	function handleDragLeave() { dragging = false }

	function handleDrop(e: DragEvent) {
		e.preventDefault()
		dragging = false
		const file = e.dataTransfer?.files[0]
		if (!file) return
		const reader = new FileReader()
		reader.onload = () => {
			appState.loadFile(file.name, reader.result as string)
			docStore.renameCurrentTab(file.name.replace(/\.[^.]+$/, ''))
		}
		reader.readAsText(file)
	}

	// Auto-save on content changes (debounced)
	let saveTimer: ReturnType<typeof setTimeout> | null = null
	$effect(() => {
		appState.rexcText
		if (saveTimer) clearTimeout(saveTimer)
		saveTimer = setTimeout(() => docStore.autoSave(), 2000)
	})
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex flex-col h-screen" ondragover={handleDragOver} ondragleave={handleDragLeave} ondrop={handleDrop}>
	<Header />
	<div class="flex flex-1 min-h-0">
		<div class="{appState.mode === 'split' ? 'hidden xl:block' : ''}">
			<Sidebar />
		</div>
		<main class="flex-1 min-w-0 relative {appState.mode === 'split' ? 'flex' : ''}">
			{#if appState.mode === 'encoding'}
				<EncodingView />
			{:else if appState.mode === 'data'}
				<DataView />
			{:else if appState.mode === 'split'}
				<div bind:this={dataPane} class="flex-1 min-w-0">
					<DataView />
				</div>
				<div class="w-px bg-[#333]"></div>
				<div bind:this={encodingPane} class="flex-1 min-w-0">
					<EncodingView />
				</div>
			{/if}
			{#if appState.error}
				<div class="absolute bottom-0 left-0 right-0 px-4 py-2 bg-[#2a1215] border-t border-[#f4877133] text-xs text-[#f48771]">
					{appState.error}
				</div>
			{/if}
		</main>
		{#if dragging}
			<div class="absolute inset-0 z-50 flex items-center justify-center bg-[#000c] border-2 border-dashed border-[#555] pointer-events-none">
				<span class="text-lg text-[#888]">Drop file to open</span>
			</div>
		{/if}
	</div>
</div>

<script lang="ts">
import Header from './components/Header.svelte'
import Sidebar from './components/Sidebar.svelte'
import EncodingView from './components/EncodingView.svelte'
import DataTreeView from './components/DataView.svelte'
import PasteModal from './components/PasteModal.svelte'
import { appState } from './lib/state.svelte'
import { docStore } from './lib/docs.svelte'

	docStore.init()

let dragging = $state(false)
let dataPane = $state<HTMLDivElement | null>(null)
let encodingPane = $state<HTMLDivElement | null>(null)
let fileInput = $state<HTMLInputElement | null>(null)
let pasteOpen = $state(false)

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

function openFile() {
	fileInput?.click()
}

function handleFileSelected(e: Event) {
	const file = (e.target as HTMLInputElement).files?.[0]
	if (!file) return
	const reader = new FileReader()
	reader.onload = async () => {
		docStore.newTab()
		appState.loadFile(file.name, reader.result as string)
		const docName = file.name.replace(/\.[^.]+$/, '')
		await docStore.saveCurrentAs(docName)
	}
	reader.readAsText(file)
	;(e.target as HTMLInputElement).value = ''
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
	<input bind:this={fileInput} type="file" accept=".json,.rexc,.rx" class="hidden" onchange={handleFileSelected} />
	<div class="lg:hidden border-b border-[#333] bg-[#111] px-2 py-1.5">
		<div class="flex items-center gap-1 mb-1.5">
			<button
				type="button"
				onclick={openFile}
				class="text-[11px] px-2 py-1 rounded border border-[#333] text-[#aaa] hover:text-white hover:border-[#555]"
			>Open</button>
			<button
				type="button"
				onclick={() => { pasteOpen = true }}
				class="text-[11px] px-2 py-1 rounded border border-[#333] text-[#aaa] hover:text-white hover:border-[#555]"
			>New</button>
		</div>
		<div class="flex gap-1 overflow-x-auto pb-0.5">
			{#each docStore.tabs as tab (tab.id)}
				<button
					type="button"
					onclick={() => docStore.switchTab(tab.id)}
					class="shrink-0 max-w-[12rem] truncate text-[11px] px-2 py-1 rounded border
						{tab.id === docStore.activeId
							? 'border-white text-white bg-[#1a1a1a]'
							: 'border-[#333] text-[#888] bg-[#111] hover:text-white hover:border-[#555]'}"
				>
					{tab.name}{tab.saved ? '' : ' *'}
				</button>
			{:else}
				<span class="text-[11px] text-[#666] px-1">No docs open</span>
			{/each}
		</div>
	</div>
	<div class="flex flex-1 min-h-0">
		<div class="hidden lg:block">
			<Sidebar />
		</div>
		<main class="flex-1 min-w-0 relative {appState.mode === 'split' ? 'flex' : ''}">
			{#if appState.mode === 'encoding'}
				<EncodingView />
			{:else if appState.mode === 'data'}
				<DataTreeView />
			{:else if appState.mode === 'split'}
				<div class="lg:hidden absolute top-0 left-0 right-0 z-20 flex items-center gap-1 border-b border-[#333] bg-[#111] px-2 py-1.5">
					<button
						type="button"
						onclick={() => { appState.activePane = 'data' }}
						class="text-[11px] px-2 py-1 rounded border {appState.activePane === 'data' ? 'border-white text-white' : 'border-[#333] text-[#888]'}"
					>Data Pane</button>
					<button
						type="button"
						onclick={() => { appState.activePane = 'encoding' }}
						class="text-[11px] px-2 py-1 rounded border {appState.activePane === 'encoding' ? 'border-white text-white' : 'border-[#333] text-[#888]'}"
					>Encoding Pane</button>
				</div>
				<div bind:this={dataPane} class="min-w-0 flex-1 {appState.activePane === 'data' ? 'block pt-9 lg:pt-0' : 'hidden lg:block'}">
					<DataTreeView />
				</div>
				<div class="hidden lg:block w-px bg-[#333]"></div>
				<div bind:this={encodingPane} class="min-w-0 flex-1 {appState.activePane === 'encoding' ? 'block pt-9 lg:pt-0' : 'hidden lg:block'}">
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

<PasteModal bind:open={pasteOpen} />

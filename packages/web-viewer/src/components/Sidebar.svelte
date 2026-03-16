<script lang="ts">
	import { appState } from '../lib/state.svelte'
	import { docStore } from '../lib/docs.svelte'
	import PasteModal from './PasteModal.svelte'

	let fileInput: HTMLInputElement
	let pasteOpen = $state(false)
	let renaming = $state<string | null>(null)
	let renameValue = $state('')
	let savePromptId = $state<string | null>(null)
	let saveNameValue = $state('')
	let saveInput = $state<HTMLInputElement | null>(null)

	$effect(() => {
		if (savePromptId && saveInput) saveInput.focus()
	})

	function startSave(id: string) {
		const tab = docStore.tabs.find(t => t.id === id)
		if (tab?.saved) {
			docStore.saveCurrent()
		} else {
			savePromptId = id
			saveNameValue = tab?.name === 'untitled' ? '' : (tab?.name ?? '')
		}
	}

	function confirmSave() {
		const name = saveNameValue.trim()
		if (!name) return
		docStore.saveCurrentAs(name)
		savePromptId = null
	}

	function startRename(id: string) {
		const tab = docStore.tabs.find(t => t.id === id)
		if (!tab) return
		renaming = id
		renameValue = tab.name
	}

	function confirmRename() {
		if (!renaming) return
		const name = renameValue.trim()
		if (name) docStore.saveCurrentAs(name)
		renaming = null
	}

	// Cmd/Ctrl+S to save
	function onKeyDown(e: KeyboardEvent) {
		if ((e.metaKey || e.ctrlKey) && e.key === 's') {
			e.preventDefault()
			const tab = docStore.currentTab
			if (tab) startSave(tab.id)
		}
	}

	$effect(() => {
		window.addEventListener('keydown', onKeyDown)
		return () => window.removeEventListener('keydown', onKeyDown)
	})

	function openFile() { fileInput.click() }

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
		fileInput.value = ''
	}
</script>

<aside class="w-48 flex flex-col border-r border-[#333] bg-[#111] overflow-hidden shrink-0">
	<input bind:this={fileInput} type="file" accept=".json,.rexc,.rx" class="hidden" onchange={handleFileSelected} />
	<div class="flex items-center justify-between px-3 py-2 border-b border-[#333]">
		<span class="text-[10px] font-semibold text-[#666] uppercase tracking-wider">Documents</span>
		<div class="flex items-center gap-1">
			<button
				onclick={openFile}
				class="text-[#666] hover:text-white text-[10px] cursor-pointer px-0.5"
				title="Open file"
			>📂</button>
			<button
				onclick={() => { pasteOpen = true }}
				class="text-[#666] hover:text-white text-sm cursor-pointer"
				title="New document (paste)"
			>+</button>
		</div>
	</div>

	<div class="flex-1 overflow-y-auto">
		{#each docStore.tabs as tab (tab.id)}
			<div
				class="group flex items-center px-3 py-1.5 text-xs cursor-pointer border-l-2 transition-colors
					{tab.id === docStore.activeId
						? 'bg-[#1a1a1a] border-white text-white'
						: 'border-transparent text-[#888] hover:text-white hover:bg-[#1a1a1a]'}"
				onclick={() => docStore.switchTab(tab.id)}
				role="button"
				tabindex="0"
				onkeydown={(e) => { if (e.key === 'Enter') docStore.switchTab(tab.id) }}
			>
				{#if renaming === tab.id}
					<input
						type="text"
						bind:value={renameValue}
						onkeydown={(e) => { if (e.key === 'Enter') confirmRename(); if (e.key === 'Escape') renaming = null }}
						onblur={() => confirmRename()}
						class="flex-1 min-w-0 bg-transparent border-b border-[#555] text-xs text-white outline-none"
						onclick={(e) => e.stopPropagation()}
					/>
				{:else}
					<span class="flex-1 min-w-0 truncate">{tab.name}</span>
					{#if !tab.saved}
						<span class="text-[10px] text-[#555] ml-1">*</span>
					{/if}
				{/if}

				{#if tab.id === docStore.activeId && renaming !== tab.id}
					<div class="flex items-center gap-0.5 ml-1 opacity-0 group-hover:opacity-100">
						<button
							onclick={(e) => { e.stopPropagation(); startSave(tab.id) }}
							class="text-[#666] hover:text-white text-[10px] cursor-pointer px-0.5"
							title={tab.saved ? 'Save' : 'Save as...'}
						>{tab.saved ? '💾' : '💾'}</button>
						{#if tab.saved}
							<button
								onclick={(e) => { e.stopPropagation(); startRename(tab.id) }}
								class="text-[#666] hover:text-white text-[10px] cursor-pointer px-0.5"
								title="Rename"
							>✎</button>
						{/if}
						<button
							onclick={(e) => { e.stopPropagation(); docStore.closeTab(tab.id) }}
							class="text-[#666] hover:text-[#f48771] text-[10px] cursor-pointer px-0.5"
							title="Close"
						>✕</button>
					</div>
				{/if}
			</div>
		{:else}
			<div class="px-3 py-3 text-[10px] text-[#555]">
				No documents open.<br/>
				Click <span class="text-[#888]">+</span> to paste or <span class="text-[#888]">📂</span> to open a file.
			</div>
		{/each}
	</div>

	{#if savePromptId}
		<div class="border-t border-[#333] px-3 py-2">
			<label class="text-[10px] text-[#666] uppercase tracking-wider" for="save-name-input">Save as</label>
			<input
				id="save-name-input"
				type="text"
				bind:this={saveInput}
				bind:value={saveNameValue}
				onkeydown={(e) => { if (e.key === 'Enter') confirmSave(); if (e.key === 'Escape') savePromptId = null }}
				placeholder="Document name..."
				class="w-full mt-1 px-2 py-1 text-xs bg-[#0a0a0a] border border-[#333] rounded text-white outline-none focus:border-[#555]"
			/>
			<div class="flex gap-1 mt-1">
				<button
					onclick={confirmSave}
					class="flex-1 text-[10px] px-2 py-0.5 rounded bg-white text-black cursor-pointer hover:bg-[#ddd]"
				>Save</button>
				<button
					onclick={() => savePromptId = null}
					class="flex-1 text-[10px] px-2 py-0.5 rounded border border-[#333] text-[#888] cursor-pointer hover:text-white"
				>Cancel</button>
			</div>
		</div>
	{/if}
</aside>

<PasteModal bind:open={pasteOpen} />

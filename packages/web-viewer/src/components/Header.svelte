<script lang="ts">
	import ModeToggle from './ModeToggle.svelte'
	import { appState } from '../lib/state.svelte'
	import { docStore } from '../lib/docs.svelte'

	let copied = $state(false)
	let fileInput: HTMLInputElement

	function openFile() { fileInput.click() }

	function handleFileSelected(e: Event) {
		const file = (e.target as HTMLInputElement).files?.[0]
		if (!file) return
		const reader = new FileReader()
		reader.onload = () => {
			appState.loadFile(file.name, reader.result as string)
			docStore.renameCurrentTab(file.name.replace(/\.[^.]+$/, ''))
		}
		reader.readAsText(file)
		fileInput.value = ''
	}

	async function copyToClipboard() {
		const text = appState.copyCurrentView()
		if (text) {
			await navigator.clipboard.writeText(text)
			copied = true
			setTimeout(() => { copied = false }, 1500)
		}
	}

	function humanSize(bytes: number) {
		if (bytes < 1024) return bytes + ' B'
		if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KiB'
		return (bytes / (1024 * 1024)).toFixed(2) + ' MiB'
	}

	const sizeLabel = $derived(() => {
		if (appState.mode === 'source' && appState.sourceFormat === 'json') return humanSize(appState.jsonSize)
		return humanSize(appState.rexcSize)
	})
</script>

<header class="flex items-center justify-between px-4 py-2 border-b border-[#333] bg-[#111]">
	<div class="flex items-center gap-4">
		<h1 class="text-sm font-semibold text-white tracking-tight">REXC Viewer</h1>
		{#if docStore.currentTab}
			<span class="text-xs text-[#555]">{docStore.currentTab.name}{docStore.currentTab.saved ? '' : ' *'}</span>
		{/if}
		<ModeToggle />
	</div>
	<div class="flex items-center gap-3">
		{#if sizeLabel()}
			<span class="text-xs text-[#666]">{sizeLabel()}</span>
		{/if}
		<input bind:this={fileInput} type="file" accept=".json,.rexc,.rx" class="hidden" onchange={handleFileSelected} />
		<button
			onclick={openFile}
			class="text-xs px-2.5 py-1 rounded-md border border-[#333] bg-[#111] text-[#888] hover:text-white hover:border-[#555] transition-colors cursor-pointer"
		>
			Open
		</button>
		<button
			onclick={() => { appState.refsOpen = !appState.refsOpen }}
			class="text-xs px-2.5 py-1 rounded-md border transition-colors cursor-pointer
				{appState.refsOpen
					? 'border-[#555] bg-[#1a1a1a] text-white'
					: 'border-[#333] bg-[#111] text-[#888] hover:text-white hover:border-[#555]'}"
		>
			Refs
		</button>
		<button
			onclick={copyToClipboard}
			class="text-xs px-2.5 py-1 rounded-md border border-[#333] bg-[#111] text-[#888] hover:text-white hover:border-[#555] transition-colors cursor-pointer"
		>
			{copied ? 'Copied!' : 'Copy'}
		</button>
	</div>
</header>

<script lang="ts">
	import { appState } from '../lib/state.svelte'

	let textarea = $state<HTMLTextAreaElement | null>(null)
	let wasEmpty = false

	function handleInput(e: Event) {
		const target = e.target as HTMLTextAreaElement
		appState.setRexc(target.value)
	}

	function handlePaste(_e: ClipboardEvent) {
		// Record whether we're pasting into an empty textarea
		// Don't preventDefault — let the browser handle the paste natively
		wasEmpty = !appState.rexcText.trim()
	}

	// After paste completes, let the browser paint the text, then switch
	function handleInputAfterPaste(e: Event) {
		if (!wasEmpty) return
		wasEmpty = false
		const target = e.target as HTMLTextAreaElement
		const text = target.value
		if (text.trim()) {
			appState.setRexc(text)
			requestAnimationFrame(() => appState.switchMode('inspect'))
		}
	}

	$effect(() => {
		if (textarea) textarea.focus()
	})
</script>

<div class="h-full flex flex-col bg-[#0a0a0a]">
	<textarea
		bind:this={textarea}
		value={appState.rexcText}
		oninput={(e) => { handleInputAfterPaste(e); handleInput(e) }}
		onpaste={handlePaste}
		placeholder="Paste or type REXC here..."
		spellcheck="false"
		class="flex-1 w-full resize-none border-none outline-none p-4
			font-[var(--font-mono)] text-sm leading-relaxed
			bg-transparent text-[#ededed] placeholder-[#444]
			focus:outline-none"
	></textarea>
</div>

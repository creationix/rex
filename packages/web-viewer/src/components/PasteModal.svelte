<script lang="ts">
	import { appState } from '../lib/state.svelte'
	import { docStore } from '../lib/docs.svelte'
	import { open as rxOpen, makeKey } from '@creationix/rx'

	let { open = $bindable(false) } = $props()

	let name = $state('')
	let content = $state('')
	let parseError = $state<string | null>(null)
	let nameInput = $state<HTMLInputElement | null>(null)
	let validatedRexc: string | null = null  // cached rexc from validation

	function onKeydown(e: KeyboardEvent) {
		if (open && e.key === 'Escape') {
			e.preventDefault()
			close()
		}
	}

	function validate(text: string): string | null {
		validatedRexc = null
		const trimmed = text.trim()
		if (!trimmed) return null
		// Try as rexc first
		if (appState.isValidRexc(trimmed)) {
			try {
				const buf = new TextEncoder().encode(trimmed)
				const root = rxOpen(buf)
				// Walk up to 10K nodes to verify structure
				makeKey(root, 10_000)
				validatedRexc = trimmed
				return null
			} catch (e: any) {
				return e.message
			}
		}
		// Try as JSON
		try {
			JSON.parse(trimmed)
			return null
		} catch (e: any) {
			return e.message
		}
	}

	function close() {
		open = false
		name = ''
		content = ''
		parseError = null
		validatedRexc = null
	}

	function submit() {
		if (!content.trim()) return
		const err = validate(content)
		if (err) { parseError = err; return }
		docStore.newTab()
		const docName = name.trim() || 'untitled'
		docStore.renameCurrentTab(docName)
		if (validatedRexc) {
			// Already validated as rexc — load directly
			appState.setRexc(validatedRexc)
		} else {
			// JSON input — need to convert to rexc
			appState.setJson(content.trim())
			appState.syncRexc()
		}
		appState.mode = 'data'
		close()
	}

	function handlePaste() {
		requestAnimationFrame(() => {
			parseError = validate(content)
			// Auto-submit if name is set, content is valid, and was pasted
			if (!parseError && name.trim() && content.trim()) {
				submit()
			}
		})
	}

	function handleInput() {
		parseError = content.trim() ? validate(content) : null
	}

	$effect(() => {
		if (open && nameInput) {
			requestAnimationFrame(() => nameInput?.focus())
		}
	})
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-[#000a]" onmousedown={close}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="bg-[#1a1a1a] border border-[#333] rounded-lg shadow-xl p-5 w-[600px] max-w-[90vw] max-h-[80vh] flex flex-col" onmousedown={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between mb-3">
				<h2 class="text-sm font-semibold text-white">New Document</h2>
				<button onclick={close} class="text-[#666] hover:text-white text-lg cursor-pointer">&times;</button>
			</div>

			<input
				bind:this={nameInput}
				bind:value={name}
				type="text"
				placeholder="Document name..."
				onkeydown={(e) => { if (e.key === 'Enter' && content.trim()) submit() }}
				class="w-full px-3 py-1.5 mb-3 text-xs bg-[#0a0a0a] border border-[#333] rounded text-white outline-none focus:border-[#555] font-[var(--font-mono)]"
			/>

			<textarea
				bind:value={content}
				onpaste={handlePaste}
				oninput={handleInput}
				placeholder="Paste REXC or JSON data here..."
				class="flex-1 min-h-[200px] w-full resize-none bg-[#0a0a0a] border border-[#333] rounded text-[#ccc] px-3 py-2 outline-none focus:border-[#555] font-[var(--font-mono)] text-xs {parseError ? 'border-[#f48771]' : ''}"
				spellcheck="false"
				autocomplete="off"
				autocapitalize="off"
			></textarea>

			{#if parseError}
				<div class="mt-1 text-xs text-[#f48771] truncate">{parseError}</div>
			{/if}

			<div class="flex justify-end gap-2 mt-3">
				<button
					onclick={close}
					class="text-xs px-3 py-1.5 rounded-md border border-[#333] text-[#888] hover:text-white hover:border-[#555] transition-colors cursor-pointer"
				>Cancel</button>
				<button
					onclick={submit}
					disabled={!content.trim() || !!parseError}
					class="text-xs px-3 py-1.5 rounded-md bg-white text-black hover:bg-[#ddd] transition-colors cursor-pointer disabled:opacity-30 disabled:cursor-default"
				>Open</button>
			</div>
		</div>
	</div>
{/if}

<script lang="ts">
	import { appState } from '../lib/state.svelte'
	import { docStore } from '../lib/docs.svelte'
	import { open as rxOpen, makeKey } from '@creationix/rx'

	let { open = $bindable(false) } = $props()

	let name = $state('')
	let content = $state('')
	let parseError = $state<string | null>(null)
	let nameInput = $state<HTMLInputElement | null>(null)
	let dialogEl = $state<HTMLDivElement | null>(null)
	let lastFocused = $state<HTMLElement | null>(null)
	let bgState: Array<{ el: HTMLElement; ariaHidden: string | null; inert: boolean }> = []
	let validatedRexc: string | null = null  // cached rexc from validation

	function getFocusable(container: HTMLElement | null): HTMLElement[] {
		if (!container) return []
		const nodes = container.querySelectorAll<HTMLElement>(
			'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
		)
		return Array.from(nodes).filter(el => !el.hasAttribute('disabled') && el.tabIndex >= 0)
	}

	function trapTab(e: KeyboardEvent) {
		if (!open || e.key !== 'Tab') return
		const items = getFocusable(dialogEl)
		if (items.length === 0) {
			e.preventDefault()
			dialogEl?.focus()
			return
		}
		const first = items[0]
		const last = items[items.length - 1]
		const active = document.activeElement
		if (e.shiftKey && active === first) {
			e.preventDefault()
			last.focus()
		} else if (!e.shiftKey && active === last) {
			e.preventDefault()
			first.focus()
		}
	}

	function setBackgroundInert(active: boolean) {
		if (active) {
			const regions = Array.from(document.querySelectorAll<HTMLElement>('header, aside, main'))
			bgState = regions.map(el => ({
				el,
				ariaHidden: el.getAttribute('aria-hidden'),
				inert: el.inert,
			}))
			for (const { el } of bgState) {
				el.setAttribute('aria-hidden', 'true')
				el.inert = true
			}
			return
		}
		for (const item of bgState) {
			if (item.ariaHidden == null) item.el.removeAttribute('aria-hidden')
			else item.el.setAttribute('aria-hidden', item.ariaHidden)
			item.el.inert = item.inert
		}
		bgState = []
	}

	function onKeydown(e: KeyboardEvent) {
		trapTab(e)
		if (open && e.key === 'Escape') {
			e.preventDefault()
			close()
		}
	}

	function validate(text: string): string | null {
		validatedRexc = null
		const trimmed = text.trim()
		if (!trimmed) return null
		// Try as JSON first to avoid false-positive REXC checks on JSON text.
		try {
			JSON.parse(trimmed)
			return null
		} catch {
			// Not JSON, continue to REXC validation.
		}
		// Try as rexc
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
		// Return JSON parse error for user-friendly feedback.
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

	async function submit() {
		if (!content.trim()) return
		const err = validate(content)
		if (err) { parseError = err; return }
		docStore.newTab()
		const docName = name.trim() || 'untitled'
		if (validatedRexc) {
			appState.setRexc(validatedRexc)
		} else {
			appState.setJson(content.trim())
			await appState.syncRexc()
		}
		appState.mode = 'data'
		await docStore.saveCurrentAs(docName)
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

	function handleNameInput(e: Event) {
		const target = e.currentTarget as HTMLInputElement | null
		if (target) name = target.value
	}

	function handleContentInput(e: Event) {
		const target = e.currentTarget as HTMLTextAreaElement | null
		if (target) content = target.value
		handleInput()
	}

	$effect(() => {
		if (!open) return
		lastFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null
		setBackgroundInert(true)
		if (nameInput) {
			requestAnimationFrame(() => nameInput?.focus())
		}
		return () => {
			setBackgroundInert(false)
			lastFocused?.focus()
		}
	})
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-[#000a]" onmousedown={close}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			bind:this={dialogEl}
			class="bg-[#1a1a1a] border border-[#333] rounded-lg shadow-xl p-5 w-[600px] max-w-[90vw] max-h-[80vh] flex flex-col"
			onmousedown={(e) => e.stopPropagation()}
			role="dialog"
			aria-modal="true"
			aria-labelledby="new-doc-title"
			aria-describedby={parseError ? 'new-doc-error' : undefined}
			tabindex="-1"
		>
			<div class="flex items-center justify-between mb-3">
				<h2 id="new-doc-title" class="text-sm font-semibold text-white">New Document</h2>
				<button type="button" aria-label="Close new document dialog" onclick={close} class="text-[#666] hover:text-white text-lg cursor-pointer">&times;</button>
			</div>

			<input
				bind:this={nameInput}
				bind:value={name}
				type="text"
				placeholder="Document name..."
				oninput={handleNameInput}
				onchange={handleNameInput}
				onkeydown={(e) => { if (e.key === 'Enter' && content.trim()) submit() }}
				class="w-full px-3 py-1.5 mb-3 text-xs bg-[#0a0a0a] border border-[#333] rounded text-white outline-none focus:border-[#555] font-[var(--font-mono)]"
			/>

			<textarea
				bind:value={content}
				onpaste={handlePaste}
				oninput={handleContentInput}
				onchange={handleContentInput}
				placeholder="Paste REXC or JSON data here..."
				class="flex-1 min-h-[200px] w-full resize-none bg-[#0a0a0a] border border-[#333] rounded text-[#ccc] px-3 py-2 outline-none focus:border-[#555] font-[var(--font-mono)] text-xs {parseError ? 'border-[#f48771]' : ''}"
				spellcheck="false"
				autocomplete="off"
				autocapitalize="off"
			></textarea>

			{#if parseError}
				<div id="new-doc-error" class="mt-1 text-xs text-[#f48771] truncate">{parseError}</div>
			{/if}

			<div class="flex justify-end gap-2 mt-3">
				<button
					type="button"
					onclick={close}
					class="text-xs px-3 py-1.5 rounded-md border border-[#333] text-[#888] hover:text-white hover:border-[#555] transition-colors cursor-pointer"
				>Cancel</button>
				<button
					type="button"
					onclick={submit}
					disabled={!content.trim() || !!parseError}
					class="text-xs px-3 py-1.5 rounded-md bg-white text-black hover:bg-[#ddd] transition-colors cursor-pointer disabled:opacity-30 disabled:cursor-default"
				>Open</button>
			</div>
		</div>
	</div>
{/if}

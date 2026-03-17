<script lang="ts">
	import KeyboardShortcuts from './KeyboardShortcuts.svelte'

	let { open = $bindable(false) } = $props()
	let dialogEl = $state<HTMLDivElement | null>(null)
	let lastFocused = $state<HTMLElement | null>(null)
	let bgState: Array<{ el: HTMLElement; ariaHidden: string | null; inert: boolean }> = []

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
			open = false
		}
	}

	$effect(() => {
		if (!open) return
		lastFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null
		setBackgroundInert(true)
		requestAnimationFrame(() => {
			const items = getFocusable(dialogEl)
			;(items[0] ?? dialogEl)?.focus()
		})
		return () => {
			setBackgroundInert(false)
			lastFocused?.focus()
		}
	})
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-[#000a]" onmousedown={() => open = false}>
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			bind:this={dialogEl}
			class="bg-[#1a1a1a] border border-[#333] rounded-lg shadow-xl p-6 max-w-lg w-full mx-4"
			onmousedown={(e) => e.stopPropagation()}
			role="dialog"
			aria-modal="true"
			aria-labelledby="help-dialog-title"
			tabindex="-1"
		>
			<div class="flex items-center justify-between mb-4">
				<h2 id="help-dialog-title" class="text-sm font-semibold text-white">Keyboard Shortcuts</h2>
				<button type="button" aria-label="Close help" onclick={() => open = false} class="text-[#666] hover:text-white text-lg cursor-pointer">&times;</button>
			</div>

			<KeyboardShortcuts />

			<div class="mt-4 text-[11px] text-[#555]">Press Escape or click outside to close</div>
		</div>
	</div>
{/if}

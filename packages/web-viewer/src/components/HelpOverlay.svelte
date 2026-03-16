<script lang="ts">
	import KeyboardShortcuts from './KeyboardShortcuts.svelte'

	let { open = $bindable(false) } = $props()

	function onKeydown(e: KeyboardEvent) {
		if (open && e.key === 'Escape') {
			e.preventDefault()
			open = false
		}
	}
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-[#000a]" onmousedown={() => open = false}>
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="bg-[#1a1a1a] border border-[#333] rounded-lg shadow-xl p-6 max-w-lg w-full mx-4" onmousedown={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between mb-4">
				<h2 class="text-sm font-semibold text-white">Keyboard Shortcuts</h2>
				<button onclick={() => open = false} class="text-[#666] hover:text-white text-lg cursor-pointer">&times;</button>
			</div>

			<KeyboardShortcuts />

			<div class="mt-4 text-[11px] text-[#555]">Press Escape or click outside to close</div>
		</div>
	</div>
{/if}

<script lang="ts">
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

			<div class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-[13px]">
				<span class="text-[#666] text-right">Navigation</span>
				<span></span>

				<kbd class="text-[#9cdcfe] text-right">Up / Down</kbd>
				<span class="text-[#ccc]">Move focus</span>

				<kbd class="text-[#9cdcfe] text-right">PageUp / PageDown</kbd>
				<span class="text-[#ccc]">Move focus by page</span>

				<kbd class="text-[#9cdcfe] text-right">Home / End</kbd>
				<span class="text-[#ccc]">Jump to first / last row</span>

				<span class="text-[#666] text-right mt-2">Tree</span>
				<span></span>

				<kbd class="text-[#9cdcfe] text-right">Right</kbd>
				<span class="text-[#ccc]">Expand or enter child</span>

				<kbd class="text-[#9cdcfe] text-right">Left</kbd>
				<span class="text-[#ccc]">Collapse or go to parent</span>

				<kbd class="text-[#9cdcfe] text-right">Enter</kbd>
				<span class="text-[#ccc]">Toggle expand / follow pointer</span>

				<span class="text-[#666] text-right mt-2">History</span>
				<span></span>

				<kbd class="text-[#9cdcfe] text-right">Backspace</kbd>
				<span class="text-[#ccc]">Go back</span>

				<span class="text-[#666] text-right mt-2">Split View</span>
				<span></span>

				<kbd class="text-[#9cdcfe] text-right">Tab</kbd>
				<span class="text-[#ccc]">Switch active pane</span>

				<span class="text-[#666] text-right mt-2">Context Menu</span>
				<span></span>

				<kbd class="text-[#9cdcfe] text-right">Right-click</kbd>
				<span class="text-[#ccc]">Copy as JSON / REXC, extract as document</span>
			</div>

			<div class="mt-4 text-[11px] text-[#555]">Press Escape or click outside to close</div>
		</div>
	</div>
{/if}

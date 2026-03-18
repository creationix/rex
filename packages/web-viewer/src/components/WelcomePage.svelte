<script lang="ts">
	import KeyboardShortcuts from './KeyboardShortcuts.svelte'
</script>

<div class="h-full overflow-auto bg-bg-deep p-6">
	<div class="flex flex-col lg:flex-row gap-8">
		<div class="flex-1 min-w-0">
			<h1 class="text-lg font-semibold text-text-bright mb-2">RX Viewer</h1>
			<p class="text-sm text-text-dim mb-6">
				A viewer and inspector for REXC-encoded data. Browse decoded values in the
				<span class="text-text">Data</span> view, examine the raw encoding in the
				<span class="text-text">Encoding</span> view, or use
				<span class="text-text">Split</span> to see both side by side.
			</p>

			<h2 class="text-xs font-semibold text-text-muted uppercase tracking-wider mb-2">Getting Started</h2>
			<div class="flex flex-col gap-2 text-sm text-text-dim">
				<div class="flex items-center gap-2">
					<span class="text-text-bright">+</span>
					<span>Paste RX or JSON data as a new document</span>
				</div>
				<div class="flex items-center gap-2">
					<span>📂</span>
					<span>Open a file from your computer</span>
				</div>
				<div class="flex items-center gap-2">
					<span class="text-text-muted">drop</span>
					<span>Drag and drop a file anywhere</span>
				</div>
			</div>
		</div>

		<div class="flex-1 min-w-0">
			<h2 class="text-xs font-semibold text-text-muted uppercase tracking-wider mb-2">Keyboard Shortcuts</h2>
			<KeyboardShortcuts />
		</div>
	</div>
</div>

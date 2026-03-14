<script lang="ts">
	import { untrack } from 'svelte'
	import { EditorView, basicSetup } from 'codemirror'
	import { json as jsonLang } from '@codemirror/lang-json'
	import { oneDark } from '@codemirror/theme-one-dark'

	interface Props {
		value: string
		onchange?: (value: string) => void
		readonly?: boolean
	}

	let { value, onchange, readonly = false }: Props = $props()

	let container: HTMLDivElement
	let editor: EditorView | null = null
	let internalUpdate = false

	// Track the last doc string we know came from user input, so we
	// don't round-trip it back into the editor (which steals focus).
	let lastUserDoc: string | null = null

	$effect(() => {
		if (!container) return

		// Use untrack so this effect only depends on `container`, not on
		// value/onchange/readonly — those are handled by the sync effect below.
		const initialDoc = untrack(() => value)
		const isReadonly = untrack(() => readonly)

		editor = new EditorView({
			parent: container,
			doc: initialDoc,
			extensions: [
				basicSetup,
				jsonLang(),
				oneDark,
				EditorView.lineWrapping,
				...(isReadonly ? [EditorView.editable.of(false)] : []),
				EditorView.updateListener.of(update => {
					if (update.docChanged && !internalUpdate) {
						lastUserDoc = update.state.doc.toString()
						// Read onchange at call time so we always get the current prop
						onchange?.(lastUserDoc)
					}
				}),
			],
		})

		return () => {
			editor?.destroy()
			editor = null
		}
	})

	// Sync external value changes into the editor
	$effect(() => {
		if (!editor) return
		// Skip if this value originated from user typing in this editor
		if (value === lastUserDoc) return
		if (editor.state.doc.toString() !== value) {
			internalUpdate = true
			editor.dispatch({
				changes: { from: 0, to: editor.state.doc.length, insert: value }
			})
			internalUpdate = false
		}
	})
</script>

<div bind:this={container} class="h-full [&_.cm-editor]:h-full [&_.cm-editor]:outline-none [&_.cm-editor.cm-focused]:outline-none [&_.cm-scroller]:font-[var(--font-mono)] [&_.cm-scroller]:text-[13px] [&_.cm-scroller]:leading-relaxed"></div>

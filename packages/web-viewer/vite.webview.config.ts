import { defineConfig, type Plugin } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'
import { resolve } from 'node:path'

/**
 * Replace imports of './worker' (and '../lib/worker') with './webview-worker'
 * so the webview build uses the try-catch-wrapped variant that gracefully
 * handles Worker unavailability in VS Code webviews.
 */
function webviewWorkerPlugin(): Plugin {
	return {
		name: 'webview-worker-alias',
		enforce: 'pre',
		resolveId(source, importer) {
			if (importer && /worker(?:\.ts)?$/.test(source) && !source.includes('webview-worker') && !source.includes('decode-worker')) {
				return this.resolve(source.replace(/worker(\.ts)?/, 'webview-worker$1'), importer, { skipSelf: true })
			}
		},
	}
}

export default defineConfig({
	plugins: [webviewWorkerPlugin(), svelte(), tailwindcss()],
	build: {
		outDir: resolve(__dirname, '../vscode-rex/dist/webview'),
		emptyOutDir: true,
		cssCodeSplit: false,
		rollupOptions: {
			input: resolve(__dirname, 'src/webview-main.ts'),
			output: {
				format: 'iife',
				entryFileNames: 'webview.js',
				assetFileNames: 'webview.[ext]',
			},
		},
	},
	worker: {
		format: 'es',
	},
})

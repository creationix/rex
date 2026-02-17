import { fileURLToPath } from "node:url";
import { compile } from "./rex.ts";

type DocCheckResult = {
	filePath: string;
	totalBlocks: number;
	compiledBlocks: number;
	skippedBlocks: number;
	failures: Array<{
		blockIndex: number;
		line: number;
		language: string;
		preview: string;
		error: string;
	}>;
};

const FENCE_RE = /```([^\n`]*)\n([\s\S]*?)```/g;
const COMPILE_LANGS = new Set(["rex", "rex-infix"]);

function toPosixPath(path: string): string {
	return path.replaceAll("\\\\", "/");
}

function lineOfOffset(text: string, offset: number): number {
	let line = 1;
	for (let index = 0; index < offset; index += 1) {
		if (text[index] === "\n") line += 1;
	}
	return line;
}

function previewSnippet(value: string): string {
	return value
		.trim()
		.split(/\r?\n/)
		.slice(0, 2)
		.join(" ")
		.slice(0, 120);
}

async function checkFile(filePath: string): Promise<DocCheckResult> {
	const content = await Bun.file(filePath).text();
	const failures: DocCheckResult["failures"] = [];
	let totalBlocks = 0;
	let compiledBlocks = 0;
	let skippedBlocks = 0;

	for (const match of content.matchAll(FENCE_RE)) {
		totalBlocks += 1;
		const language = (match[1] ?? "").trim().toLowerCase().split(/\s+/)[0] ?? "";
		const body = match[2] ?? "";
		if (!COMPILE_LANGS.has(language)) {
			skippedBlocks += 1;
			continue;
		}

		compiledBlocks += 1;
		const line = lineOfOffset(content, match.index ?? 0) + 1;
		try {
			compile(body);
		}
		catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			failures.push({
				blockIndex: totalBlocks,
				line,
				language,
				preview: previewSnippet(body),
				error: message,
			});
		}
	}

	return {
		filePath,
		totalBlocks,
		compiledBlocks,
		skippedBlocks,
		failures,
	};
}

async function main() {
	const args = Bun.argv.slice(2);
	const scriptDir = new URL(".", import.meta.url);
	const defaultHighLevel = fileURLToPath(new URL("../../high-level.md", scriptDir));
	const defaultEncoding = fileURLToPath(new URL("../../encoding.md", scriptDir));
	const files = args.length > 0
		? args.map(toPosixPath)
		: [toPosixPath(defaultHighLevel), toPosixPath(defaultEncoding)];

	const results: DocCheckResult[] = [];
	for (const filePath of files) {
		if (!(await Bun.file(filePath).exists())) {
			console.error(`Missing file: ${filePath}`);
			process.exitCode = 1;
			continue;
		}
		results.push(await checkFile(filePath));
	}

	let failedCount = 0;
	for (const result of results) {
		console.log(`\n${result.filePath}`);
		console.log(`  fenced blocks: ${result.totalBlocks}`);
		console.log(`  rex/rex-infix checked: ${result.compiledBlocks}`);
		console.log(`  skipped (other langs): ${result.skippedBlocks}`);
		console.log(`  failures: ${result.failures.length}`);
		for (const failure of result.failures) {
			failedCount += 1;
			console.log(`\n  [block ${failure.blockIndex}] line ${failure.line} (${failure.language})`);
			console.log(`    preview: ${failure.preview}`);
			console.log(`    ${failure.error.replaceAll("\n", "\n    ")}`);
		}
	}

	if (failedCount > 0) {
		console.error(`\nDoc example verification failed: ${failedCount} block(s) did not compile.`);
		process.exit(1);
	}

	console.log("\nDoc example verification passed.");
}

await main();

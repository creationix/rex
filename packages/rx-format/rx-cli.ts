import { stringify as rexcStringify, type RexCStringifyOptions } from "@creationix/rex/rexc";
import { stringify as rexStringify } from "@creationix/rex";
import {
	setColorEnabled,
	highlightLine,
	highlightJSON,
	highlightRexc,
} from "@creationix/rex/rex-repl";
import { open } from "./rx";
import { readFile, writeFile, mkdir, unlink, lstat } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

const textEncoder = new TextEncoder();

// ── Types ────────────────────────────────────────────────────

type Format = "json" | "rexc";
type OutputFormat = Format | "tree";

type RxOptions = {
	files: string[];
	fromFormat?: Format;
	toFormat?: OutputFormat;
	select?: string[];
	out?: string;
	indexes?: number | false;
	color: boolean;
	colorExplicit: boolean;
	help: boolean;
};

// ── Arg parsing ──────────────────────────────────────────────

function parseArgs(argv: string[]): RxOptions {
	const opts: RxOptions = {
		files: [],
		color: process.stdout.isTTY ?? false,
		colorExplicit: false,
		help: false,
	};
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i]!;
		if (arg === "-h" || arg === "--help") { opts.help = true; continue; }
		if (arg === "--color") { opts.color = true; opts.colorExplicit = true; continue; }
		if (arg === "--no-color") { opts.color = false; opts.colorExplicit = true; continue; }
		if (arg === "-j" || arg === "--json") { opts.toFormat = "json"; continue; }
		if (arg === "-r" || arg === "--rexc") { opts.toFormat = "rexc"; continue; }
		if (arg === "-t" || arg === "--tree") { opts.toFormat = "tree"; continue; }
		if (arg === "--from") {
			const v = argv[++i];
			if (v !== "json" && v !== "rexc") throw new Error("--from must be 'json' or 'rexc'");
			opts.fromFormat = v;
			continue;
		}
		if (arg === "--to") {
			const v = argv[++i];
			if (v !== "json" && v !== "rexc" && v !== "tree") throw new Error("--to must be 'json', 'rexc', or 'tree'");
			opts.toFormat = v;
			continue;
		}
		if (arg === "-s" || arg === "--select") {
			// Everything after -s until end-of-args or next flag is a selector segment
			const segments: string[] = [];
			while (i + 1 < argv.length && !argv[i + 1]!.startsWith("-")) {
				segments.push(argv[++i]!);
			}
			if (segments.length === 0) throw new Error("Missing value for --select");
			opts.select = segments;
			continue;
		}
		if (arg === "-o" || arg === "--out") {
			const v = argv[++i];
			if (!v) throw new Error("Missing value for --out");
			opts.out = v;
			continue;
		}
		if (arg === "--indexes") {
			const v = argv[++i];
			if (v === undefined) throw new Error("Missing value for --indexes");
			if (v === "false" || v === "off" || v === "no") { opts.indexes = false; continue; }
			const n = parseInt(v, 10);
			if (Number.isNaN(n) || n < 0) throw new Error("--indexes must be a non-negative integer or 'false'");
			opts.indexes = n;
			continue;
		}
		if (!arg.startsWith("-") || arg === "-") {
			opts.files.push(arg);
			continue;
		}
		throw new Error(`Unknown option: ${arg}`);
	}
	return opts;
}

function usage(): string {
	return [
		"rx — inspect, convert, and filter REXC & JSON data.",
		"",
		"Usage:",
		"  rx data.rexc                   Pretty-print rexc as a tree",
		"  rx data.rexc --to json         Convert rexc to JSON",
		"  rx data.json --to rexc         Convert JSON to rexc",
		"  cat data.rexc | rx             Read from stdin (auto-detect)",
		"  rx data.rexc -s routes 0 op    Select a sub-value",
		"",
		"Input:",
		"  <file>              Read from file (format auto-detected by extension)",
		"  -                   Read from stdin explicitly",
		"  (no args, piped)    Read from stdin automatically",
		"",
		"Format control:",
		"  --from json|rexc    Force input format (default: auto-detect)",
		"  --to json|rexc|tree Output format",
		"  -j, --json          Shortcut for --to json",
		"  -r, --rexc          Shortcut for --to rexc",
		"  -t, --tree          Shortcut for --to tree",
		"",
		"  Default output: tree on TTY, json when piped.",
		"",
		"Encoding:",
		"  --indexes <n>       Add indexes to containers with >= n entries",
		"                      Use 'false' to disable indexes entirely",
		"",
		"Filtering:",
		"  -s, --select <seg>  Space-delimited selector segments (e.g. -s foo bar 0 baz)",
		"",
		"Output:",
		"  -o, --out <path>    Write to file instead of stdout",
		"  --color             Force ANSI color",
		"  --no-color          Disable ANSI color",
		"  -h, --help          Show this message",
		"",
		"Shell completions:",
		"  rx --completions setup [zsh|bash]  Install tab completions",
		"  rx --completions zsh|bash          Print completion script to stdout",
	].join("\n");
}

// ── Format detection ─────────────────────────────────────────

function formatFromExt(path: string): Format | undefined {
	if (path.endsWith(".json")) return "json";
	if (path.endsWith(".rexc")) return "rexc";
	return undefined;
}

function detectFormat(content: string): Format {
	const t = content.trimStart();
	if (/^[\[{"0-9tfn\-]/.test(t)) {
		try { JSON.parse(content); return "json"; } catch { /* not json */ }
	}
	return "rexc";
}

// ── Input reading ────────────────────────────────────────────

async function readStdin(): Promise<string> {
	const chunks: Buffer[] = [];
	for await (const chunk of process.stdin) {
		chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
	}
	return Buffer.concat(chunks).toString("utf8");
}

type ParsedInput = { value: unknown };

function parseRaw(raw: string, format: Format): unknown {
	if (format === "json") return JSON.parse(raw);
	return open(textEncoder.encode(raw.trim()));
}

async function readInput(opts: RxOptions): Promise<ParsedInput> {
	if (opts.files.length === 0) {
		// stdin
		if (process.stdin.isTTY) {
			const c = opts.color;
			const bold = c ? "\x1b[1m" : "";
			const dim = c ? "\x1b[2m" : "";
			const cyan = c ? "\x1b[36m" : "";
			const reset = c ? "\x1b[0m" : "";
			process.stderr.write([
				`${bold}rx${reset} — inspect, convert, and filter REXC & JSON data.`,
				"",
				`${dim}Usage:${reset}`,
				`  ${cyan}rx${reset} ${dim}<file>${reset}               Pretty-print as a tree`,
				`  ${cyan}rx${reset} ${dim}<file>${reset} ${cyan}--to json${reset}     Convert to JSON`,
				`  cat data.rexc | ${cyan}rx${reset}      Read from stdin`,
				`  ${cyan}rx${reset} ${dim}<file>${reset} ${cyan}-s${reset} ${dim}key 0 sub${reset}   Select a sub-value`,
				"",
				`Run ${cyan}rx --help${reset} for full usage.`,
				"",
			].join("\n"));
			process.exit(1);
		}
		const raw = await readStdin();
		if (!raw.trim()) throw new Error("Empty stdin.");
		const format = opts.fromFormat ?? detectFormat(raw);
		return { value: parseRaw(raw, format) };
	}

	if (opts.files.length === 1) {
		const file = opts.files[0]!;
		const raw = file === "-" ? await readStdin() : await readFile(file, "utf8");
		const format = opts.fromFormat ?? (file === "-" ? detectFormat(raw) : formatFromExt(file) ?? detectFormat(raw));
		return { value: parseRaw(raw, format) };
	}

	// Multiple files → array
	const values: unknown[] = [];
	for (const file of opts.files) {
		const raw = file === "-" ? await readStdin() : await readFile(file, "utf8");
		const format = opts.fromFormat ?? (file === "-" ? detectFormat(raw) : formatFromExt(file) ?? detectFormat(raw));
		values.push(parseRaw(raw, format));
	}
	return { value: values };
}

// ── Selector ─────────────────────────────────────────────────

function applySelector(value: unknown, segments: string[]): unknown {
	let current = value;
	let path = "";
	for (const seg of segments) {
		const asIndex = /^\d+$/.test(seg) ? parseInt(seg, 10) : undefined;
		if (Array.isArray(current) && asIndex !== undefined) {
			path += `[${asIndex}]`;
			if (asIndex < 0 || asIndex >= current.length) {
				throw new Error(`Selector ${path}: index ${asIndex} out of range (length ${current.length})`);
			}
			current = current[asIndex];
		} else if (current !== null && current !== undefined && typeof current === "object" && !Array.isArray(current)) {
			path += ` ${seg}`;
			const obj = current as Record<string, unknown>;
			if (!(seg in obj)) {
				throw new Error(`Selector${path}: property '${seg}' not found`);
			}
			current = obj[seg];
		} else {
			path += ` ${seg}`;
			throw new Error(`Selector${path}: cannot access '${seg}' on ${typeLabel(current)}`);
		}
	}
	return current;
}

function typeLabel(v: unknown): string {
	if (v === null) return "null";
	if (v === undefined) return "undefined";
	if (Array.isArray(v)) return "array";
	return typeof v;
}

// ── Completions ──────────────────────────────────────────────

function walkSegments(value: unknown, segments: string[]): unknown {
	let current = value;
	for (const seg of segments) {
		const asIndex = /^\d+$/.test(seg) ? parseInt(seg, 10) : undefined;
		if (Array.isArray(current) && asIndex !== undefined) {
			if (asIndex < 0 || asIndex >= current.length) return undefined;
			current = current[asIndex];
		} else if (current !== null && current !== undefined && typeof current === "object" && !Array.isArray(current)) {
			const obj = current as Record<string, unknown>;
			if (!(seg in obj)) return undefined;
			current = obj[seg];
		} else {
			return undefined;
		}
	}
	return current;
}

const MAX_COMPLETIONS = 50;

function collapseCompletions(matches: string[], partial: string): string[] {
	if (matches.length <= MAX_COMPLETIONS) return matches;
	// Sort once, then binary-search for the right prefix length.
	// Sorted matches means we only need to compare adjacent entries
	// to count distinct prefixes at a given length.
	matches.sort();
	const maxLen = matches[matches.length - 1]!.length;
	// Count distinct prefixes at a given length
	function distinctAt(len: number): number {
		let count = 1;
		for (let i = 1; i < matches.length; i++) {
			const a = matches[i - 1]!, b = matches[i]!;
			// Compare only up to len chars — since sorted, first diff means new prefix
			let same = a.length >= len && b.length >= len;
			if (same) {
				for (let j = 0; j < len; j++) {
					if (a.charCodeAt(j) !== b.charCodeAt(j)) { same = false; break; }
				}
			} else {
				// Different lengths under len — check the shorter portion
				const end = Math.min(a.length, b.length);
				for (let j = 0; j < end; j++) {
					if (a.charCodeAt(j) !== b.charCodeAt(j)) { same = false; break; }
				}
				if (same) same = a.length === b.length;
			}
			if (!same) count++;
		}
		return count;
	}
	// Binary search for the longest prefix length where distinct <= MAX.
	// As len increases, distinctAt(len) increases (more granular grouping).
	// We want the largest len where it's still <= MAX.
	let lo = partial.length + 1;
	let hi = maxLen;
	while (lo < hi) {
		const mid = (lo + hi + 1) >>> 1;
		if (distinctAt(mid) <= MAX_COMPLETIONS) lo = mid;
		else hi = mid - 1;
	}
	// Collect unique prefixes at this length
	const result: string[] = [matches[0]!.slice(0, lo)];
	for (let i = 1; i < matches.length; i++) {
		const p = matches[i]!.slice(0, lo);
		if (p !== result[result.length - 1]) result.push(p);
	}
	return result;
}

function generateCompletions(value: unknown, segments: string[], partial: string): string[] {
	const target = walkSegments(value, segments);
	if (target === null || target === undefined || typeof target !== "object") return [];

	let matches: string[];
	if (Array.isArray(target)) {
		matches = target.map((_, i) => String(i)).filter(s => s.startsWith(partial));
	} else {
		matches = Object.keys(target as Record<string, unknown>).filter(k => k.startsWith(partial));
	}
	return collapseCompletions(matches, partial);
}

// ── Shell completions engine ─────────────────────────────────

// Flags that take a value argument (excluding -s which consumes all remaining non-flag args)
const FLAGS_WITH_VALUE = new Set(["-o", "--out", "--from", "--to", "--indexes"]);
const ALL_FLAGS = ["-h", "--help", "-j", "--json", "-r", "--rexc", "-t", "--tree",
	"--from", "--to", "-s", "--select", "-o", "--out", "--indexes", "--color", "--no-color"];
const DATA_EXTENSIONS = [".json", ".rexc", ".rex"];

/** Find the index of -s/--select in words (before the current word), or -1 */
function findSelectIndex(words: string[]): number {
	// Scan words before the current (last) word
	for (let i = 0; i < words.length - 1; i++) {
		const w = words[i]!;
		if (w === "-s" || w === "--select") return i;
		if (FLAGS_WITH_VALUE.has(w)) { i++; continue; }
	}
	return -1;
}

async function handleCompletions(argv: string[]) {
	// argv = words after "rx" on the command line, with the word being completed last
	// If empty, we're completing the first arg
	const words = argv.length > 0 ? argv : [""];
	const current = words[words.length - 1]!;
	const prev = words.length >= 2 ? words[words.length - 2] : undefined;

	// Completing a flag value?
	if (prev === "--from") return output(["json", "rexc"]);
	if (prev === "--to") return output(["json", "rexc", "tree"]);
	if (prev === "-o" || prev === "--out") return output(await listFiles(current, false));

	// Inside a selector? (any position after -s that isn't a flag)
	const selectIdx = findSelectIndex(words);
	if (selectIdx >= 0 && !current.startsWith("-")) {
		const files = extractFiles(words.slice(0, selectIdx));
		if (files.length > 0) {
			// Segments are all words between -s and the current word
			const segments = words.slice(selectIdx + 1, -1);
			try {
				const raw = await readFile(files[0]!, "utf8");
				const format = formatFromExt(files[0]!) ?? detectFormat(raw);
				const value = parseRaw(raw, format);
				return output(generateCompletions(value, segments, current));
			} catch { /* can't parse, no completions */ }
		}
		return output([]);
	}

	// Completing a flag?
	if (current.startsWith("-")) return output(ALL_FLAGS.filter(f => f.startsWith(current)));

	// Default: complete files
	return output(await listFiles(current, true));
}

function extractFiles(words: string[]): string[] {
	const files: string[] = [];
	for (let i = 0; i < words.length; i++) {
		const w = words[i]!;
		// Stop at -s since everything after it is selector segments
		if (w === "-s" || w === "--select") break;
		if (FLAGS_WITH_VALUE.has(w)) { i++; continue; }
		if (w.startsWith("-")) continue;
		files.push(w);
	}
	return files;
}

async function listFiles(prefix: string, dataOnly: boolean): Promise<string[]> {
	const { readdirSync, statSync } = await import("node:fs");
	const { dirname: d, basename: b, join: j } = await import("node:path");

	const dir = prefix.includes("/") ? d(prefix) : ".";
	const partial = prefix.includes("/") ? b(prefix) : prefix;

	try {
		const entries = readdirSync(dir, { withFileTypes: true });
		const results: string[] = [];
		for (const entry of entries) {
			if (!entry.name.startsWith(partial)) continue;
			if (entry.name.startsWith(".") && !partial.startsWith(".")) continue;
			const rel = dir === "." ? entry.name : j(dir, entry.name);
			if (entry.isDirectory()) {
				results.push(rel + "/");
			} else if (!dataOnly || DATA_EXTENSIONS.some(ext => entry.name.endsWith(ext))) {
				results.push(rel);
			}
		}
		return results.sort();
	} catch { return []; }
}

function output(completions: string[]) {
	if (completions.length > 0) process.stdout.write(completions.join("\n") + "\n");
}

// ── Shell shims & setup ─────────────────────────────────────

const ZSH_COMPLETION = `#compdef rx
_rx() {
	local -a results
	results=("\${(@f)$(rx --completions -- "\${(@)words[2,$CURRENT]}" 2>/dev/null)}")
	(( \${#results} == 0 )) && return
	# Check if we're inside a selector (after -s/--select)
	local in_select=0
	local i
	for (( i=2; i < CURRENT; i++ )); do
		[[ "\${words[$i]}" == (-s|--select) ]] && in_select=1 && break
	done
	local last="\${words[$CURRENT]}"
	if [[ "$last" == -* ]] || (( in_select )); then
		compadd -Q -S '' -- "\${results[@]}"
	else
		compadd -Q -f -S '' -- "\${results[@]}"
	fi
}
_rx "$@"`;

const BASH_COMPLETION = `_rx() {
	local IFS=$'\\n'
	COMPREPLY=($(rx --completions -- "\${COMP_WORDS[@]:1}" 2>/dev/null))
	[[ \${#COMPREPLY[@]} -gt 0 ]] && compopt -o nospace
}
complete -o default -F _rx rx`;

type Shell = "zsh" | "bash";

function detectShell(): Shell | undefined {
	const shell = process.env.SHELL ?? "";
	if (shell.endsWith("/zsh")) return "zsh";
	if (shell.endsWith("/bash")) return "bash";
	return undefined;
}

function completionScript(shell: Shell): string {
	return shell === "zsh" ? ZSH_COMPLETION : BASH_COMPLETION;
}

async function removeIfSymlink(path: string) {
	try {
		const stat = await lstat(path);
		if (stat.isSymbolicLink()) await unlink(path);
	} catch { /* doesn't exist, fine */ }
}

async function setupCompletions(args: string[]) {
	let shell = args[0] as Shell | undefined;
	if (shell && shell !== "zsh" && shell !== "bash") {
		throw new Error(`Unsupported shell: ${shell}. Use 'zsh' or 'bash'.`);
	}
	shell ??= detectShell();
	if (!shell) throw new Error("Cannot detect shell. Specify: rx setup-completions zsh|bash");

	const home = homedir();
	let dest: string;
	let extraInstructions = "";

	if (shell === "zsh") {
		const dir = join(home, ".local", "share", "zsh", "site-functions");
		await mkdir(dir, { recursive: true });
		dest = join(dir, "_rx");
		await removeIfSymlink(dest);
		await writeFile(dest, ZSH_COMPLETION + "\n", "utf8");
		extraInstructions = [
			"",
			"Ensure this is in your ~/.zshrc:",
			"",
			`  fpath=(${dir} $fpath)`,
			"  autoload -Uz compinit && compinit",
			"",
			"Then restart your shell or run: exec zsh",
		].join("\n");
	} else {
		const dir = join(home, ".local", "share", "bash-completion", "completions");
		await mkdir(dir, { recursive: true });
		dest = join(dir, "rx");
		await removeIfSymlink(dest);
		await writeFile(dest, BASH_COMPLETION + "\n", "utf8");
		extraInstructions = [
			"",
			"Ensure bash-completion is loaded in your ~/.bashrc:",
			"",
			`  [[ -r ${dir}/rx ]] && source ${dir}/rx`,
			"",
			"Then restart your shell or run: source ~/.bashrc",
		].join("\n");
	}

	process.stderr.write(`Installed ${shell} completions to ${dest}\n${extraInstructions}\n`);
}

// ── Output formatting ────────────────────────────────────────

function formatTree(value: unknown, color: boolean): string {
	const text = rexStringify(value, { indent: 2, maxWidth: 80 });
	if (!color) return text;
	return text.split("\n").map((line) => highlightLine(line)).join("\n");
}

function normalizeForJson(value: unknown, inArray: boolean): unknown {
	if (value === undefined) return inArray ? null : undefined;
	if (value === null || typeof value !== "object") return value;
	if (Array.isArray(value)) {
		const arr: unknown[] = [];
		for (let i = 0, len = (value as unknown[]).length; i < len; i++) {
			arr.push(normalizeForJson((value as unknown[])[i], true));
		}
		return arr;
	}
	const out: Record<string, unknown> = {};
	for (const key of Object.keys(value as Record<string, unknown>)) {
		const n = normalizeForJson((value as Record<string, unknown>)[key], false);
		if (n !== undefined) out[key] = n;
	}
	return out;
}

function formatOutput(value: unknown, format: OutputFormat, color: boolean, encodeOpts?: RexCStringifyOptions): string {
	if (format === "tree") return formatTree(value, color);
	if (format === "json") {
		const text = JSON.stringify(normalizeForJson(value, false), null, 2) ?? "null";
		return color ? highlightJSON(text) : text;
	}
	// rexc (non-streaming fallback for -o file output)
	const text = rexcStringify(value, encodeOpts);
	return color ? highlightRexc(text) : text;
}

function streamRexcOutput(value: unknown, encodeOpts?: RexCStringifyOptions) {
	rexcStringify(value, {
		...encodeOpts,
		onChunk: (chunk: string) => {
			process.stdout.write(chunk);
		},
	});
	process.stdout.write("\n");
}

// ── Main ─────────────────────────────────────────────────────

async function main() {
	const argv = process.argv.slice(2);

	if (argv[0] === "--completions") {
		const sub = argv[1];
		if (sub === "setup") {
			await setupCompletions(argv.slice(2));
			return;
		}
		if (sub === "zsh" || sub === "bash") {
			process.stdout.write(completionScript(sub) + "\n");
			return;
		}
		// --completions -- word1 word2 ... (called by shell shim)
		const dashDash = argv.indexOf("--");
		const words = dashDash >= 0 ? argv.slice(dashDash + 1) : [];
		await handleCompletions(words);
		return;
	}

	const opts = parseArgs(argv);
	if (opts.help) { console.log(usage()); return; }

	setColorEnabled(opts.color);

	const toFormat: OutputFormat = opts.toFormat
		?? (process.stdout.isTTY ? "tree" : "json");

	const encodeOpts: RexCStringifyOptions | undefined =
		opts.indexes !== undefined ? { indexes: opts.indexes } : undefined;

	const { value: parsed } = await readInput(opts);

	const value = opts.select ? applySelector(parsed, opts.select) : parsed;

	// Stream rexc directly to stdout (no color — chunks aren't full words yet)
	if (toFormat === "rexc" && !opts.out && !opts.color) {
		streamRexcOutput(value, encodeOpts);
		return;
	}

	// Stream tree output to stdout line-by-line
	if (toFormat === "tree" && !opts.out) {
		rexStringify(value, {
			indent: 2, maxWidth: 80,
			onLine: opts.color
				? (line: string) => { process.stdout.write(highlightLine(line) + "\n"); }
				: (line: string) => { process.stdout.write(line + "\n"); },
		});
		return;
	}

	const output = formatOutput(value, toFormat, opts.color, encodeOpts);

	if (opts.out) {
		await writeFile(opts.out, output + "\n", "utf8");
	} else {
		process.stdout.write(output + "\n");
	}
}

await main().catch((error) => {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`rx: ${message}\n`);
	process.exit(1);
});

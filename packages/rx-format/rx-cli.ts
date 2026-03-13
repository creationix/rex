import * as rexc from "@creationix/rex/rexc";
import { stringify as rexStringify } from "@creationix/rex";
import {
	setColorEnabled,
	highlightLine,
	highlightJSON,
	highlightRexc,
} from "@creationix/rex/rex-repl";
import { readFile, writeFile, mkdir, unlink, lstat } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

// ── Types ────────────────────────────────────────────────────

type Format = "json" | "rexc";
type OutputFormat = Format | "tree";

type RxOptions = {
	files: string[];
	fromFormat?: Format;
	toFormat?: OutputFormat;
	select?: string;
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
			const v = argv[++i];
			if (!v) throw new Error("Missing value for --select");
			opts.select = v;
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
		"  rx -s .routes[0].op data.rexc  Select a sub-value",
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
		"  -s, --select <path> Dot-path selector (e.g. .foo.bar[0].baz)",
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
	return rexc.parse(raw.trim());
}

async function readInput(opts: RxOptions): Promise<ParsedInput> {
	if (opts.files.length === 0) {
		// stdin
		if (process.stdin.isTTY) throw new Error("No input. Provide a file or pipe data via stdin.");
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

type Segment = { type: "key"; name: string } | { type: "index"; value: number };

const BARE_KEY = /^[a-zA-Z_][\w-]*$/;

function formatSegment(key: string): string {
	if (BARE_KEY.test(key)) return `.${key}`;
	return `["${key.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"]`;
}

function parseSelector(selector: string): Segment[] {
	const segments: Segment[] = [];
	const re = /\.([a-zA-Z_][\w-]*)|\[(\d+)\]|\["((?:[^"\\]|\\.)*)"\]/g;
	// Allow leading bare key without dot
	let s = selector;
	if (!s.startsWith(".") && !s.startsWith("[")) s = "." + s;

	let match: RegExpExecArray | null;
	let lastIndex = 0;
	while ((match = re.exec(s)) !== null) {
		if (match.index !== lastIndex) {
			throw new Error(`Invalid selector at position ${lastIndex}: ${selector}`);
		}
		if (match[1] !== undefined) {
			segments.push({ type: "key", name: match[1] });
		} else if (match[2] !== undefined) {
			segments.push({ type: "index", value: parseInt(match[2], 10) });
		} else if (match[3] !== undefined) {
			segments.push({ type: "key", name: match[3].replace(/\\(.)/g, "$1") });
		}
		lastIndex = re.lastIndex;
	}
	if (lastIndex !== s.length) {
		throw new Error(`Invalid selector at position ${lastIndex}: ${selector}`);
	}
	return segments;
}

function applySelector(value: unknown, selector: string): unknown {
	if (selector === "." || selector === "") return value;
	const segments = parseSelector(selector);
	let current = value;
	let path = "";
	for (const seg of segments) {
		if (seg.type === "key") {
			path += `.${seg.name}`;
			if (current === null || current === undefined || typeof current !== "object" || Array.isArray(current)) {
				throw new Error(`Selector ${path}: cannot access property '${seg.name}' on ${typeLabel(current)}`);
			}
			const obj = current as Record<string, unknown>;
			if (!(seg.name in obj)) {
				throw new Error(`Selector ${path}: property '${seg.name}' not found`);
			}
			current = obj[seg.name];
		} else {
			path += `[${seg.value}]`;
			if (!Array.isArray(current)) {
				throw new Error(`Selector ${path}: cannot index into ${typeLabel(current)}`);
			}
			if (seg.value < 0 || seg.value >= current.length) {
				throw new Error(`Selector ${path}: index ${seg.value} out of range (length ${current.length})`);
			}
			current = current[seg.value];
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

function walkToPrefix(value: unknown, prefix: string): { target: unknown; resolvedPrefix: string } {
	if (prefix === "" || prefix === ".") return { target: value, resolvedPrefix: "" };

	// Parse as much of the prefix as possible
	const segments = parseSelectorPartial(prefix);
	let current = value;
	let resolvedPrefix = "";
	for (const seg of segments) {
		if (seg.type === "key") {
			if (current === null || current === undefined || typeof current !== "object" || Array.isArray(current)) return { target: undefined, resolvedPrefix };
			const obj = current as Record<string, unknown>;
			if (!(seg.name in obj)) return { target: undefined, resolvedPrefix };
			current = obj[seg.name];
			resolvedPrefix += formatSegment(seg.name);
		} else {
			if (!Array.isArray(current) || seg.value >= current.length) return { target: undefined, resolvedPrefix };
			current = current[seg.value];
			resolvedPrefix += `[${seg.value}]`;
		}
	}
	return { target: current, resolvedPrefix };
}

function parseSelectorPartial(selector: string): Segment[] {
	// Parse complete segments from the prefix, ignoring trailing partial
	const segments: Segment[] = [];
	const re = /\.([a-zA-Z_][\w-]*)|\[(\d+)\]|\["((?:[^"\\]|\\.)*)"\]/g;
	let s = selector;
	if (!s.startsWith(".") && !s.startsWith("[")) s = "." + s;

	let match: RegExpExecArray | null;
	while ((match = re.exec(s)) !== null) {
		if (match[1] !== undefined) segments.push({ type: "key", name: match[1] });
		else if (match[2] !== undefined) segments.push({ type: "index", value: parseInt(match[2], 10) });
		else if (match[3] !== undefined) segments.push({ type: "key", name: match[3].replace(/\\(.)/g, "$1") });
	}
	return segments;
}

function generateCompletions(value: unknown, prefix: string): string[] {
	const { target, resolvedPrefix } = walkToPrefix(value, prefix);
	if (target === null || target === undefined || typeof target !== "object") return [];

	if (Array.isArray(target)) {
		return target.map((_, i) => `${resolvedPrefix}[${i}]`);
	}

	const keys = Object.keys(target as Record<string, unknown>);
	return keys.map((key) => resolvedPrefix + formatSegment(key));
}

// ── Shell completions engine ─────────────────────────────────

// Flags that take a value argument
const FLAGS_WITH_VALUE = new Set(["-s", "--select", "-o", "--out", "--from", "--to", "--indexes"]);
const ALL_FLAGS = ["-h", "--help", "-j", "--json", "-r", "--rexc", "-t", "--tree",
	"--from", "--to", "-s", "--select", "-o", "--out", "--indexes", "--color", "--no-color"];
const DATA_EXTENSIONS = [".json", ".rexc", ".rex"];

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

	// Completing a selector value? Parse files from earlier args to generate paths
	if (prev === "-s" || prev === "--select") {
		const files = extractFiles(words.slice(0, -1));
		if (files.length > 0) {
			const prefix = current || ".";
			try {
				const raw = await readFile(files[0]!, "utf8");
				const format = formatFromExt(files[0]!) ?? detectFormat(raw);
				const value = parseRaw(raw, format);
				return output(generateCompletions(value, prefix));
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
	local last="\${words[$CURRENT]}"
	if [[ "$last" == -* ]] || { local prev="\${words[$((CURRENT-1))]}"; [[ "$prev" == (-s|--select) ]]; }; then
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
	if (Array.isArray(value)) return value.map((item) => normalizeForJson(item, true));
	const out: Record<string, unknown> = {};
	for (const [key, val] of Object.entries(value as Record<string, unknown>)) {
		const n = normalizeForJson(val, false);
		if (n !== undefined) out[key] = n;
	}
	return out;
}

function formatOutput(value: unknown, format: OutputFormat, color: boolean, encodeOpts?: rexc.RexCStringifyOptions): string {
	if (format === "tree") return formatTree(value, color);
	if (format === "json") {
		const text = JSON.stringify(normalizeForJson(value, false), null, 2) ?? "null";
		return color ? highlightJSON(text) : text;
	}
	// rexc (non-streaming fallback for -o file output)
	const text = rexc.stringify(value, encodeOpts);
	return color ? highlightRexc(text) : text;
}

function streamRexcOutput(value: unknown, encodeOpts?: rexc.RexCStringifyOptions) {
	rexc.stringify(value, {
		...encodeOpts,
		onChunk: (chunk) => {
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

	const encodeOpts: rexc.RexCStringifyOptions | undefined =
		opts.indexes !== undefined ? { indexes: opts.indexes } : undefined;

	const { value: parsed } = await readInput(opts);

	const value = opts.select ? applySelector(parsed, opts.select) : parsed;

	// Stream rexc directly to stdout (no color — chunks aren't full words yet)
	if (toFormat === "rexc" && !opts.out && !opts.color) {
		streamRexcOutput(value, encodeOpts);
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

import { open } from "./rx";
import { readdirSync } from "node:fs";
import { readFile, writeFile, mkdir, unlink, lstat } from "node:fs/promises";
import { homedir } from "node:os";
import { join, dirname, basename } from "node:path";

// ── ANSI colors ──────────────────────────────────────────────

function createColors(on: boolean) {
	const o = on ? (s: string) => s : () => "";
	return {
		reset: o("\x1b[0m"), dim: o("\x1b[2m"),
		green: o("\x1b[38;5;114m"), yellow: o("\x1b[38;5;179m"),
		cyan: o("\x1b[38;5;81m"), magenta: o("\x1b[38;5;141m"),
	};
}

let C = createColors(false);

// ── Types & arg parsing ──────────────────────────────────────

type Format = "json" | "rexc";
type OutputFormat = "json" | "rexc" | "tree";

type RxOptions = {
	files: string[];
	fromFormat?: Format;
	toFormat?: OutputFormat;
	select?: string[];
	out?: string;
	color: boolean;
	help: boolean;
};

function parseArgs(argv: string[]): RxOptions {
	const opts: RxOptions = {
		files: [],
		color: process.stdout.isTTY ?? false,
		help: false,
	};
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i]!;
		if (arg === "-h" || arg === "--help") { opts.help = true; continue; }
		if (arg === "--color") { opts.color = true; continue; }
		if (arg === "--no-color") { opts.color = false; continue; }
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
		"  --to json|tree      Output format",
		"  -j, --json          Shortcut for --to json",
		"  -t, --tree          Shortcut for --to tree",
		"",
		"  Default output: tree on TTY, json when piped.",
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

// ── Format detection & input reading ─────────────────────────

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

async function readStdin(): Promise<string> {
	const chunks: Buffer[] = [];
	for await (const chunk of process.stdin) {
		chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
	}
	return Buffer.concat(chunks).toString("utf8");
}

function parseRaw(raw: string, format: Format): unknown {
	if (format === "json") return JSON.parse(raw);
	return open(new TextEncoder().encode(raw.trim()));
}

async function readOne(file: string, fromFormat?: Format): Promise<unknown> {
	const raw = file === "-" ? await readStdin() : await readFile(file, "utf8");
	const format = fromFormat ?? (file === "-" ? detectFormat(raw) : formatFromExt(file) ?? detectFormat(raw));
	return parseRaw(raw, format);
}

async function readInput(opts: RxOptions): Promise<unknown> {
	if (opts.files.length === 0) {
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
		return parseRaw(raw, opts.fromFormat ?? detectFormat(raw));
	}

	if (opts.files.length === 1) return readOne(opts.files[0]!, opts.fromFormat);

	const values: unknown[] = [];
	for (const file of opts.files) values.push(await readOne(file, opts.fromFormat));
	return values;
}

// ── Selector ─────────────────────────────────────────────────

function applySelector(value: unknown, segments: string[]): unknown {
	let current = value;
	let path = "";
	for (const seg of segments) {
		const idx = /^\d+$/.test(seg) ? parseInt(seg, 10) : undefined;
		if (Array.isArray(current) && idx !== undefined) {
			path += `[${idx}]`;
			if (idx < 0 || idx >= current.length) {
				throw new Error(`Selector ${path}: index ${idx} out of range (length ${current.length})`);
			}
			current = current[idx];
		} else if (isObj(current)) {
			path += `.${seg}`;
			if (!(seg in current)) throw new Error(`Selector${path}: property '${seg}' not found`);
			current = current[seg];
		} else {
			throw new Error(`Selector${path}.${seg}: cannot index into ${typeLabel(current)}`);
		}
	}
	return current;
}

function typeLabel(v: unknown): string {
	if (v === null) return "null";
	if (Array.isArray(v)) return "array";
	return typeof v;
}

// ── Tree pretty-printer ──────────────────────────────────────
// Rex-style: bare keys, space-separated arrays, inline-first

function isObj(v: unknown): v is Record<string, unknown> {
	if (!v || typeof v !== "object" || Array.isArray(v)) return false;
	const p = Object.getPrototypeOf(v);
	return p === Object.prototype || p === null;
}

function isBareKey(k: string): boolean { return /^[A-Za-z_][A-Za-z0-9_-]*$/.test(k); }

function fmtKey(k: string): string {
	if (isBareKey(k)) return k;
	if (k !== "" && String(Number(k)) === k && Number.isFinite(Number(k))) return k;
	return JSON.stringify(k);
}

function fmtInline(v: unknown): string {
	if (v === undefined) return "undefined";
	if (v === null) return "null";
	if (typeof v === "boolean") return String(v);
	if (typeof v === "number") {
		if (Number.isNaN(v)) return "nan";
		if (v === Infinity) return "inf";
		if (v === -Infinity) return "-inf";
		return String(v);
	}
	if (typeof v === "string") return JSON.stringify(v);
	if (Array.isArray(v)) {
		if (v.length === 0) return "[]";
		let s = "[";
		for (let i = 0; i < v.length; i++) s += (i ? " " : "") + fmtInline(v[i]);
		return s + "]";
	}
	if (isObj(v)) {
		const ks = Object.keys(v);
		if (ks.length === 0) return "{}";
		let s = "{";
		for (let i = 0; i < ks.length; i++) {
			if (i) s += " ";
			s += fmtKey(ks[i]!) + ": " + fmtInline(v[ks[i]!]);
		}
		return s + "}";
	}
	return String(v);
}

function fmtPretty(v: unknown, depth: number, ind: number, maxW: number): string {
	if (v === undefined || v === null || typeof v !== "object") return fmtInline(v);
	const budget = maxW - depth * ind;

	if (Array.isArray(v)) {
		if (v.length === 0) return "[]";
		// try inline (bail on nested objects/arrays)
		let s = "[", ok = true;
		for (let i = 0; i < v.length; i++) {
			if (typeof v[i] === "object" && v[i] !== null) { ok = false; break; }
			s += (i ? " " : "") + fmtInline(v[i]);
			if (s.length > budget) { ok = false; break; }
		}
		if (ok) { s += "]"; if (s.length <= budget) return s; }
		const pad = " ".repeat(depth * ind), cp = " ".repeat((depth + 1) * ind);
		let r = "[\n";
		for (let i = 0; i < v.length; i++) {
			if (i) r += "\n";
			r += cp + fmtPretty(v[i], depth + 1, ind, maxW);
		}
		return r + "\n" + pad + "]";
	}

	if (isObj(v)) {
		const ks = Object.keys(v);
		if (ks.length === 0) return "{}";
		let s = "{", ok = true;
		for (const k of ks) {
			if (typeof v[k] === "object" && v[k] !== null) { ok = false; break; }
			if (s.length > 1) s += " ";
			s += fmtKey(k) + ": " + fmtInline(v[k]);
			if (s.length > budget) { ok = false; break; }
		}
		if (ok) { if (s.length === 1) return "{}"; s += "}"; if (s.length <= budget) return s; }
		const pad = " ".repeat(depth * ind), cp = " ".repeat((depth + 1) * ind);
		let r = "{\n", first = true;
		for (const k of ks) {
			if (!first) r += "\n";
			first = false;
			r += cp + fmtKey(k) + ": " + fmtPretty(v[k], depth + 1, ind, maxW);
		}
		return r + "\n" + pad + "}";
	}

	return fmtInline(v);
}

function treeStringify(value: unknown, onLine?: (line: string) => void): string {
	const text = fmtPretty(value, 0, 2, 80);
	if (onLine) { for (const line of text.split("\n")) onLine(line); return ""; }
	return text;
}

// ── Syntax highlighting ──────────────────────────────────────

function highlightTree(line: string): string {
	let result = "", i = 0;
	const len = line.length;
	while (i < len) {
		if (line[i] === " " || line[i] === "\t") { result += line[i]; i++; continue; }
		// key followed by ':'
		const km = line.slice(i).match(/^([A-Za-z_][A-Za-z0-9_-]*|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|"(?:[^"\\]|\\.)*")(\s*:)/);
		if (km) { result += C.magenta + km[1] + C.reset + km[2]; i += km[0].length; continue; }
		// string
		if (line[i] === '"') {
			const m = line.slice(i).match(/^"(?:[^"\\]|\\.)*"/);
			if (m) { result += C.green + m[0] + C.reset; i += m[0].length; continue; }
		}
		// keywords
		const kw = line.slice(i).match(/^(?:true|false|null|undefined|nan|-?inf)\b/);
		if (kw) { result += C.yellow + kw[0] + C.reset; i += kw[0].length; continue; }
		// numbers
		const nm = line.slice(i).match(/^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?(?=[\s\]\}]|$)/);
		if (nm) { result += C.cyan + nm[0] + C.reset; i += nm[0].length; continue; }
		result += line[i]; i++;
	}
	return result;
}

const JSON_RE = /(?<key>"(?:[^"\\]|\\.)*")\s*:|(?<string>"(?:[^"\\]|\\.)*")|(?<number>-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)\b|(?<bool>true|false)|(?<null>null)/g;

function highlightJSON(json: string): string {
	let result = "", last = 0;
	JSON_RE.lastIndex = 0;
	for (const m of json.matchAll(JSON_RE)) {
		result += json.slice(last, m.index);
		const g = m.groups!;
		if (g.key) result += C.cyan + g.key + C.reset + ":";
		else if (g.string) result += C.green + m[0] + C.reset;
		else if (g.number || g.bool) result += C.yellow + m[0] + C.reset;
		else if (g.null) result += C.dim + m[0] + C.reset;
		else result += m[0];
		last = m.index! + m[0].length;
	}
	return result + json.slice(last);
}

// ── Output formatting ────────────────────────────────────────

function normalizeForJson(value: unknown, inArray: boolean): unknown {
	if (value === undefined) return inArray ? null : undefined;
	if (value === null || typeof value !== "object") return value;
	if (Array.isArray(value)) return value.map(v => normalizeForJson(v, true));
	const obj = value as Record<string, unknown>;
	const out: Record<string, unknown> = {};
	for (const key of Object.keys(obj)) {
		const n = normalizeForJson(obj[key], false);
		if (n !== undefined) out[key] = n;
	}
	return out;
}

function formatOutput(value: unknown, format: OutputFormat, color: boolean): string {
	if (format === "tree") {
		const text = treeStringify(value);
		if (!color) return text;
		return text.split("\n").map(highlightTree).join("\n");
	}
	if (format === "json") {
		const text = JSON.stringify(normalizeForJson(value, false), null, 2) ?? "null";
		return color ? highlightJSON(text) : text;
	}
	throw new Error("TODO: rexc encoding is not yet supported. Use --to json or --to tree.");
}

// ── Shell completions ────────────────────────────────────────

const FLAGS_WITH_VALUE = new Set(["-o", "--out", "--from", "--to"]);
const ALL_FLAGS = ["-h", "--help", "-j", "--json", "-r", "--rexc", "-t", "--tree",
	"--from", "--to", "-s", "--select", "-o", "--out", "--color", "--no-color"];
const DATA_EXTENSIONS = [".json", ".rexc", ".rex"];

function findSelectIndex(words: string[]): number {
	for (let i = 0; i < words.length - 1; i++) {
		const w = words[i]!;
		if (w === "-s" || w === "--select") return i;
		if (FLAGS_WITH_VALUE.has(w)) { i++; continue; }
	}
	return -1;
}

function extractFiles(words: string[]): string[] {
	const files: string[] = [];
	for (let i = 0; i < words.length; i++) {
		const w = words[i]!;
		if (w === "-s" || w === "--select") break;
		if (FLAGS_WITH_VALUE.has(w)) { i++; continue; }
		if (w.startsWith("-")) continue;
		files.push(w);
	}
	return files;
}

function listFiles(prefix: string, dataOnly: boolean): string[] {
	const dir = prefix.includes("/") ? dirname(prefix) : ".";
	const partial = prefix.includes("/") ? basename(prefix) : prefix;
	try {
		const entries = readdirSync(dir, { withFileTypes: true });
		const results: string[] = [];
		for (const entry of entries) {
			if (!entry.name.startsWith(partial)) continue;
			if (entry.name.startsWith(".") && !partial.startsWith(".")) continue;
			const rel = dir === "." ? entry.name : join(dir, entry.name);
			if (entry.isDirectory()) {
				results.push(rel + "/");
			} else if (!dataOnly || DATA_EXTENSIONS.some(ext => entry.name.endsWith(ext))) {
				results.push(rel);
			}
		}
		return results.sort();
	} catch { return []; }
}

function printCompletions(completions: string[]) {
	if (completions.length > 0) process.stdout.write(completions.join("\n") + "\n");
}

function walkSegments(value: unknown, segments: string[]): unknown {
	let current = value;
	for (const seg of segments) {
		const idx = /^\d+$/.test(seg) ? parseInt(seg, 10) : undefined;
		if (Array.isArray(current) && idx !== undefined) {
			if (idx < 0 || idx >= current.length) return undefined;
			current = current[idx];
		} else if (isObj(current)) {
			if (!(seg in current)) return undefined;
			current = current[seg];
		} else {
			return undefined;
		}
	}
	return current;
}

const MAX_COMPLETIONS = 50;

function collapseCompletions(matches: string[], partial: string): string[] {
	if (matches.length <= MAX_COMPLETIONS) return matches;
	matches.sort();
	const maxLen = matches[matches.length - 1]!.length;
	function distinctAt(len: number): number {
		let count = 1;
		for (let i = 1; i < matches.length; i++) {
			const a = matches[i - 1]!, b = matches[i]!;
			let same = a.length >= len && b.length >= len;
			if (same) {
				for (let j = 0; j < len; j++) {
					if (a.charCodeAt(j) !== b.charCodeAt(j)) { same = false; break; }
				}
			} else {
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
	let lo = partial.length + 1;
	let hi = maxLen;
	while (lo < hi) {
		const mid = (lo + hi + 1) >>> 1;
		if (distinctAt(mid) <= MAX_COMPLETIONS) lo = mid;
		else hi = mid - 1;
	}
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

async function handleCompletions(argv: string[]) {
	const words = argv.length > 0 ? argv : [""];
	const current = words[words.length - 1]!;
	const prev = words.length >= 2 ? words[words.length - 2] : undefined;

	if (prev === "--from") return printCompletions(["json", "rexc"]);
	if (prev === "--to") return printCompletions(["json", "rexc", "tree"]);
	if (prev === "-o" || prev === "--out") return printCompletions(listFiles(current, false));

	const selectIdx = findSelectIndex(words);
	if (selectIdx >= 0 && !current.startsWith("-")) {
		const files = extractFiles(words.slice(0, selectIdx));
		if (files.length > 0) {
			const segments = words.slice(selectIdx + 1, -1);
			try {
				const raw = await readFile(files[0]!, "utf8");
				const format = formatFromExt(files[0]!) ?? detectFormat(raw);
				const value = parseRaw(raw, format);
				return printCompletions(generateCompletions(value, segments, current));
			} catch { /* can't parse, no completions */ }
		}
		return printCompletions([]);
	}

	if (current.startsWith("-")) return printCompletions(ALL_FLAGS.filter(f => f.startsWith(current)));
	return printCompletions(listFiles(current, true));
}

// ── Shell completion scripts & setup ─────────────────────────

const ZSH_COMPLETION = `#compdef rx
_rx() {
	local -a results
	results=("\${(@f)$(rx --completions -- "\${(@)words[2,$CURRENT]}" 2>/dev/null)}")
	(( \${#results} == 0 )) && return
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

async function removeIfSymlink(path: string) {
	try {
		const stat = await lstat(path);
		if (stat.isSymbolicLink()) await unlink(path);
	} catch { /* doesn't exist */ }
}

async function setupCompletions(args: string[]) {
	let shell = args[0] as Shell | undefined;
	if (shell && shell !== "zsh" && shell !== "bash") {
		throw new Error(`Unsupported shell: ${shell}. Use 'zsh' or 'bash'.`);
	}
	shell ??= detectShell();
	if (!shell) throw new Error("Cannot detect shell. Specify: rx setup-completions zsh|bash");

	const home = homedir();
	const isZsh = shell === "zsh";
	const dir = isZsh
		? join(home, ".local", "share", "zsh", "site-functions")
		: join(home, ".local", "share", "bash-completion", "completions");
	const dest = join(dir, isZsh ? "_rx" : "rx");
	const script = isZsh ? ZSH_COMPLETION : BASH_COMPLETION;

	await mkdir(dir, { recursive: true });
	await removeIfSymlink(dest);
	await writeFile(dest, script + "\n", "utf8");

	const instructions = isZsh
		? `\nEnsure this is in your ~/.zshrc:\n\n  fpath=(${dir} $fpath)\n  autoload -Uz compinit && compinit\n\nThen restart your shell or run: exec zsh`
		: `\nEnsure bash-completion is loaded in your ~/.bashrc:\n\n  [[ -r ${dir}/rx ]] && source ${dir}/rx\n\nThen restart your shell or run: source ~/.bashrc`;

	process.stderr.write(`Installed ${shell} completions to ${dest}${instructions}\n`);
}

// ── Main ─────────────────────────────────────────────────────

async function main() {
	const argv = process.argv.slice(2);

	if (argv[0] === "--completions") {
		const sub = argv[1];
		if (sub === "setup") { await setupCompletions(argv.slice(2)); return; }
		if (sub === "zsh" || sub === "bash") {
			process.stdout.write((sub === "zsh" ? ZSH_COMPLETION : BASH_COMPLETION) + "\n");
			return;
		}
		const dashDash = argv.indexOf("--");
		await handleCompletions(dashDash >= 0 ? argv.slice(dashDash + 1) : []);
		return;
	}

	const opts = parseArgs(argv);
	if (opts.help) { console.log(usage()); return; }

	C = createColors(opts.color);

	const toFormat: OutputFormat = opts.toFormat ?? (process.stdout.isTTY ? "tree" : "json");
	const parsed = await readInput(opts);
	const value = opts.select ? applySelector(parsed, opts.select) : parsed;

	// Stream tree to stdout line-by-line
	if (toFormat === "tree" && !opts.out) {
		treeStringify(value, opts.color
			? (line: string) => { process.stdout.write(highlightTree(line) + "\n"); }
			: (line: string) => { process.stdout.write(line + "\n"); },
		);
		return;
	}

	const out = formatOutput(value, toFormat, opts.color);

	if (opts.out) {
		await writeFile(opts.out, out + "\n", "utf8");
	} else {
		process.stdout.write(out + "\n");
	}
}

await main().catch((error) => {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`rx: ${message}\n`);
	process.exit(1);
});

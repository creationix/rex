import * as readline from "node:readline";
import { createRequire } from "node:module";
import { grammar, stringify, parseToIR, optimizeIR, compile } from "./rex.ts";
import { evaluateRexc } from "./rexc-interpreter.ts";

const req = createRequire(import.meta.url);
const { version } = req("./package.json");

// ── ANSI helpers ──────────────────────────────────────────────

const C = {
	reset: "\x1b[0m",
	bold: "\x1b[1m",
	dim: "\x1b[2m",
	red: "\x1b[31m",
	green: "\x1b[32m",
	yellow: "\x1b[33m",
	blue: "\x1b[34m",
	cyan: "\x1b[36m",
	gray: "\x1b[90m",
	boldBlue: "\x1b[1;34m",
};

// ── Syntax highlighting ───────────────────────────────────────

const TOKEN_RE =
	/(?<blockComment>\/\*[\s\S]*?(?:\*\/|$))|(?<lineComment>\/\/[^\n]*)|(?<dstring>"(?:[^"\\]|\\.)*"?)|(?<sstring>'(?:[^'\\]|\\.)*'?)|(?<keyword>\b(?:when|unless|while|for|do|end|in|of|and|or|else|break|continue|delete|self)(?![a-zA-Z0-9_-]))|(?<literal>\b(?:true|false|null|undefined)(?![a-zA-Z0-9_-]))|(?<typePred>\b(?:string|number|object|array|boolean)(?![a-zA-Z0-9_-]))|(?<num>\b(?:0x[0-9a-fA-F]+|0b[01]+|(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)\b)/g;

export function highlightLine(line: string): string {
	let result = "";
	let lastIndex = 0;
	TOKEN_RE.lastIndex = 0;

	for (const m of line.matchAll(TOKEN_RE)) {
		result += line.slice(lastIndex, m.index);
		const text = m[0];
		const g = m.groups!;
		if (g.blockComment || g.lineComment) {
			result += C.gray + text + C.reset;
		} else if (g.dstring || g.sstring) {
			result += C.green + text + C.reset;
		} else if (g.keyword) {
			result += C.boldBlue + text + C.reset;
		} else if (g.literal) {
			result += C.yellow + text + C.reset;
		} else if (g.typePred) {
			result += C.cyan + text + C.reset;
		} else if (g.num) {
			result += C.cyan + text + C.reset;
		} else {
			result += text;
		}
		lastIndex = m.index! + text.length;
	}
	result += line.slice(lastIndex);
	return result;
}

// ── Multi-line detection ──────────────────────────────────────

/** Strip string literals and comments, replacing them with spaces. */
function stripStringsAndComments(source: string): string {
	return source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'/g, (m) =>
		" ".repeat(m.length),
	);
}

function countWord(text: string, word: string): number {
	const re = new RegExp(`\\b${word}(?![a-zA-Z0-9_-])`, "g");
	return (text.match(re) || []).length;
}

export function isIncomplete(buffer: string): boolean {
	// If the grammar accepts it, it's complete.
	try {
		if (grammar.match(buffer).succeeded()) return false;
	} catch {
		// match itself shouldn't throw, but be safe
	}

	const stripped = stripStringsAndComments(buffer);

	// Unmatched brackets
	let parens = 0, brackets = 0, braces = 0;
	for (const ch of stripped) {
		if (ch === "(") parens++;
		else if (ch === ")") parens--;
		else if (ch === "[") brackets++;
		else if (ch === "]") brackets--;
		else if (ch === "{") braces++;
		else if (ch === "}") braces--;
	}
	if (parens > 0 || brackets > 0 || braces > 0) return true;

	// Unmatched do/end or when/unless/for without end
	const doCount = countWord(stripped, "do");
	const endCount = countWord(stripped, "end");
	if (doCount > endCount) return true;

	// Trailing binary operator or keyword suggests continuation
	const trimmed = buffer.trimEnd();
	if (/[+\-*/%&|^=<>]$/.test(trimmed)) return true;
	if (/\b(?:and|or|do|in|of)\s*$/.test(trimmed)) return true;

	return false;
}

// ── Result & state formatting ─────────────────────────────────

function formatResult(value: unknown): string {
	let text: string;
	try {
		text = stringify(value, { maxWidth: 60 });
	} catch {
		text = String(value);
	}
	return `${C.gray}→${C.reset} ${highlightLine(text)}`;
}

export function formatVarState(vars: Record<string, unknown>): string {
	const entries = Object.entries(vars);
	if (entries.length === 0) return "";

	const MAX_LINE = 70;
	const MAX_VALUE = 30;
	const parts: string[] = [];
	let totalLen = 0;

	for (const [key, val] of entries) {
		let valStr: string;
		try {
			valStr = stringify(val, { maxWidth: MAX_VALUE });
		} catch {
			valStr = String(val);
		}
		if (valStr.length > MAX_VALUE) {
			valStr = valStr.slice(0, MAX_VALUE - 1) + "\u2026";
		}
		const part = `${key} = ${valStr}`;
		if (totalLen + part.length + 2 > MAX_LINE && parts.length > 0) {
			parts.push("\u2026");
			break;
		}
		parts.push(part);
		totalLen += part.length + 2;
	}

	return `${C.dim}  ${parts.join(", ")}${C.reset}`;
}

// ── Tab completion ────────────────────────────────────────────

const KEYWORDS = [
	"when", "unless", "while", "for", "do", "end", "in", "of",
	"and", "or", "else", "break", "continue", "delete",
	"self", "true", "false", "null", "undefined",
	"string", "number", "object", "array", "boolean",
];

function completer(state: ReplState): (line: string) => [string[], string] {
	return (line: string) => {
		const match = line.match(/[a-zA-Z_][a-zA-Z0-9_.-]*$/);
		const partial = match ? match[0] : "";
		if (!partial) return [[], ""];

		const varNames = Object.keys(state.vars);
		const all = [...new Set([...KEYWORDS, ...varNames])];
		const hits = all.filter((w) => w.startsWith(partial));
		return [hits, partial];
	};
}

// ── Dot commands ──────────────────────────────────────────────

function handleDotCommand(cmd: string, state: ReplState, rl: readline.Interface): boolean {
	function toggleLabel(on: boolean): string {
		return on ? `${C.green}on${C.reset}` : `${C.dim}off${C.reset}`;
	}

	switch (cmd) {
		case ".help":
			console.log(
				[
					`${C.boldBlue}Rex REPL Commands:${C.reset}`,
					"  .help   Show this help message",
					"  .vars   Show all current variables",
					"  .clear  Clear all variables",
					"  .ir     Toggle showing IR JSON after parsing",
					"  .rexc   Toggle showing compiled rexc before execution",
					"  .opt    Toggle IR optimizations",
					"  .exit   Exit the REPL",
					"",
					"Enter Rex expressions to evaluate them.",
					"Multi-line: open brackets or do/end blocks continue on the next line.",
					"Ctrl-C cancels multi-line input.",
					"Ctrl-D exits.",
				].join("\n"),
			);
			return true;

		case ".ir":
			state.showIR = !state.showIR;
			console.log(`${C.dim}  IR display: ${toggleLabel(state.showIR)}${C.reset}`);
			return true;

		case ".rexc":
			state.showRexc = !state.showRexc;
			console.log(`${C.dim}  Rexc display: ${toggleLabel(state.showRexc)}${C.reset}`);
			return true;

		case ".opt":
			state.optimize = !state.optimize;
			console.log(`${C.dim}  Optimizations: ${toggleLabel(state.optimize)}${C.reset}`);
			return true;

		case ".vars": {
			const entries = Object.entries(state.vars);
			if (entries.length === 0) {
				console.log(`${C.dim}  (no variables)${C.reset}`);
			} else {
				for (const [key, val] of entries) {
					let valStr: string;
					try {
						valStr = stringify(val, { maxWidth: 60 });
					} catch {
						valStr = String(val);
					}
					console.log(`  ${key} = ${highlightLine(valStr)}`);
				}
			}
			return true;
		}

		case ".clear":
			state.vars = {};
			state.refs = {};
			console.log(`${C.dim}  Variables cleared.${C.reset}`);
			return true;

		case ".exit":
			rl.close();
			return true;

		default:
			if (cmd.startsWith(".")) {
				console.log(`${C.red}  Unknown command: ${cmd}. Type .help for available commands.${C.reset}`);
				return true;
			}
			return false;
	}
}

// ── REPL state ────────────────────────────────────────────────

type ReplState = {
	vars: Record<string, unknown>;
	refs: Partial<Record<number, unknown>>;
	showIR: boolean;
	showRexc: boolean;
	optimize: boolean;
};

// ── Gas limit for loop safety ─────────────────────────────────

const GAS_LIMIT = 10_000_000;

// ── Main REPL entry point ─────────────────────────────────────

export async function startRepl(): Promise<void> {
	const state: ReplState = { vars: {}, refs: {}, showIR: false, showRexc: false, optimize: false };
	let multiLineBuffer = "";

	const PRIMARY_PROMPT = "rex> ";
	const CONT_PROMPT = "...  ";
	const STYLED_PRIMARY = `${C.boldBlue}rex${C.reset}> `;
	const STYLED_CONT = `${C.dim}...${C.reset}  `;

	let currentPrompt = PRIMARY_PROMPT;
	let styledPrompt = STYLED_PRIMARY;

	console.log(`${C.boldBlue}Rex${C.reset} v${version} — type ${C.dim}.help${C.reset} for commands`);

	const rl = readline.createInterface({
		input: process.stdin,
		output: process.stdout,
		prompt: PRIMARY_PROMPT,
		historySize: 500,
		completer: completer(state),
		terminal: true,
	});

	// ── Syntax highlighting via keypress redraw ──
	process.stdin.on("keypress", () => {
		process.nextTick(() => {
			// Guard: if the interface has been closed, skip
			if (!rl.line && rl.line !== "") return;

			readline.clearLine(process.stdout, 0);
			readline.cursorTo(process.stdout, 0);
			process.stdout.write(styledPrompt + highlightLine(rl.line));
			readline.cursorTo(process.stdout, currentPrompt.length + rl.cursor);
		});
	});

	// ── Ctrl-L to clear screen ──
	process.stdin.on("keypress", (_ch: string, key: readline.Key) => {
		if (key?.ctrl && key.name === "l") {
			readline.cursorTo(process.stdout, 0, 0);
			readline.clearScreenDown(process.stdout);
			rl.prompt();
		}
	});

	// ── Ctrl-C handling ──
	rl.on("SIGINT", () => {
		if (multiLineBuffer) {
			multiLineBuffer = "";
			currentPrompt = PRIMARY_PROMPT;
			styledPrompt = STYLED_PRIMARY;
			rl.setPrompt(PRIMARY_PROMPT);
			process.stdout.write("\n");
			rl.prompt();
		} else {
			console.log();
			rl.close();
		}
	});

	function resetPrompt() {
		currentPrompt = PRIMARY_PROMPT;
		styledPrompt = STYLED_PRIMARY;
		rl.setPrompt(PRIMARY_PROMPT);
		rl.prompt();
	}

	// ── Line handler ──
	rl.on("line", (line: string) => {
		const trimmed = line.trim();

		// Dot commands (only when not accumulating multi-line)
		if (!multiLineBuffer && trimmed.startsWith(".")) {
			if (handleDotCommand(trimmed, state, rl)) {
				rl.prompt();
				return;
			}
		}

		// Accumulate
		multiLineBuffer += (multiLineBuffer ? "\n" : "") + line;

		// Empty input
		if (multiLineBuffer.trim() === "") {
			multiLineBuffer = "";
			rl.prompt();
			return;
		}

		// Check for incomplete expression
		if (isIncomplete(multiLineBuffer)) {
			currentPrompt = CONT_PROMPT;
			styledPrompt = STYLED_CONT;
			rl.setPrompt(CONT_PROMPT);
			rl.prompt();
			return;
		}

		// Try to evaluate
		const source = multiLineBuffer;
		multiLineBuffer = "";

		const match = grammar.match(source);
		if (!match.succeeded()) {
			console.log(`${C.red}  ${match.message}${C.reset}`);
			resetPrompt();
			return;
		}

		try {
			const ir = parseToIR(source);
			const lowered = state.optimize ? optimizeIR(ir) : ir;

			if (state.showIR) {
				console.log(`${C.dim}  IR: ${JSON.stringify(lowered)}${C.reset}`);
			}

			const rexc = compile(source, { optimize: state.optimize });

			if (state.showRexc) {
				console.log(`${C.dim}  rexc: ${rexc}${C.reset}`);
			}

			const result = evaluateRexc(rexc, {
				vars: { ...state.vars },
				refs: { ...state.refs },
				gasLimit: GAS_LIMIT,
			});
			state.vars = result.state.vars;
			state.refs = result.state.refs;

			console.log(formatResult(result.value));
			const varLine = formatVarState(state.vars);
			if (varLine) console.log(varLine);
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			if (message.includes("Gas limit exceeded")) {
				console.log(`${C.yellow}  ${message}${C.reset}`);
			} else {
				console.log(`${C.red}  Error: ${message}${C.reset}`);
			}
		}

		resetPrompt();
	});

	// ── Exit ──
	rl.on("close", () => {
		process.exit(0);
	});

	rl.prompt();
}

import { compile, parse, parseToIR } from "./rex.ts";
import { evaluateSource } from "./rexc-interpreter.ts";
import { dirname, resolve } from "node:path";
import { readFile, writeFile } from "node:fs/promises";

type CliOptions = {
	expr?: string;
	file?: string;
	out?: string;
	compile: boolean;
	ir: boolean;
	minifyNames: boolean;
	dedupeValues: boolean;
	dedupeMinBytes?: number;
	help: boolean;
};

function parseArgs(argv: string[]): CliOptions {
	const options: CliOptions = {
		compile: false,
		ir: false,
		minifyNames: false,
		dedupeValues: false,
		help: false,
	};
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (!arg) continue;
		if (arg === "--help" || arg === "-h") {
			options.help = true;
			continue;
		}
		if (arg === "--compile" || arg === "-c") {
			options.compile = true;
			continue;
		}
		if (arg === "--ir") {
			options.ir = true;
			continue;
		}
		if (arg === "--minify-names" || arg === "-m") {
			options.minifyNames = true;
			continue;
		}
		if (arg === "--dedupe-values") {
			options.dedupeValues = true;
			continue;
		}
		if (arg === "--dedupe-min-bytes") {
			const value = argv[index + 1];
			if (!value) throw new Error("Missing value for --dedupe-min-bytes");
			const parsed = Number(value);
			if (!Number.isInteger(parsed) || parsed < 1) throw new Error("--dedupe-min-bytes must be a positive integer");
			options.dedupeMinBytes = parsed;
			index += 1;
			continue;
		}
		if (arg === "--expr" || arg === "-e") {
			const value = argv[index + 1];
			if (!value) throw new Error("Missing value for --expr");
			options.expr = value;
			index += 1;
			continue;
		}
		if (arg === "--file" || arg === "-f") {
			const value = argv[index + 1];
			if (!value) throw new Error("Missing value for --file");
			options.file = value;
			index += 1;
			continue;
		}
		if (arg === "--out" || arg === "-o") {
			const value = argv[index + 1];
			if (!value) throw new Error("Missing value for --out");
			options.out = value;
			index += 1;
			continue;
		}
		// Positional argument = file path
		if (!arg.startsWith("-")) {
			if (options.file) throw new Error("Multiple file arguments provided");
			options.file = arg;
			continue;
		}
		throw new Error(`Unknown option: ${arg}`);
	}
	return options;
}

function usage() {
	return [
		"Rex expression language CLI.",
		"",
		"Usage:",
		"  rex                            Start interactive REPL",
		"  rex input.rex                  Evaluate a Rex script (JSON output)",
		"  rex --expr '1 + 2'             Evaluate an inline expression",
		"  cat input.rex | rex            Evaluate from stdin",
		"  rex -c input.rex               Compile to rexc bytecode",
		"",
		"Input:",
		"  <file>                Evaluate/compile a Rex source file",
		"  -e, --expr <source>   Evaluate/compile an inline expression",
		"  -f, --file <path>     Evaluate/compile source from a file",
		"",
		"Output mode:",
		"  (default)             Evaluate and output result as JSON",
		"  -c, --compile         Compile to rexc bytecode",
		"      --ir              Output lowered IR as JSON",
		"",
		"Compile options:",
		"  -m, --minify-names    Minify local variable names",
		"      --dedupe-values   Deduplicate large repeated values",
		"      --dedupe-min-bytes <n>  Minimum bytes for dedupe (default: 4)",
		"",
		"General:",
		"  -o, --out <path>      Write output to file instead of stdout",
		"  -h, --help            Show this message",
	].join("\n");
}

async function readStdin(): Promise<string> {
	const chunks: Buffer[] = [];
	for await (const chunk of process.stdin) {
		chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
	}
	return Buffer.concat(chunks).toString("utf8");
}

async function resolveSource(options: CliOptions): Promise<string> {
	if (options.expr && options.file) throw new Error("Use only one of --expr, --file, or a positional file path");
	if (options.expr) return options.expr;
	if (options.file) return readFile(options.file, "utf8");
	if (!process.stdin.isTTY) {
		const piped = await readStdin();
		if (piped.trim().length > 0) return piped;
	}
	throw new Error("No input provided. Use a file path, --expr, or pipe source via stdin.");
}

async function loadDomainConfigFromFolder(folderPath: string): Promise<unknown | undefined> {
	const configPath = resolve(folderPath, ".config.rex");
	try {
		return parse(await readFile(configPath, "utf8"));
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
		throw error;
	}
}

async function resolveDomainConfig(options: CliOptions): Promise<unknown | undefined> {
	const baseFolder = options.file ? dirname(resolve(options.file)) : process.cwd();
	return loadDomainConfigFromFolder(baseFolder);
}

async function main() {
	const options = parseArgs(process.argv.slice(2));
	if (options.help) {
		console.log(usage());
		return;
	}

	// No source provided on a TTY → launch interactive REPL
	const hasSource = options.expr || options.file || !process.stdin.isTTY;
	if (!hasSource && !options.compile && !options.ir) {
		const { startRepl } = await import("./rex-repl.ts");
		await startRepl();
		return;
	}

	const source = await resolveSource(options);

	let output: string;
	if (options.ir) {
		output = JSON.stringify(parseToIR(source), null, 2);
	} else if (options.compile) {
		const domainConfig = await resolveDomainConfig(options);
		output = compile(source, {
			minifyNames: options.minifyNames,
			dedupeValues: options.dedupeValues,
			dedupeMinBytes: options.dedupeMinBytes,
			domainConfig,
		});
	} else {
		const { value } = evaluateSource(source);
		output = JSON.stringify(value, null, 2);
	}

	if (options.out) {
		await writeFile(options.out, `${output}\n`, "utf8");
		return;
	}
	console.log(output);
}

await main().catch((error) => {
	const message = error instanceof Error ? error.message : String(error);
	console.error(`rex: ${message}`);
	process.exit(1);
});

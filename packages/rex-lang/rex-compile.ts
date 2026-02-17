import { compile, parseToIR } from "./rex.ts";

type CliOptions = {
	expr?: string;
	file?: string;
	out?: string;
	ir: boolean;
	help: boolean;
};

function parseArgs(argv: string[]): CliOptions {
	const options: CliOptions = { ir: false, help: false };
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (!arg) continue;
		if (arg === "--help" || arg === "-h") {
			options.help = true;
			continue;
		}
		if (arg === "--ir") {
			options.ir = true;
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
		throw new Error(`Unknown option: ${arg}`);
	}
	return options;
}

function usage() {
	return [
		"Compile high-level Rex to compact encoding (rexc).",
		"",
		"Usage:",
		"  bun run rex:compile --expr \"when x do y end\"",
		"  bun run rex:compile --file input.rex",
		"  cat input.rex | bun run rex:compile",
		"",
		"Options:",
		"  -e, --expr <source>   Compile an inline expression/program",
		"  -f, --file <path>     Compile source from a file",
		"  -o, --out <path>      Write output to file instead of stdout",
		"      --ir              Output lowered IR JSON instead of compact encoding",
		"  -h, --help            Show this message",
	].join("\n");
}

async function readStdin(): Promise<string> {
	return Bun.stdin.text();
}

async function resolveSource(options: CliOptions): Promise<string> {
	if (options.expr && options.file) throw new Error("Use only one of --expr or --file");
	if (options.expr) return options.expr;
	if (options.file) return Bun.file(options.file).text();
	if (!process.stdin.isTTY) {
		const piped = await readStdin();
		if (piped.trim().length > 0) return piped;
	}
	throw new Error("No input provided. Use --expr, --file, or pipe source via stdin.");
}

async function main() {
	const options = parseArgs(Bun.argv.slice(2));
	if (options.help) {
		console.log(usage());
		return;
	}

	const source = await resolveSource(options);
	const output = options.ir
		? JSON.stringify(parseToIR(source), null, 2)
		: compile(source);

	if (options.out) {
		await Bun.write(options.out, `${output}\n`);
		return;
	}
	console.log(output);
}

await main().catch((error) => {
	const message = error instanceof Error ? error.message : String(error);
	console.error(`rex:compile error: ${message}`);
	process.exit(1);
});

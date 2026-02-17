import { compile, parseToIR } from "./rex.ts";
import { dirname, resolve } from "node:path";
import { readFile, writeFile } from "node:fs/promises";

type CliOptions = {
	expr?: string;
	file?: string;
	out?: string;
	ir: boolean;
	minifyNames: boolean;
	dedupeValues: boolean;
	dedupeMinBytes?: number;
	domainRefs: Record<string, number>;
	help: boolean;
};

type DomainSchema = {
	globals?: Record<string, unknown>;
};

function parseArgs(argv: string[]): CliOptions {
	const options: CliOptions = {
		ir: false,
		minifyNames: false,
		dedupeValues: false,
		domainRefs: {},
		help: false,
	};
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
		if (arg === "--domain-extension") {
			const value = argv[index + 1];
			if (!value) throw new Error("Missing value for --domain-extension");
			options.domainRefs[value] = 0;
			index += 1;
			continue;
		}
		if (arg === "--domain-ref") {
			const value = argv[index + 1];
			if (!value) throw new Error("Missing value for --domain-ref");
			const separator = value.indexOf("=");
			if (separator < 1 || separator === value.length - 1) {
				throw new Error("--domain-ref expects NAME=ID (for example: headers=0)");
			}
			const name = value.slice(0, separator);
			const idText = value.slice(separator + 1);
			const id = Number(idText);
			if (!Number.isInteger(id) || id < 0) throw new Error(`Invalid domain ref id in --domain-ref '${value}'`);
			options.domainRefs[name] = id;
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
		throw new Error(`Unknown option: ${arg}`);
	}
	return options;
}

function usage() {
	return [
		"Compile high-level Rex to compact encoding (rexc).",
		"",
		"Usage:",
		"  rex --expr \"when x do y end\"",
		"  rex --file input.rex",
		"  cat input.rex | rex",
		"",
		"(Repo script alternative: bun run rex:compile --expr \"when x do y end\")",
		"",
		"Options:",
		"  -e, --expr <source>   Compile an inline expression/program",
		"  -f, --file <path>     Compile source from a file",
		"  -o, --out <path>      Write output to file instead of stdout",
		"      --ir              Output lowered IR JSON instead of compact encoding",
		"  -m, --minify-names    Minify local variable names in compiled output",
		"      --dedupe-values   Deduplicate large repeated values using forward pointers",
		"      --dedupe-min-bytes <n>  Minimum encoded value bytes for pointer dedupe (default: 4)",
		"      --domain-extension <name>  Map domain symbol name to ref 0 (apostrophe)",
		"      --domain-ref <name=id>    Map domain symbol name to a specific ref id",
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
	if (options.expr && options.file) throw new Error("Use only one of --expr or --file");
	if (options.expr) return options.expr;
	if (options.file) return readFile(options.file, "utf8");
	if (!process.stdin.isTTY) {
		const piped = await readStdin();
		if (piped.trim().length > 0) return piped;
	}
	throw new Error("No input provided. Use --expr, --file, or pipe source via stdin.");
}

async function loadDomainRefsFromFolder(folderPath: string): Promise<Record<string, number>> {
	const schemaPath = resolve(folderPath, "rex-domain.json");
	let parsed: DomainSchema;
	try {
		parsed = JSON.parse(await readFile(schemaPath, "utf8")) as DomainSchema;
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return {};
		throw error;
	}
	if (!parsed || typeof parsed !== "object" || !parsed.globals || typeof parsed.globals !== "object") {
		throw new Error(`Invalid rex-domain.json at ${schemaPath}: expected { globals: { ... } }`);
	}

	const refs: Record<string, number> = {};
	let nextRef = 0;
	for (const name of Object.keys(parsed.globals)) {
		refs[name] = nextRef;
		nextRef += 1;
	}
	return refs;
}

async function resolveDomainRefs(options: CliOptions): Promise<Record<string, number>> {
	const baseFolder = options.file ? dirname(resolve(options.file)) : process.cwd();
	const autoRefs = await loadDomainRefsFromFolder(baseFolder);
	return { ...autoRefs, ...options.domainRefs };
}

async function main() {
	const options = parseArgs(process.argv.slice(2));
	if (options.help) {
		console.log(usage());
		return;
	}

	const source = await resolveSource(options);
	const domainRefs = await resolveDomainRefs(options);
	const output = options.ir
		? JSON.stringify(parseToIR(source), null, 2)
		: compile(source, {
			minifyNames: options.minifyNames,
			dedupeValues: options.dedupeValues,
			dedupeMinBytes: options.dedupeMinBytes,
			domainRefs,
		});

	if (options.out) {
		await writeFile(options.out, `${output}\n`, "utf8");
		return;
	}
	console.log(output);
}

await main().catch((error) => {
	const message = error instanceof Error ? error.message : String(error);
	console.error(`rex:compile error: ${message}`);
	process.exit(1);
});

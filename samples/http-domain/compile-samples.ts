import { readdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join, extname, basename } from "node:path";
import { compile } from "../../packages/rex-lang/rex.ts";

const samplesDir = fileURLToPath(new URL(".", import.meta.url));

type DomainSchema = {
	globals?: Record<string, unknown>;
};

async function loadDomainRefs(dirPath: string): Promise<Record<string, number>> {
	const schemaPath = join(dirPath, "rex-domain.json");
	try {
		const raw = await readFile(schemaPath, "utf8");
		const parsed = JSON.parse(raw) as DomainSchema;
		if (!parsed || typeof parsed !== "object" || !parsed.globals || typeof parsed.globals !== "object") {
			throw new Error("Expected { globals: { ... } }");
		}

		const refs: Record<string, number> = {};
		let nextRef = 0;
		for (const name of Object.keys(parsed.globals)) {
			refs[name] = nextRef;
			nextRef += 1;
		}
		return refs;
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return {};
		throw new Error(`Failed to load rex-domain.json in ${dirPath}: ${(error as Error).message}`);
	}
}

async function main() {
	const domainRefs = await loadDomainRefs(samplesDir);
	const files = await collectRexFiles(samplesDir);
	let compiled = 0;

	for (const filePath of files) {
		const source = await readFile(filePath, "utf8");
		const debugOut = compile(source, { domainRefs });
		const optimizedOut = compile(source, { optimize: true, minifyNames: true, dedupeValues: true, domainRefs });

		const outBase = filePath.slice(0, -extname(filePath).length);
		await writeFile(`${outBase}.rexc`, `${debugOut}\n`, "utf8");
		await writeFile(`${outBase}.opt.rexc`, `${optimizedOut}\n`, "utf8");
		compiled += 1;
	}

	console.log(`Compiled ${compiled} Rex sample file(s).`);
}

async function collectRexFiles(dirPath: string): Promise<string[]> {
	const entries = await readdir(dirPath, { withFileTypes: true });
	const files: string[] = [];

	for (const entry of entries) {
		const fullPath = join(dirPath, entry.name);
		if (entry.isDirectory()) {
			files.push(...(await collectRexFiles(fullPath)));
			continue;
		}
		if (!entry.isFile()) continue;
		if (!entry.name.endsWith(".rex")) continue;
		if (entry.name.endsWith(".test.rex")) continue;
		if (basename(entry.name).startsWith(".")) continue;
		files.push(fullPath);
	}

	files.sort((a, b) => a.localeCompare(b));
	return files;
}

main().catch((error) => {
	console.error("Failed to compile sample rex files:", error);
	process.exit(1);
});

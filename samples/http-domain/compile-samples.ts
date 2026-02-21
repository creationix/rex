import { readdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join, extname, basename } from "node:path";
import { compile, parse } from "../../packages/rex-lang/rex.ts";

const samplesDir = fileURLToPath(new URL(".", import.meta.url));

async function loadDomainConfig(dirPath: string): Promise<unknown | undefined> {
	const configPath = join(dirPath, ".config.rex");
	try {
		return parse(await readFile(configPath, "utf8"));
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
		throw new Error(`Failed to load .config.rex in ${dirPath}: ${(error as Error).message}`);
	}
}

async function main() {
	const domainConfig = await loadDomainConfig(samplesDir);
	const files = await collectRexFiles(samplesDir);
	let compiled = 0;

	for (const filePath of files) {
		const source = await readFile(filePath, "utf8");
		const debugOut = compile(source, { domainConfig });

		const outBase = filePath.slice(0, -extname(filePath).length);
		await writeFile(`${outBase}.rexc`, `${debugOut}\n`, "utf8");
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

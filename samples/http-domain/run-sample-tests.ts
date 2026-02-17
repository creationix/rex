import { readFile, readdir } from "node:fs/promises";
import { basename, dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { evaluateSource } from "../../packages/rex-lang/rexc-interpreter.ts";

type TestCase = {
	name?: string;
	input?: {
		vars?: Record<string, unknown>;
		refs?: Record<string, unknown>;
		self?: unknown;
		selfStack?: unknown[];
	};
	expect?: {
		value?: unknown;
		vars?: Record<string, unknown>;
		refs?: Record<string, unknown>;
	};
};

type TestDoc = {
	program?: string;
	cases?: TestCase[];
};

const samplesDir = fileURLToPath(new URL(".", import.meta.url));

async function main() {
	const testFiles = await collectTestFiles(samplesDir);
	if (testFiles.length === 0) {
		console.log("No .test.rex files found.");
		return;
	}

	let passed = 0;
	let failed = 0;

	for (const testFile of testFiles) {
		const relName = testFile.slice(samplesDir.length).replace(/^[/\\]/, "");
		const { programPath, cases } = await loadTestDoc(testFile);
		const programSource = await readFile(programPath, "utf8");

		for (let index = 0; index < cases.length; index += 1) {
			const testCase = cases[index] ?? {};
			const label = testCase.name ?? `case-${index + 1}`;
			const ctx = {
				vars: testCase.input?.vars ?? {},
				refs: normalizeRefs(testCase.input?.refs ?? {}),
				self: testCase.input?.self,
				selfStack: testCase.input?.selfStack,
			};

			const { value, state } = evaluateSource(programSource, ctx);

			const checks: string[] = [];
			if (testCase.expect && "value" in testCase.expect) {
				if (!isDeepStrictEqual(value, testCase.expect.value)) {
					checks.push(
						`value mismatch expected=${formatValue(testCase.expect.value)} actual=${formatValue(value)}`,
					);
				}
			}

			if (testCase.expect?.vars) {
				for (const [key, expected] of Object.entries(testCase.expect.vars)) {
					const actual = state.vars[key];
					if (!isDeepStrictEqual(actual, expected)) {
						checks.push(
							`vars.${key} mismatch expected=${formatValue(expected)} actual=${formatValue(actual)}`,
						);
					}
				}
			}

			if (testCase.expect?.refs) {
				for (const [key, expected] of Object.entries(testCase.expect.refs)) {
					const actual = state.refs[key];
					if (!isDeepStrictEqual(actual, expected)) {
						checks.push(
							`refs.${key} mismatch expected=${formatValue(expected)} actual=${formatValue(actual)}`,
						);
					}
				}
			}

			if (checks.length === 0) {
				console.log(`PASS ${relName} :: ${label}`);
				passed += 1;
			} else {
				console.log(`FAIL ${relName} :: ${label}`);
				for (const check of checks) console.log(`  - ${check}`);
				failed += 1;
			}
		}
	}

	console.log(`\nSummary: ${passed} passed, ${failed} failed.`);
	if (failed > 0) process.exit(1);
}

async function loadTestDoc(testFilePath: string): Promise<{ programPath: string; cases: TestCase[] }> {
	const raw = await readFile(testFilePath, "utf8");
	const doc = evaluateSource(raw).value as TestDoc;
	if (!doc || typeof doc !== "object") {
		throw new Error(`Test doc ${testFilePath} did not evaluate to an object`);
	}

	const testDir = dirname(testFilePath);
	const inferredProgram = basename(testFilePath).replace(/\.test\.rex$/, ".rex");
	const programPath = resolve(testDir, doc.program ?? inferredProgram);
	const cases = Array.isArray(doc.cases) ? doc.cases : [];
	if (cases.length === 0) {
		throw new Error(`Test doc ${testFilePath} has no cases`);
	}

	return { programPath, cases };
}

function normalizeRefs(refs: Record<string, unknown>): Record<number, unknown> {
	const out: Record<number, unknown> = {};
	for (const [key, value] of Object.entries(refs)) {
		const numeric = Number(key);
		if (Number.isFinite(numeric)) out[numeric] = value;
	}
	return out;
}

async function collectTestFiles(dirPath: string): Promise<string[]> {
	const entries = await readdir(dirPath, { withFileTypes: true });
	const files: string[] = [];

	for (const entry of entries) {
		const fullPath = join(dirPath, entry.name);
		if (entry.isDirectory()) {
			files.push(...(await collectTestFiles(fullPath)));
			continue;
		}
		if (!entry.isFile()) continue;
		if (!entry.name.endsWith(".test.rex")) continue;
		files.push(fullPath);
	}

	files.sort((a, b) => a.localeCompare(b));
	return files;
}

function formatValue(value: unknown): string {
	try {
		return JSON.stringify(value);
	} catch {
		return String(value);
	}
}

main().catch((error) => {
	console.error("Failed to run sample tests:", error);
	process.exit(1);
});

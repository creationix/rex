import { readFile, readdir } from "node:fs/promises";
import { basename, dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { compile, parse } from "../../packages/rex-lang/rex.ts";
import { evaluateRexc } from "../../packages/rex-lang/rexc-interpreter.ts";

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
	const domainConfig = await loadDomainConfig(samplesDir);
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
		const programRexc = compile(programSource, { domainConfig, optimize: true });
		const { rexcPath, optRexcPath } = toRexcPaths(programPath);
		const programRexcFile = await readOptionalFile(rexcPath);
		const programOptRexcFile = await readOptionalFile(optRexcPath);

		for (let index = 0; index < cases.length; index += 1) {
			const testCase = cases[index] ?? {};
			const label = testCase.name ?? `case-${index + 1}`;
			const hasAssertions = !!(
				testCase.expect
				&& (
					"value" in testCase.expect
					|| (testCase.expect.vars && Object.keys(testCase.expect.vars).length > 0)
					|| (testCase.expect.refs && Object.keys(testCase.expect.refs).length > 0)
				)
			);
			if (!hasAssertions) {
				throw new Error(`Test case '${label}' in ${relName} has no assertions (expect.value/vars/refs required)`);
			}
			const ctx = {
				vars: testCase.input?.vars ?? {},
				refs: normalizeRefs(testCase.input?.refs ?? {}),
				self: testCase.input?.self,
				selfStack: testCase.input?.selfStack,
			};

			const compiledResult = evaluateRexc(programRexc, ctx);
			const { value, state } = compiledResult;

			const compareErrors: string[] = [];
			if (programRexcFile) {
				compareErrors.push(
					...compareRexcOutput(programRexcFile, ctx, compiledResult, "rexc"),
				);
			}
			if (programOptRexcFile) {
				compareErrors.push(
					...compareRexcOutput(programOptRexcFile, ctx, compiledResult, "opt.rexc"),
				);
			}

			const checks: string[] = [];
			if (testCase.expect && "value" in testCase.expect) {
				if (!isDeepStrictEqual(normalizeComparable(value), normalizeComparable(testCase.expect.value))) {
					checks.push(
						`value mismatch expected=${formatValue(testCase.expect.value)} actual=${formatValue(value)}`,
					);
				}
			}

			if (testCase.expect?.vars) {
				for (const [key, expected] of Object.entries(testCase.expect.vars)) {
					const actual = state.vars[key];
					if (!isDeepStrictEqual(normalizeComparable(actual), normalizeComparable(expected))) {
						checks.push(
							`vars.${key} mismatch expected=${formatValue(expected)} actual=${formatValue(actual)}`,
						);
					}
				}
			}

			if (testCase.expect?.refs) {
				for (const [key, expected] of Object.entries(testCase.expect.refs)) {
					const actual = state.refs[key];
					if (!isDeepStrictEqual(normalizeComparable(actual), normalizeComparable(expected))) {
						checks.push(
							`refs.${key} mismatch expected=${formatValue(expected)} actual=${formatValue(actual)}`,
						);
					}
				}
			}

			checks.push(...compareErrors);
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
	const doc = evaluateRexc(compile(raw)).value as TestDoc;
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

function toRexcPaths(programPath: string): { rexcPath: string; optRexcPath: string } {
	const base = programPath.replace(/\.rex$/, "");
	return { rexcPath: `${base}.rexc`, optRexcPath: `${base}.opt.rexc` };
}

async function readOptionalFile(filePath: string): Promise<string | undefined> {
	try {
		return await readFile(filePath, "utf8");
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
		throw error;
	}
}

function compareRexcOutput(
	program: string,
	ctx: { vars?: Record<string, unknown>; refs?: Record<string, unknown>; self?: unknown; selfStack?: unknown[] },
	baseline: { value: unknown; state: { vars: Record<string, unknown>; refs: Record<string, unknown> } },
	label: string,
): string[] {
	try {
		const result = evaluateRexc(program, ctx);
		const checks: string[] = [];
		if (!isDeepStrictEqual(normalizeComparable(result.value), normalizeComparable(baseline.value))) {
			checks.push(`${label} value mismatch (expected compiled output)`);
		}
		if (!isDeepStrictEqual(normalizeComparable(result.state.vars), normalizeComparable(baseline.state.vars))) {
			checks.push(`${label} vars mismatch (expected compiled output)`);
		}
		if (!isDeepStrictEqual(normalizeComparable(result.state.refs), normalizeComparable(baseline.state.refs))) {
			checks.push(`${label} refs mismatch (expected compiled output)`);
		}
		return checks;
	} catch (error) {
		const message = error instanceof Error ? error.message : String(error);
		return [`${label} failed to execute: ${message}`];
	}
}

async function loadDomainConfig(dirPath: string): Promise<unknown> {
	const configPath = resolve(dirPath, ".config.rex");
	const raw = await readFile(configPath, "utf8");
	return parse(raw);
}

function normalizeRefs(refs: Record<string, unknown>): Record<string, unknown> {
	return { ...refs };
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

function normalizeComparable(value: unknown): unknown {
	if (Array.isArray(value)) {
		return value.map((item) => normalizeComparable(item));
	}
	if (value && typeof value === "object") {
		const out: Record<string, unknown> = {};
		for (const [key, item] of Object.entries(value as Record<string, unknown>)) {
			const normalized = normalizeComparable(item);
			if (normalized !== undefined) out[key] = normalized;
		}
		return out;
	}
	return value;
}

main().catch((error) => {
	console.error("Failed to run sample tests:", error);
	process.exit(1);
});

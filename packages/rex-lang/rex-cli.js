#!/usr/bin/env node

import { spawn } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const compileScript = join(here, "rex-compile.ts");
const passthroughArgs = process.argv.slice(2);

if (typeof Bun !== "undefined") {
	await import("./rex-compile.ts");
	process.exit(0);
}

const [majorText, minorText] = process.versions.node.split(".");
const major = Number(majorText);
const minor = Number(minorText);

if (!Number.isFinite(major) || !Number.isFinite(minor) || major < 22 || (major === 22 && minor < 18)) {
	console.error("rex: Node.js v22.18+ is required to run TypeScript natively, or use Bun.");
	process.exit(1);
}

const child = spawn(process.execPath, [compileScript, ...passthroughArgs], {
	stdio: "inherit",
});

child.on("error", (error) => {
	const message = error instanceof Error ? error.message : String(error);
	console.error("rex: failed to launch runtime.");
	console.error("Use Node.js v22.18+ or Bun.");
	console.error(`Details: ${message}`);
	process.exit(1);
});

child.on("exit", (code, signal) => {
	if (typeof code === "number") process.exit(code);
	if (signal) {
		console.error(`rex: process terminated by signal ${signal}`);
		process.exit(1);
	}
	process.exit(1);
});

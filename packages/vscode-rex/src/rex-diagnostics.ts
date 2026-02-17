import { grammar } from "../../rex-lang/rex.ts";

export interface RexParseFailure {
	message: string;
	startOffset: number;
	endOffset: number;
	line: number;
	column: number;
}

export function getRexParseFailure(source: string): RexParseFailure | null {
	const match = grammar.match(source);
	if (match.succeeded()) return null;

	const startOffset = Math.max(0, match.getRightmostFailurePosition());
	const endOffset = startOffset < source.length ? startOffset + 1 : startOffset;
	const { line, column } = offsetToLineColumn(source, startOffset);

	return {
		message: match.shortMessage,
		startOffset,
		endOffset,
		line,
		column,
	};
}

function offsetToLineColumn(source: string, offset: number): { line: number; column: number } {
	let line = 1;
	let column = 1;

	for (let index = 0; index < offset && index < source.length; index += 1) {
		const char = source[index];
		if (char === "\n") {
			line += 1;
			column = 1;
			continue;
		}
		if (char === "\r") {
			if (source[index + 1] === "\n") index += 1;
			line += 1;
			column = 1;
			continue;
		}
		column += 1;
	}

	return { line, column };
}

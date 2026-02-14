// Digit alphabet: 0-9 a-z A-Z - _
const DIGIT_SET = new Set(
	"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_",
);

// Tags that can start a value (excludes closing delimiters)
const VALUE_TAGS = new Set("+*:%@$^~=([{,?!|&");

export const TOKEN_TYPES = [
	"byteLength", // 0 - length prefixes
	"keyword", // 1 - opcodes (%), control flow (?( !( |( &()
	"function", // 2 - (unused, kept for token index stability)
	"variable", // 3 - variables ($)
	"number", // 4 - integers (+), decimals (*)
	"string", // 5 - bare strings (:), string containers (,)
	"operator", // 6 - set (=), delete (~), pointer (^)
	"property", // 7 - references (@)
	"decorator", // 8 - index marker (#)
	"annotation", // 9 - line-drawing annotations
	"objectKey", // 10 - object keys
	"indexData", // 11 - pointer array index data
] as const;

const T = {
	byteLength: 0,
	keyword: 1,
	function: 2,
	variable: 3,
	number: 4,
	string: 5,
	operator: 6,
	property: 7,
	decorator: 8,
	annotation: 9,
	objectKey: 10,
	indexData: 11,
} as const;

export interface Token {
	line: number;
	char: number;
	length: number;
	type: number;
}

function digitValue(c: number): number {
	if (c >= 48 && c <= 57) return c - 48; // 0-9
	if (c >= 97 && c <= 122) return c - 97 + 10; // a-z
	if (c >= 65 && c <= 90) return c - 65 + 36; // A-Z
	if (c === 45) return 62; // -
	if (c === 95) return 63; // _
	return -1;
}

function decodePrefix(text: string, start: number, end: number): number {
	let value = 0;
	for (let i = start; i < end; i++) {
		value = value * 64 + digitValue(text.charCodeAt(i));
	}
	return value;
}

export function tokenize(text: string): Token[] {
	const tokens: Token[] = [];
	let pos = 0;
	let line = 0;
	let col = 0;

	function advance() {
		if (text.charCodeAt(pos) === 10) {
			line++;
			col = 0;
		} else {
			col++;
		}
		pos++;
	}

	function emit(l: number, c: number, len: number, type: number) {
		if (len > 0) tokens.push({ line: l, char: c, length: len, type });
	}

	function skipNonCode() {
		while (pos < text.length) {
			const c = text.charCodeAt(pos);
			// Whitespace
			if (c === 32 || c === 9 || c === 10 || c === 13) {
				advance();
				continue;
			}
			// Annotation — line-drawing character (Box Drawing U+2500–U+257F, Block Elements U+2580–U+259F)
			if (c >= 0x2500 && c <= 0x259f) {
				const artLine = line;
				while (pos < text.length && text.charCodeAt(pos) !== 10) {
					if (text.charCodeAt(pos) >= 0x2500 && text.charCodeAt(pos) <= 0x259f) {
						const runCol = col;
						const runStart = pos;
						while (pos < text.length && text.charCodeAt(pos) >= 0x2500 && text.charCodeAt(pos) <= 0x259f)
							advance();
						emit(artLine, runCol, pos - runStart, T.annotation);
					} else {
						advance();
					}
				}
				continue;
			}
			// Line comment //
			if (
				c === 47 &&
				pos + 1 < text.length &&
				text.charCodeAt(pos + 1) === 47
			) {
				while (pos < text.length && text.charCodeAt(pos) !== 10) advance();
				continue;
			}
			// Block comment /*
			if (
				c === 47 &&
				pos + 1 < text.length &&
				text.charCodeAt(pos + 1) === 42
			) {
				advance();
				advance();
				while (pos < text.length) {
					if (
						text.charCodeAt(pos) === 42 &&
						pos + 1 < text.length &&
						text.charCodeAt(pos + 1) === 47
					) {
						advance();
						advance();
						break;
					}
					advance();
				}
				continue;
			}
			break;
		}
	}

	let objectKeyMode = false;

	// Try to parse index header (count#width+data) at current position inside [ or {
	function tryParseIndex() {
		const indexStart = pos;
		const indexLine = line;
		const indexCol = col;
		// Read count digits
		while (pos < text.length && DIGIT_SET.has(text[pos])) advance();
		// Must see #
		if (pos < text.length && text[pos] === "#") {
			const count = decodePrefix(text, indexStart, pos);
			advance(); // skip #
			// Read pointer width digit (part of metadata group)
			let width = 0;
			if (pos < text.length && DIGIT_SET.has(text[pos])) {
				width = digitValue(text.charCodeAt(pos));
				advance();
			}
			// Emit count#width as single modifier token
			emit(indexLine, indexCol, pos - indexStart, T.decorator);
			if (width > 0) {
				// Read count × width digits of index data
				const arrLen = count * width;
				const arrLine = line;
				const arrCol = col;
				const arrStart = pos;
				for (let i = 0; i < arrLen && pos < text.length; i++) advance();
				if (pos > arrStart)
					emit(arrLine, arrCol, pos - arrStart, T.indexData);
			}
		} else {
			// Not an index — rewind
			pos = indexStart;
			line = indexLine;
			col = indexCol;
		}
	}

	function parseOneValue() {
		skipNonCode();
		if (pos >= text.length) return;

		// Bail on closing delimiters
		const first = text[pos];
		if (first === ")" || first === "]" || first === "}")
			return;

		// Read digit prefix
		const prefixStart = pos;
		const prefixLine = line;
		const prefixCol = col;
		while (pos < text.length && DIGIT_SET.has(text[pos])) advance();

		if (pos >= text.length) return; // orphan digits at end

		const tag = text[pos];
		if (!VALUE_TAGS.has(tag)) return; // not a valid tag — bail

		const tagLine = line;
		const tagCol = col;

		switch (tag) {
			// Control-flow containers: ?( !( |( &(
			case "?":
			case "!":
			case "|":
			case "&": {
				// Peek for ( after the tag
				if (pos + 1 < text.length && text[pos + 1] === "(") {
					if (pos > prefixStart)
						emit(prefixLine, prefixCol, pos - prefixStart, T.byteLength);
					advance(); // skip tag char (? ! | &)
					advance(); // skip (
					emit(tagLine, tagCol, 2, T.keyword);
					while (pos < text.length) {
						skipNonCode();
						if (pos >= text.length || text[pos] === ")") break;
						const prev = pos;
						parseOneValue();
						if (pos === prev) advance();
					}
					if (pos < text.length && text[pos] === ")") advance();
				}
				// Bare ? ! | & without ( — not valid in new format, skip
				break;
			}

			// Call container
			case "(": {
				if (pos > prefixStart)
					emit(prefixLine, prefixCol, pos - prefixStart, T.byteLength);
				advance(); // open delimiter
				while (pos < text.length) {
					skipNonCode();
					if (pos >= text.length || text[pos] === ")") break;
					const prev = pos;
					parseOneValue();
					if (pos === prev) advance();
				}
				if (pos < text.length && text[pos] === ")") advance();
				break;
			}

			// Array container — check for index inside
			case "[": {
				if (pos > prefixStart)
					emit(prefixLine, prefixCol, pos - prefixStart, T.byteLength);
				advance(); // open delimiter
				skipNonCode();
				tryParseIndex();
				while (pos < text.length) {
					skipNonCode();
					if (pos >= text.length || text[pos] === "]") break;
					const prev = pos;
					parseOneValue();
					if (pos === prev) advance();
				}
				if (pos < text.length && text[pos] === "]") advance();
				break;
			}

			// Object container — alternates key/value, check for index inside
			case "{": {
				if (pos > prefixStart)
					emit(prefixLine, prefixCol, pos - prefixStart, T.byteLength);
				advance(); // open delimiter
				skipNonCode();
				tryParseIndex();
				let isKey = true;
				while (pos < text.length) {
					skipNonCode();
					if (pos >= text.length || text[pos] === "}") break;
					const prev = pos;
					const saved = objectKeyMode;
					objectKeyMode = isKey;
					parseOneValue();
					objectKeyMode = saved;
					if (pos === prev) advance();
					isKey = !isKey;
				}
				if (pos < text.length && text[pos] === "}") advance();
				break;
			}

			// String container — decode byte length and skip that many bytes
			case ",": {
				const byteLen = decodePrefix(text, prefixStart, pos);
				if (pos > prefixStart)
					emit(prefixLine, prefixCol, pos - prefixStart, T.byteLength);
				advance(); // skip ,
				emit(tagLine, tagCol, 1, T.string);
				if (byteLen > 0) {
					let cLine = line;
					let cCol = col;
					let cLen = 0;
					for (let i = 0; i < byteLen && pos < text.length; i++) {
						if (text.charCodeAt(pos) === 10) {
							if (cLen > 0) emit(cLine, cCol, cLen, T.string);
							advance();
							cLine = line;
							cCol = col;
							cLen = 0;
						} else {
							advance();
							cLen++;
						}
					}
					if (cLen > 0) emit(cLine, cCol, cLen, T.string);
				}
				break;
			}

			// Set operator — fixed arity 2 (place, value)
			case "=":
				if (pos > prefixStart)
					emit(prefixLine, prefixCol, pos - prefixStart, T.byteLength);
				advance();
				emit(tagLine, tagCol, 1, T.operator);
				parseOneValue(); // place
				parseOneValue(); // value
				break;

			// Delete operator — fixed arity 1 (place)
			case "~":
				if (pos > prefixStart)
					emit(prefixLine, prefixCol, pos - prefixStart, T.byteLength);
				advance();
				emit(tagLine, tagCol, 1, T.operator);
				parseOneValue(); // place
				break;

			// Decimal — consuming tag, emit then parse next value (significand)
			case "*":
				advance();
				emit(prefixLine, prefixCol, pos - prefixStart, T.number);
				parseOneValue();
				break;

			// Integer (zigzag encoded)
			case "+":
				advance();
				emit(prefixLine, prefixCol, pos - prefixStart, T.number);
				break;

			// Bare string
			case ":":
				advance();
				emit(prefixLine, prefixCol, pos - prefixStart, objectKeyMode ? T.objectKey : T.string);
				break;

			// Opcode
			case "%":
				advance();
				emit(prefixLine, prefixCol, pos - prefixStart, T.keyword);
				break;

			// Reference
			case "@":
				advance();
				emit(prefixLine, prefixCol, pos - prefixStart, T.property);
				break;

			// Variable
			case "$":
				advance();
				emit(prefixLine, prefixCol, pos - prefixStart, T.variable);
				break;

			// Pointer
			case "^":
				advance();
				emit(prefixLine, prefixCol, pos - prefixStart, T.operator);
				break;
		}
	}

	// Main loop: parse top-level values
	while (pos < text.length) {
		skipNonCode();
		if (pos >= text.length) break;
		const prev = pos;
		parseOneValue();
		if (pos === prev) advance(); // skip unknown char
	}

	return tokens;
}

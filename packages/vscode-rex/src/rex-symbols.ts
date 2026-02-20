export interface RexScope {
	id: number;
	parentId: number | null;
	start: number;
	end: number;
}

export interface RexDefinition {
	name: string;
	start: number;
	end: number;
	scopeId: number;
	kind: "assign" | "loop";
}

export interface RexReference {
	name: string;
	start: number;
	end: number;
	scopeId: number;
}

export interface RexSymbolAnalysis {
	scopes: RexScope[];
	definitions: RexDefinition[];
	references: RexReference[];
}

export interface RexSymbolLocation {
	name: string;
	start: number;
	end: number;
}

type TokenType = "identifier" | "keyword" | "operator" | "punct";

interface Token {
	type: TokenType;
	value: string;
	start: number;
	end: number;
}

const KEYWORDS = new Set([
	"when",
	"unless",
	"else",
	"while",
	"for",
	"in",
	"of",
	"do",
	"end",
	"break",
	"continue",
	"and",
	"or",
	"nor",
	"true",
	"false",
	"null",
	"undefined",
	"nan",
	"inf",
	"self",
]);

const ASSIGNMENT_OPERATORS = new Set([":=", "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^="]);

export function analyzeRexSymbols(source: string): RexSymbolAnalysis {
	const tokens = tokenize(source);
	const scopes: RexScope[] = [{ id: 0, parentId: null, start: 0, end: source.length }];
	const definitions: RexDefinition[] = [];
	const tokenScopeIds: number[] = new Array(tokens.length).fill(0);
	const definitionStarts = new Set<number>();
	const scopeStack: number[] = [0];
	const pendingForBindings = new Map<number, Token[]>();

	for (let index = 0; index < tokens.length; index += 1) {
		const token = tokens[index];
		if (!token) continue;
		const currentScopeId = getCurrentScopeId(scopeStack);
		tokenScopeIds[index] = currentScopeId;

		if (token.type === "keyword" && token.value === "for") {
			const parsed = parseForHeader(tokens, index);
			if (parsed.doIndex >= 0 && parsed.bindings.length > 0) {
				pendingForBindings.set(parsed.doIndex, parsed.bindings);
			}
		}

		if (token.type === "identifier" && isIdentifierAssignment(tokens, index)) {
			definitions.push({
				name: token.value,
				start: token.start,
				end: token.end,
				scopeId: currentScopeId,
				kind: "assign",
			});
			definitionStarts.add(token.start);
		}

		if (token.type === "keyword" && token.value === "do") {
			const newScopeId = scopes.length;
			scopes.push({
				id: newScopeId,
				parentId: currentScopeId,
				start: token.start,
				end: source.length,
			});
			scopeStack.push(newScopeId);

			const bindings = pendingForBindings.get(index);
			if (bindings) {
				for (const binding of bindings) {
					definitions.push({
						name: binding.value,
						start: binding.start,
						end: binding.end,
						scopeId: newScopeId,
						kind: "loop",
					});
					definitionStarts.add(binding.start);
				}
				pendingForBindings.delete(index);
			}
			continue;
		}

		if (token.type === "keyword" && token.value === "end") {
			if (scopeStack.length > 1) {
				const closingScopeId = scopeStack.pop()!;
				const scope = scopes[closingScopeId];
				if (scope) scope.end = token.end;
			}
		}
	}

	for (const scopeId of scopeStack) {
		const scope = scopes[scopeId];
		if (scope) scope.end = source.length;
	}

	const references: RexReference[] = [];
	for (let index = 0; index < tokens.length; index += 1) {
		const token = tokens[index];
		if (!token) continue;
		if (token.type !== "identifier") continue;
		if (definitionStarts.has(token.start)) continue;
		if (isObjectLiteralKey(tokens, index)) continue;
		if (isStaticNavigationSegment(tokens, index)) continue;

		references.push({
			name: token.value,
			start: token.start,
			end: token.end,
			scopeId: tokenScopeIds[index] ?? 0,
		});
	}

	definitions.sort((left, right) => left.start - right.start);
	return { scopes, definitions, references };
}

export function findDefinitionAtOffset(
	source: string,
	offset: number,
): RexSymbolLocation | null {
	const analysis = analyzeRexSymbols(source);
	const name = readIdentifierAt(source, offset);
	if (!name) return null;

	const currentScopeId = findInnermostScope(analysis.scopes, offset);
	const best = resolveDefinition(analysis, name, offset, currentScopeId);
	if (!best) return null;
	return { name: best.name, start: best.start, end: best.end };
}

export function findReferencesAtOffset(
	source: string,
	offset: number,
	includeDeclaration = true,
): RexSymbolLocation[] {
	const analysis = analyzeRexSymbols(source);
	const name = readIdentifierAt(source, offset);
	if (!name) return [];

	const currentScopeId = findInnermostScope(analysis.scopes, offset);
	const target = resolveDefinition(analysis, name, offset, currentScopeId);
	if (!target) return [];

	const locations: RexSymbolLocation[] = [];

	for (const definition of analysis.definitions) {
		if (definition.name !== name) continue;
		const resolved = resolveDefinition(
			analysis,
			name,
			definition.start,
			definition.scopeId,
		);
		if (!resolved || resolved.start !== target.start) continue;
		if (!includeDeclaration && definition.start === target.start) continue;
		locations.push({
			name: definition.name,
			start: definition.start,
			end: definition.end,
		});
	}

	for (const reference of analysis.references) {
		if (reference.name !== name) continue;
		const resolved = resolveDefinition(
			analysis,
			name,
			reference.start,
			reference.scopeId,
		);
		if (!resolved || resolved.start !== target.start) continue;
		locations.push({
			name: reference.name,
			start: reference.start,
			end: reference.end,
		});
	}

	locations.sort((left, right) => left.start - right.start);
	return locations;
}

function resolveDefinition(
	analysis: RexSymbolAnalysis,
	name: string,
	offset: number,
	currentScopeId: number,
): RexDefinition | null {
	const candidates = analysis.definitions
		.filter((definition) => definition.name === name)
		.filter((definition) => definition.start <= offset)
		.filter((definition) => isScopeVisible(analysis.scopes, definition.scopeId, currentScopeId));

	if (candidates.length === 0) return null;
	return candidates.reduce((latest, current) =>
		current.start > latest.start ? current : latest,
	);
}

function findInnermostScope(scopes: RexScope[], offset: number): number {
	let scopeId = 0;
	let currentStart = -1;
	for (const scope of scopes) {
		if (scope.start <= offset && offset <= scope.end && scope.start >= currentStart) {
			scopeId = scope.id;
			currentStart = scope.start;
		}
	}
	return scopeId;
}

function isScopeVisible(scopes: RexScope[], definitionScopeId: number, referenceScopeId: number): boolean {
	let current: number | null = referenceScopeId;
	while (current !== null) {
		if (current === definitionScopeId) return true;
		const currentScope: RexScope | undefined = scopes[current];
		if (!currentScope) return false;
		current = currentScope.parentId;
	}
	return false;
}

function readIdentifierAt(source: string, offset: number): string | null {
	if (offset < 0 || offset > source.length) return null;
	let start = offset;
	while (start > 0 && isIdentifierChar(source[start - 1])) start -= 1;
	let end = offset;
	while (end < source.length && isIdentifierChar(source[end])) end += 1;
	if (start === end) return null;
	const value = source.slice(start, end);
	if (!value || !isIdentifierStart(value.charAt(0))) return null;
	return value;
}

function isIdentifierAssignment(tokens: Token[], index: number): boolean {
	const next = tokens[index + 1];
	if (!next) return false;
	if (next.type !== "operator") return false;
	return ASSIGNMENT_OPERATORS.has(next.value);
}

function isObjectLiteralKey(tokens: Token[], index: number): boolean {
	const next = tokens[index + 1];
	return Boolean(next?.type === "punct" && next.value === ":");
}

function isStaticNavigationSegment(tokens: Token[], index: number): boolean {
	const prev = tokens[index - 1];
	if (!prev || prev.type !== "punct") return false;
	return prev.value === ".";
}

function parseForHeader(tokens: Token[], forIndex: number): { doIndex: number; bindings: Token[] } {
	let doIndex = -1;
	let depthParens = 0;
	let depthBrackets = 0;
	let depthBraces = 0;

	for (let index = forIndex + 1; index < tokens.length; index += 1) {
		const token = tokens[index];
		if (!token) continue;
		if (token.type === "punct") {
			if (token.value === "(") depthParens += 1;
			if (token.value === ")") depthParens = Math.max(0, depthParens - 1);
			if (token.value === "[") depthBrackets += 1;
			if (token.value === "]") depthBrackets = Math.max(0, depthBrackets - 1);
			if (token.value === "{") depthBraces += 1;
			if (token.value === "}") depthBraces = Math.max(0, depthBraces - 1);
		}
		if (
			depthParens === 0 &&
			depthBrackets === 0 &&
			depthBraces === 0 &&
			token.type === "keyword" &&
			token.value === "do"
		) {
			doIndex = index;
			break;
		}
	}
	if (doIndex < 0) return { doIndex: -1, bindings: [] };

	const first = tokens[forIndex + 1];
	if (!first || first.type !== "identifier") return { doIndex, bindings: [] };

	const second = tokens[forIndex + 2];
	const third = tokens[forIndex + 3];
	const fourth = tokens[forIndex + 4];

	if (
		second?.type === "punct" &&
		second.value === "," &&
		third?.type === "identifier" &&
		fourth?.type === "keyword" &&
		fourth.value === "in"
	) {
		return { doIndex, bindings: [first, third] };
	}

	if (second?.type === "keyword" && (second.value === "in" || second.value === "of")) {
		return { doIndex, bindings: [first] };
	}

	return { doIndex, bindings: [] };
}

function tokenize(source: string): Token[] {
	const tokens: Token[] = [];
	let index = 0;

	while (index < source.length) {
		const char = source[index] ?? "";
		const next = source[index + 1] ?? "";

		if (isWhitespace(char)) {
			index += 1;
			continue;
		}

		if (char === "/" && next === "/") {
			index += 2;
			while (index < source.length && source[index] !== "\n") index += 1;
			continue;
		}

		if (char === "/" && next === "*") {
			index += 2;
			while (index + 1 < source.length && !(source[index] === "*" && source[index + 1] === "/")) {
				index += 1;
			}
			index = Math.min(source.length, index + 2);
			continue;
		}

		if (char === "'" || char === '"') {
			index = skipQuotedString(source, index, char);
			continue;
		}

		const operator = readOperator(source, index);
		if (operator) {
			tokens.push({ type: "operator", value: operator, start: index, end: index + operator.length });
			index += operator.length;
			continue;
		}

		if (isPunctuation(char)) {
			tokens.push({ type: "punct", value: char, start: index, end: index + 1 });
			index += 1;
			continue;
		}

		if (isIdentifierStart(char)) {
			const start = index;
			index += 1;
			while (index < source.length && isIdentifierChar(source[index])) index += 1;
			const value = source.slice(start, index);
			const type: TokenType = KEYWORDS.has(value) ? "keyword" : "identifier";
			tokens.push({ type, value, start, end: index });
			continue;
		}

		index += 1;
	}

	return tokens;
}

function isWhitespace(char: string | undefined): boolean {
	if (!char) return false;
	return char === " " || char === "\t" || char === "\n" || char === "\r";
}

function isPunctuation(char: string | undefined): boolean {
	if (!char) return false;
	return char === "(" || char === ")" || char === "[" || char === "]" || char === "{" || char === "}" || char === "." || char === "," || char === ":" || char === ";";
}

function isIdentifierStart(char: string | undefined): boolean {
	if (!char) return false;
	return /[A-Za-z_]/.test(char);
}

function isIdentifierChar(char: string | undefined): boolean {
	if (!char) return false;
	return /[A-Za-z0-9_-]/.test(char);
}

function skipQuotedString(source: string, start: number, quote: string): number {
	let index = start + 1;
	while (index < source.length) {
		const char = source[index];
		if (char === "\\") {
			index += 2;
			continue;
		}
		if (char === quote) return index + 1;
		index += 1;
	}
	return source.length;
}

function readOperator(source: string, start: number): string | null {
	const two = source.slice(start, start + 2);
	if (["==", "!=", "<=", ">=", ":=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^="].includes(two)) {
		return two;
	}
	const one = source[start] ?? "";
	if (["=", "+", "-", "*", "/", "%", "&", "|", "^", "~", "<", ">"].includes(one)) {
		return one;
	}
	return null;
}

function getCurrentScopeId(scopeStack: number[]): number {
	return scopeStack[scopeStack.length - 1] ?? 0;
}

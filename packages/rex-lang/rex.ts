import rexGrammar from "./rex.ohm-bundle.js";

export const grammar = rexGrammar;
export const semantics = rexGrammar.createSemantics();

export type IRNode =
	| { type: "program"; body: IRNode[] }
	| { type: "identifier"; name: string }
	| { type: "self" }
	| { type: "boolean"; value: boolean }
	| { type: "null" }
	| { type: "undefined" }
	| { type: "number"; raw: string; value: number }
	| { type: "string"; raw: string }
	| { type: "array"; items: IRNode[] }
	| { type: "arrayComprehension"; binding: IRBindingOrExpr; body: IRNode }
	| { type: "object"; entries: { key: IRNode; value: IRNode }[] }
	| {
			type: "objectComprehension";
			binding: IRBindingOrExpr;
			key: IRNode;
			value: IRNode;
	  }
	| { type: "key"; name: string }
	| { type: "group"; expression: IRNode }
	| { type: "unary"; op: "neg" | "not" | "delete"; value: IRNode }
	| {
			type: "binary";
			op:
				| "add"
				| "sub"
				| "mul"
				| "div"
				| "mod"
				| "bitAnd"
				| "bitOr"
				| "bitXor"
				| "and"
				| "or"
				| "eq"
				| "neq"
				| "gt"
				| "gte"
				| "lt"
				| "lte";
			left: IRNode;
			right: IRNode;
	  }
	| {
			type: "assign";
			op: "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=";
			place: IRNode;
			value: IRNode;
	  }
	| {
			type: "navigation";
			target: IRNode;
			segments: ({ type: "static"; key: string } | { type: "dynamic"; key: IRNode })[];
	  }
	| { type: "call"; callee: IRNode; args: IRNode[] }
	| {
			type: "conditional";
			head: "when" | "unless";
			condition: IRNode;
			thenBlock: IRNode[];
			elseBranch?: IRConditionalElse;
	  }
	| { type: "for"; binding: IRBindingOrExpr; body: IRNode[] }
	| { type: "break" }
	| { type: "continue" };

export type IRBinding =
	| { type: "binding:keyValueIn"; key: string; value: string; source: IRNode }
	| { type: "binding:valueIn"; value: string; source: IRNode }
	| { type: "binding:keyOf"; key: string; source: IRNode };

export type IRBindingOrExpr = IRBinding | { type: "binding:expr"; source: IRNode };

export type IRConditionalElse =
	| { type: "else"; block: IRNode[] }
	| {
			type: "elseChain";
			head: "when" | "unless";
			condition: IRNode;
			thenBlock: IRNode[];
			elseBranch?: IRConditionalElse;
	  };

const DIGITS = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";

function byteLength(value: string): number {
	return Buffer.byteLength(value, "utf8");
}

const OPCODE_IDS = {
	do: 0,
	add: 1,
	sub: 2,
	mul: 3,
	div: 4,
	eq: 5,
	neq: 6,
	lt: 7,
	lte: 8,
	gt: 9,
	gte: 10,
	and: 11,
	or: 12,
	xor: 13,
	not: 14,
	boolean: 15,
	number: 16,
	string: 17,
	array: 18,
	object: 19,
	mod: 20,
	neg: 21,
} as const;

type OpcodeName = keyof typeof OPCODE_IDS;

const BINARY_TO_OPCODE: Record<Extract<IRNode, { type: "binary" }> ["op"], OpcodeName> = {
	add: "add",
	sub: "sub",
	mul: "mul",
	div: "div",
	mod: "mod",
	bitAnd: "and",
	bitOr: "or",
	bitXor: "xor",
	and: "and",
	or: "or",
	eq: "eq",
	neq: "neq",
	gt: "gt",
	gte: "gte",
	lt: "lt",
	lte: "lte",
};

const ASSIGN_COMPOUND_TO_OPCODE: Partial<Record<Extract<IRNode, { type: "assign" }> ["op"], OpcodeName>> = {
	"+=": "add",
	"-=": "sub",
	"*=": "mul",
	"/=": "div",
	"%=": "mod",
	"&=": "and",
	"|=": "or",
	"^=": "xor",
};

function encodeUint(value: number): string {
	if (!Number.isInteger(value) || value < 0) throw new Error(`Cannot encode non-uint value: ${value}`);
	if (value === 0) return "";
	let current = value;
	let out = "";
	while (current > 0) {
		const digit = current % 64;
		out = `${DIGITS[digit]}${out}`;
		current = Math.floor(current / 64);
	}
	return out;
}

function encodeZigzag(value: number): string {
	if (!Number.isInteger(value)) throw new Error(`Cannot zigzag non-integer: ${value}`);
	const encoded = value >= 0 ? value * 2 : -value * 2 - 1;
	return encodeUint(encoded);
}

function encodeInt(value: number): string {
	return `${encodeZigzag(value)}+`;
}

function canUseBareString(value: string): boolean {
	for (const char of value) {
		if (!DIGITS.includes(char)) return false;
	}
	return true;
}

function decodeStringLiteral(raw: string): string {
	const quote = raw[0];
	if ((quote !== '"' && quote !== "'") || raw[raw.length - 1] !== quote) {
		throw new Error(`Invalid string literal: ${raw}`);
	}
	let out = "";
	for (let index = 1; index < raw.length - 1; index += 1) {
		const char = raw[index];
		if (char !== "\\") {
			out += char;
			continue;
		}
		index += 1;
		const esc = raw[index];
		if (esc === undefined) throw new Error(`Invalid escape sequence in ${raw}`);
		if (esc === "n") out += "\n";
		else if (esc === "r") out += "\r";
		else if (esc === "t") out += "\t";
		else if (esc === "b") out += "\b";
		else if (esc === "f") out += "\f";
		else if (esc === "v") out += "\v";
		else if (esc === "0") out += "\0";
		else if (esc === "x") {
			const hex = raw.slice(index + 1, index + 3);
			if (!/^[0-9a-fA-F]{2}$/.test(hex)) throw new Error(`Invalid hex escape in ${raw}`);
			out += String.fromCodePoint(parseInt(hex, 16));
			index += 2;
		}
		else if (esc === "u") {
			const hex = raw.slice(index + 1, index + 5);
			if (!/^[0-9a-fA-F]{4}$/.test(hex)) throw new Error(`Invalid unicode escape in ${raw}`);
			out += String.fromCodePoint(parseInt(hex, 16));
			index += 4;
		}
		else {
			out += esc;
		}
	}
	return out;
}

function encodeBareOrLengthString(value: string): string {
	if (canUseBareString(value)) return `${value}:`;
	return `${encodeUint(byteLength(value))},${value}`;
}

function encodeNumberNode(node: Extract<IRNode, { type: "number" }>): string {
	const numberValue = node.value;
	if (!Number.isFinite(numberValue)) throw new Error(`Cannot encode non-finite number: ${node.raw}`);
	if (Number.isInteger(numberValue)) return encodeInt(numberValue);

	const raw = node.raw.toLowerCase();
	const sign = raw.startsWith("-") ? -1 : 1;
	const unsigned = sign < 0 ? raw.slice(1) : raw;
	const splitExp = unsigned.split("e");
	const mantissaText = splitExp[0];
	const exponentText = splitExp[1] ?? "0";
	if (!mantissaText) throw new Error(`Invalid decimal literal: ${node.raw}`);
	const exponent = Number(exponentText);
	if (!Number.isInteger(exponent)) throw new Error(`Invalid decimal exponent: ${node.raw}`);

	const dotIndex = mantissaText.indexOf(".");
	const decimals = dotIndex === -1 ? 0 : mantissaText.length - dotIndex - 1;
	const digits = mantissaText.replace(".", "");
	if (!/^\d+$/.test(digits)) throw new Error(`Invalid decimal digits: ${node.raw}`);

	let significand = Number(digits) * sign;
	let power = exponent - decimals;
	while (significand !== 0 && significand % 10 === 0) {
		significand /= 10;
		power += 1;
	}
	return `${encodeZigzag(power)}*${encodeInt(significand)}`;
}

function encodeOpcode(opcode: OpcodeName): string {
	return `${encodeUint(OPCODE_IDS[opcode])}%`;
}

function encodeCallParts(parts: string[]): string {
	return `(${parts.join("")})`;
}

function needsOptionalPrefix(encoded: string): boolean {
	const first = encoded[0];
	if (!first) return false;
	return first === "[" || first === "{" || first === "(" || first === "=" || first === "~" || first === "?" || first === "!" || first === "|" || first === "&" || first === ">" || first === "<";
}

function addOptionalPrefix(encoded: string): string {
	if (!needsOptionalPrefix(encoded)) return encoded;
	let payload = encoded;
	if (encoded.startsWith("?(") || encoded.startsWith("!(") || encoded.startsWith("|(") || encoded.startsWith("&(") || encoded.startsWith(">(") || encoded.startsWith("<(")) {
		payload = encoded.slice(2, -1);
	}
	else if (encoded.startsWith(">[") || encoded.startsWith(">{")) {
		payload = encoded.slice(2, -1);
	}
	else if (encoded.startsWith("[") || encoded.startsWith("{") || encoded.startsWith("(")) {
		payload = encoded.slice(1, -1);
	}
	else if (encoded.startsWith("=") || encoded.startsWith("~")) {
		payload = encoded.slice(1);
	}
	return `${encodeUint(byteLength(payload))}${encoded}`;
}

function encodeBlockExpression(block: IRNode[]): string {
	if (block.length === 0) return "4@";
	if (block.length === 1) return encodeNode(block[0] as IRNode);
	return encodeCallParts([encodeOpcode("do"), ...block.map((node) => encodeNode(node))]);
}

function encodeConditionalElse(elseBranch: IRConditionalElse): string {
	if (elseBranch.type === "else") return encodeBlockExpression(elseBranch.block);
	const nested = {
		type: "conditional",
		head: elseBranch.head,
		condition: elseBranch.condition,
		thenBlock: elseBranch.thenBlock,
		elseBranch: elseBranch.elseBranch,
	} satisfies IRNode;
	return encodeNode(nested);
}

function encodeNavigation(node: Extract<IRNode, { type: "navigation" }>): string {
	const parts = [encodeNode(node.target)];
	for (const segment of node.segments) {
		if (segment.type === "static") parts.push(encodeBareOrLengthString(segment.key));
		else parts.push(encodeNode(segment.key));
	}
	return encodeCallParts(parts);
}

function encodeFor(node: Extract<IRNode, { type: "for" }>): string {
	const body = addOptionalPrefix(encodeBlockExpression(node.body));
	if (node.binding.type === "binding:expr") {
		return `>(${encodeNode(node.binding.source)}${body})`;
	}
	if (node.binding.type === "binding:valueIn") {
		return `>(${encodeNode(node.binding.source)}${node.binding.value}$${body})`;
	}
	if (node.binding.type === "binding:keyValueIn") {
		return `>(${encodeNode(node.binding.source)}${node.binding.key}$${node.binding.value}$${body})`;
	}
	return `<(${encodeNode(node.binding.source)}${node.binding.key}$${body})`;
}

function encodeArrayComprehension(node: Extract<IRNode, { type: "arrayComprehension" }>): string {
	const body = addOptionalPrefix(encodeNode(node.body));
	if (node.binding.type === "binding:expr") {
		return `>[${encodeNode(node.binding.source)}${body}]`;
	}
	if (node.binding.type === "binding:valueIn") {
		return `>[${encodeNode(node.binding.source)}${node.binding.value}$${body}]`;
	}
	if (node.binding.type === "binding:keyValueIn") {
		return `>[${encodeNode(node.binding.source)}${node.binding.key}$${node.binding.value}$${body}]`;
	}
	return `>[${encodeNode(node.binding.source)}${node.binding.key}$${body}]`;
}

function encodeObjectComprehension(node: Extract<IRNode, { type: "objectComprehension" }>): string {
	const key = addOptionalPrefix(encodeNode(node.key));
	const value = addOptionalPrefix(encodeNode(node.value));
	if (node.binding.type === "binding:expr") {
		return `>{${encodeNode(node.binding.source)}${key}${value}}`;
	}
	if (node.binding.type === "binding:valueIn") {
		return `>{${encodeNode(node.binding.source)}${node.binding.value}$${key}${value}}`;
	}
	if (node.binding.type === "binding:keyValueIn") {
		return `>{${encodeNode(node.binding.source)}${node.binding.key}$${node.binding.value}$${key}${value}}`;
	}
	return `>{${encodeNode(node.binding.source)}${node.binding.key}$${key}${value}}`;
}

function encodeNode(node: IRNode): string {
	switch (node.type) {
		case "program":
			return encodeBlockExpression(node.body);
		case "identifier":
			return `${node.name}$`;
		case "self":
			return "@";
		case "boolean":
			return node.value ? "1@" : "2@";
		case "null":
			return "3@";
		case "undefined":
			return "4@";
		case "number":
			return encodeNumberNode(node);
		case "string":
			return encodeBareOrLengthString(decodeStringLiteral(node.raw));
		case "array": {
			const body = node.items.map((item) => addOptionalPrefix(encodeNode(item))).join("");
			return `[${body}]`;
		}
		case "arrayComprehension":
			return encodeArrayComprehension(node);
		case "object": {
			const body = node.entries
				.map(({ key, value }) => `${encodeNode(key)}${addOptionalPrefix(encodeNode(value))}`)
				.join("");
			return `{${body}}`;
		}
		case "objectComprehension":
			return encodeObjectComprehension(node);
		case "key":
			return encodeBareOrLengthString(node.name);
		case "group":
			return encodeNode(node.expression);
		case "unary":
			if (node.op === "delete") return `~${encodeNode(node.value)}`;
			if (node.op === "neg") return encodeCallParts([encodeOpcode("neg"), encodeNode(node.value)]);
			return encodeCallParts([encodeOpcode("not"), encodeNode(node.value)]);
		case "binary":
			return encodeCallParts([
				encodeOpcode(BINARY_TO_OPCODE[node.op]),
				encodeNode(node.left),
				encodeNode(node.right),
			]);
		case "assign": {
			if (node.op === "=") return `=${encodeNode(node.place)}${encodeNode(node.value)}`;
			const opcode = ASSIGN_COMPOUND_TO_OPCODE[node.op];
			if (!opcode) throw new Error(`Unsupported assignment op: ${node.op}`);
			const computedValue = encodeCallParts([encodeOpcode(opcode), encodeNode(node.place), encodeNode(node.value)]);
			return `=${encodeNode(node.place)}${computedValue}`;
		}
		case "navigation":
			return encodeNavigation(node);
		case "call":
			return encodeCallParts([encodeNode(node.callee), ...node.args.map((arg) => encodeNode(arg))]);
		case "conditional": {
			const opener = node.head === "when" ? "?(" : "!(";
			const cond = encodeNode(node.condition);
			const thenExpr = addOptionalPrefix(encodeBlockExpression(node.thenBlock));
			const elseExpr = node.elseBranch ? addOptionalPrefix(encodeConditionalElse(node.elseBranch)) : "";
			return `${opener}${cond}${thenExpr}${elseExpr})`;
		}
		case "for":
			return encodeFor(node);
		case "break":
			return ";";
		case "continue":
			return "1;";
		default: {
			const exhaustive: never = node;
			throw new Error(`Unsupported IR node ${(exhaustive as { type?: string }).type ?? "unknown"}`);
		}
	}
}

export function parseToIR(source: string): IRNode {
	const match = grammar.match(source);
	if (!match.succeeded()) {
		const failure = match as unknown as { message?: string };
		throw new Error(failure.message ?? "Parse failed");
	}
	return semantics(match).toIR() as IRNode;
}

export function encodeIR(node: IRNode): string {
	return encodeNode(node);
}

export function compile(source: string): string {
	return encodeIR(parseToIR(source));
}

type IRPostfixStep =
	| { kind: "navStatic"; key: string }
	| { kind: "navDynamic"; key: IRNode }
	| { kind: "call"; args: IRNode[] };

function parseNumber(raw: string) {
	if (/^-?0x/i.test(raw)) return parseInt(raw, 16);
	if (/^-?0b/i.test(raw)) {
		const isNegative = raw.startsWith("-");
		const digits = raw.replace(/^-?0b/i, "");
		const value = parseInt(digits, 2);
		return isNegative ? -value : value;
	}
	return Number(raw);
}

function collectStructured(value: unknown, out: Array<IRNode | { key: IRNode; value: IRNode }>) {
	if (Array.isArray(value)) {
		for (const part of value) collectStructured(part, out);
		return;
	}
	if (!value || typeof value !== "object") return;
	if ("type" in value || ("key" in value && "value" in value)) {
		out.push(value as IRNode | { key: IRNode; value: IRNode });
	}
}

function normalizeList(value: unknown): Array<IRNode | { key: IRNode; value: IRNode }> {
	const out: Array<IRNode | { key: IRNode; value: IRNode }> = [];
	collectStructured(value, out);
	return out;
}

function collectPostfixSteps(value: unknown, out: IRPostfixStep[]) {
	if (Array.isArray(value)) {
		for (const part of value) collectPostfixSteps(part, out);
		return;
	}
	if (!value || typeof value !== "object") return;
	if ("kind" in value) out.push(value as IRPostfixStep);
}

function normalizePostfixSteps(value: unknown): IRPostfixStep[] {
	const out: IRPostfixStep[] = [];
	collectPostfixSteps(value, out);
	return out;
}

function buildPostfix(base: IRNode, steps: IRPostfixStep[]) {
	let current = base;
	let pendingSegments: Extract<IRNode, { type: "navigation" }>["segments"] = [];

	const flushSegments = () => {
		if (pendingSegments.length === 0) return;
		current = {
			type: "navigation",
			target: current,
			segments: pendingSegments,
		} satisfies IRNode;
		pendingSegments = [];
	};

	for (const step of steps) {
		if (step.kind === "navStatic") {
			pendingSegments.push({ type: "static", key: step.key });
			continue;
		}
		if (step.kind === "navDynamic") {
			pendingSegments.push({ type: "dynamic", key: step.key });
			continue;
		}
		flushSegments();
		current = { type: "call", callee: current, args: step.args } satisfies IRNode;
	}

	flushSegments();
	return current;
}

semantics.addOperation("toIR", {
	_iter(...children) {
		return children.map((child) => child.toIR());
	},
	_terminal() {
		return this.sourceString;
	},
	_nonterminal(...children) {
		if (children.length === 1 && children[0]) return children[0].toIR();
		return children.map((child) => child.toIR());
	},

	Program(expressions) {
		const body = normalizeList(expressions.toIR()) as IRNode[];
		if (body.length === 1) return body[0];
		return { type: "program", body } satisfies IRNode;
	},

	Block(expressions) {
		return normalizeList(expressions.toIR()) as IRNode[];
	},

	Elements(first, separatorsAndItems, maybeTrailingComma, maybeEmpty) {
		return normalizeList([
			first.toIR(),
			separatorsAndItems.toIR(),
			maybeTrailingComma.toIR(),
			maybeEmpty.toIR(),
		]);
	},

	AssignExpr_assign(place, op, value) {
		return {
			type: "assign",
			op: op.sourceString as Extract<IRNode, { type: "assign" }>["op"],
			place: place.toIR(),
			value: value.toIR(),
		} satisfies IRNode;
	},

	ExistenceExpr_and(left, _and, right) {
		return { type: "binary", op: "and", left: left.toIR(), right: right.toIR() } satisfies IRNode;
	},
	ExistenceExpr_or(left, _or, right) {
		return { type: "binary", op: "or", left: left.toIR(), right: right.toIR() } satisfies IRNode;
	},

	BitExpr_and(left, _op, right) {
		return { type: "binary", op: "bitAnd", left: left.toIR(), right: right.toIR() } satisfies IRNode;
	},
	BitExpr_xor(left, _op, right) {
		return { type: "binary", op: "bitXor", left: left.toIR(), right: right.toIR() } satisfies IRNode;
	},
	BitExpr_or(left, _op, right) {
		return { type: "binary", op: "bitOr", left: left.toIR(), right: right.toIR() } satisfies IRNode;
	},

	CompareExpr_binary(left, op, right) {
		const map: Record<string, Extract<IRNode, { type: "binary" }>["op"]> = {
			"==": "eq",
			"!=": "neq",
			">": "gt",
			">=": "gte",
			"<": "lt",
			"<=": "lte",
		};
		const mapped = map[op.sourceString];
		if (!mapped) throw new Error(`Unsupported compare op: ${op.sourceString}`);
		return { type: "binary", op: mapped, left: left.toIR(), right: right.toIR() } satisfies IRNode;
	},

	AddExpr_add(left, _op, right) {
		return { type: "binary", op: "add", left: left.toIR(), right: right.toIR() } satisfies IRNode;
	},
	AddExpr_sub(left, _op, right) {
		return { type: "binary", op: "sub", left: left.toIR(), right: right.toIR() } satisfies IRNode;
	},

	MulExpr_mul(left, _op, right) {
		return { type: "binary", op: "mul", left: left.toIR(), right: right.toIR() } satisfies IRNode;
	},
	MulExpr_div(left, _op, right) {
		return { type: "binary", op: "div", left: left.toIR(), right: right.toIR() } satisfies IRNode;
	},
	MulExpr_mod(left, _op, right) {
		return { type: "binary", op: "mod", left: left.toIR(), right: right.toIR() } satisfies IRNode;
	},

	UnaryExpr_neg(_op, value) {
		const lowered = value.toIR() as IRNode;
		if (lowered.type === "number") {
			const raw = lowered.raw.startsWith("-") ? lowered.raw.slice(1) : `-${lowered.raw}`;
			return { type: "number", raw, value: -lowered.value } satisfies IRNode;
		}
		return { type: "unary", op: "neg", value: lowered } satisfies IRNode;
	},
	UnaryExpr_not(_op, value) {
		return { type: "unary", op: "not", value: value.toIR() } satisfies IRNode;
	},
	UnaryExpr_delete(_del, place) {
		return { type: "unary", op: "delete", value: place.toIR() } satisfies IRNode;
	},

	PostfixExpr_chain(base, tails) {
		return buildPostfix(base.toIR(), normalizePostfixSteps(tails.toIR()));
	},
	Place(base, tails) {
		return buildPostfix(base.toIR(), normalizePostfixSteps(tails.toIR()));
	},
	PlaceTail_navStatic(_dot, key) {
		return { kind: "navStatic", key: key.sourceString } satisfies IRPostfixStep;
	},
	PlaceTail_navDynamic(_dotOpen, key, _close) {
		return { kind: "navDynamic", key: key.toIR() } satisfies IRPostfixStep;
	},
	PostfixTail_navStatic(_dot, key) {
		return { kind: "navStatic", key: key.sourceString } satisfies IRPostfixStep;
	},
	PostfixTail_navDynamic(_dotOpen, key, _close) {
		return { kind: "navDynamic", key: key.toIR() } satisfies IRPostfixStep;
	},
	PostfixTail_callEmpty(_open, _close) {
		return { kind: "call", args: [] } satisfies IRPostfixStep;
	},
	PostfixTail_call(_open, args, _close) {
		return { kind: "call", args: normalizeList(args.toIR()) as IRNode[] } satisfies IRPostfixStep;
	},

	ConditionalExpr(head, condition, _do, thenBlock, elseBranch, _end) {
		const nextElse = elseBranch.children[0];
		return {
			type: "conditional",
			head: head.toIR() as "when" | "unless",
			condition: condition.toIR(),
			thenBlock: thenBlock.toIR() as IRNode[],
			elseBranch: nextElse ? (nextElse.toIR() as IRConditionalElse) : undefined,
		} satisfies IRNode;
	},
	ConditionalHead(_kw) {
		return this.sourceString as "when" | "unless";
	},
	ConditionalElse_elseChain(_else, head, condition, _do, thenBlock, elseBranch) {
		const nextElse = elseBranch.children[0];
		return {
			type: "elseChain",
			head: head.toIR() as "when" | "unless",
			condition: condition.toIR(),
			thenBlock: thenBlock.toIR() as IRNode[],
			elseBranch: nextElse ? (nextElse.toIR() as IRConditionalElse) : undefined,
		} satisfies IRConditionalElse;
	},
	ConditionalElse_else(_else, block) {
		return { type: "else", block: block.toIR() as IRNode[] } satisfies IRConditionalElse;
	},

	DoExpr(_do, block, _end) {
		const body = block.toIR() as IRNode[];
		if (body.length === 0) return { type: "undefined" } satisfies IRNode;
		if (body.length === 1) return body[0] as IRNode;
		return { type: "program", body } satisfies IRNode;
	},

	ForExpr(_for, binding, _do, block, _end) {
		return {
			type: "for",
			binding: binding.toIR() as IRBindingOrExpr,
			body: block.toIR() as IRNode[],
		} satisfies IRNode;
	},
	BindingExpr(iterOrExpr) {
		const node = iterOrExpr.toIR();
		if (typeof node === "object" && node && "type" in node && String(node.type).startsWith("binding:")) {
			return node as IRBinding;
		}
		return { type: "binding:expr", source: node as IRNode } satisfies IRBindingOrExpr;
	},

	Array_empty(_open, _close) {
		return { type: "array", items: [] } satisfies IRNode;
	},
	Array_comprehension(_open, binding, _semi, body, _close) {
		return {
			type: "arrayComprehension",
			binding: binding.toIR() as IRBindingOrExpr,
			body: body.toIR(),
		} satisfies IRNode;
	},
	Array_values(_open, items, _close) {
		return { type: "array", items: normalizeList(items.toIR()) as IRNode[] } satisfies IRNode;
	},

	Object_empty(_open, _close) {
		return { type: "object", entries: [] } satisfies IRNode;
	},
	Object_comprehension(_open, binding, _semi, key, _colon, value, _close) {
		return {
			type: "objectComprehension",
			binding: binding.toIR() as IRBindingOrExpr,
			key: key.toIR(),
			value: value.toIR(),
		} satisfies IRNode;
	},
	Object_pairs(_open, pairs, _close) {
		return {
			type: "object",
			entries: normalizeList(pairs.toIR()) as Array<{ key: IRNode; value: IRNode }>,
		} satisfies IRNode;
	},

	IterBinding_keyValueIn(key, _comma, value, _in, source) {
		return {
			type: "binding:keyValueIn",
			key: key.sourceString,
			value: value.sourceString,
			source: source.toIR(),
		} satisfies IRBinding;
	},
	IterBinding_valueIn(value, _in, source) {
		return {
			type: "binding:valueIn",
			value: value.sourceString,
			source: source.toIR(),
		} satisfies IRBinding;
	},
	IterBinding_keyOf(key, _of, source) {
		return {
			type: "binding:keyOf",
			key: key.sourceString,
			source: source.toIR(),
		} satisfies IRBinding;
	},

	Pair(key, _colon, value) {
		return { key: key.toIR(), value: value.toIR() };
	},
	ObjKey_bare(key) {
		return { type: "key", name: key.sourceString } satisfies IRNode;
	},
	ObjKey_number(num) {
		return num.toIR();
	},
	ObjKey_string(str) {
		return str.toIR();
	},
	ObjKey_computed(_open, expr, _close) {
		return expr.toIR();
	},

	BreakKw(_kw) {
		return { type: "break" } satisfies IRNode;
	},
	ContinueKw(_kw) {
		return { type: "continue" } satisfies IRNode;
	},
	SelfKw(_kw) {
		return { type: "self" } satisfies IRNode;
	},
	TrueKw(_kw) {
		return { type: "boolean", value: true } satisfies IRNode;
	},
	FalseKw(_kw) {
		return { type: "boolean", value: false } satisfies IRNode;
	},
	NullKw(_kw) {
		return { type: "null" } satisfies IRNode;
	},
	UndefinedKw(_kw) {
		return { type: "undefined" } satisfies IRNode;
	},

	StringKw(_kw) {
		return { type: "identifier", name: "string" } satisfies IRNode;
	},
	NumberKw(_kw) {
		return { type: "identifier", name: "number" } satisfies IRNode;
	},
	ObjectKw(_kw) {
		return { type: "identifier", name: "object" } satisfies IRNode;
	},
	ArrayKw(_kw) {
		return { type: "identifier", name: "array" } satisfies IRNode;
	},
	BooleanKw(_kw) {
		return { type: "identifier", name: "boolean" } satisfies IRNode;
	},

	identifier(_a, _b) {
		return { type: "identifier", name: this.sourceString } satisfies IRNode;
	},

	String(_value) {
		return { type: "string", raw: this.sourceString } satisfies IRNode;
	},
	Number(_value) {
		return { type: "number", raw: this.sourceString, value: parseNumber(this.sourceString) } satisfies IRNode;
	},

	PrimaryExpr_group(_open, expr, _close) {
		return { type: "group", expression: expr.toIR() } satisfies IRNode;
	},
});

export default semantics;

export type { RexActionDict, RexSemantics } from "./rex.ohm-bundle.js";

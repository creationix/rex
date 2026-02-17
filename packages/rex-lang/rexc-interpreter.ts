import { compile } from "./rex.ts";

const DIGITS = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";
const digitMap = new Map<string, number>(Array.from(DIGITS).map((char, index) => [char, index]));

const OPCODES = {
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

export type RexcContext = {
	vars?: Record<string, unknown>;
	refs?: Partial<Record<number, unknown>>;
	self?: unknown;
	selfStack?: unknown[];
	opcodes?: Partial<Record<number, (args: unknown[], state: RexcRuntimeState) => unknown>>;
};

export type RexcRuntimeState = {
	vars: Record<string, unknown>;
	refs: Record<number, unknown>;
};

type LoopControl = { kind: "break" | "continue"; depth: number };

type OpcodeMarker = { __opcode: number };

function decodePrefix(text: string, start: number, end: number): number {
	let value = 0;
	for (let index = start; index < end; index += 1) {
		const digit = digitMap.get(text[index] ?? "") ?? -1;
		if (digit < 0) throw new Error(`Invalid digit '${text[index]}'`);
		value = value * 64 + digit;
	}
	return value;
}

function decodeZigzag(value: number): number {
	return value % 2 === 0 ? value / 2 : -(value + 1) / 2;
}

function isValueStart(char: string | undefined): boolean {
	if (!char) return false;
	if (digitMap.has(char)) return true;
	return "+*:%$@'^~=([{,?!|&><;".includes(char);
}

function isDefined(value: unknown): boolean {
	return value !== undefined;
}

class CursorInterpreter {
	private readonly text: string;
	private pos = 0;
	private readonly state: RexcRuntimeState;
	private readonly selfStack: unknown[];
	private readonly opcodeMarkers: OpcodeMarker[];
	private readonly pointerCache = new Map<number, unknown>();

	constructor(text: string, ctx: RexcContext = {}) {
		const initialSelf = ctx.selfStack && ctx.selfStack.length > 0
			? ctx.selfStack[ctx.selfStack.length - 1]
			: ctx.self;
		this.text = text;
		this.state = {
			vars: ctx.vars ?? {},
			refs: {
				0: ctx.refs?.[0],
				1: ctx.refs?.[1] ?? true,
				2: ctx.refs?.[2] ?? false,
				3: ctx.refs?.[3] ?? null,
				4: ctx.refs?.[4] ?? undefined,
			},
		};
		this.selfStack = ctx.selfStack && ctx.selfStack.length > 0 ? [...ctx.selfStack] : [initialSelf];
		this.opcodeMarkers = Array.from({ length: 256 }, (_, id) => ({ __opcode: id }));
		for (const [idText, value] of Object.entries(ctx.refs ?? {})) {
			const id = Number(idText);
			if (Number.isInteger(id)) this.state.refs[id] = value;
		}
		if (ctx.opcodes) {
			for (const [idText, op] of Object.entries(ctx.opcodes)) {
				if (op) this.customOpcodes.set(Number(idText), op);
			}
		}
	}

	private readonly customOpcodes = new Map<number, (args: unknown[], state: RexcRuntimeState) => unknown>();

	private readSelf(depthPrefix: number): unknown {
		const depth = depthPrefix + 1;
		const index = this.selfStack.length - depth;
		if (index < 0) return undefined;
		return this.selfStack[index];
	}

	get runtimeState() {
		return this.state;
	}

	evaluateTopLevel(): unknown {
		this.skipNonCode();
		if (this.pos >= this.text.length) return undefined;
		const value = this.evalValue();
		this.skipNonCode();
		if (this.pos < this.text.length) throw new Error(`Unexpected trailing input at ${this.pos}`);
		return value;
	}

	private skipNonCode() {
		while (this.pos < this.text.length) {
			const ch = this.text[this.pos];
			if (ch === " " || ch === "\t" || ch === "\n" || ch === "\r") {
				this.pos += 1;
				continue;
			}
			if (ch === "/" && this.text[this.pos + 1] === "/") {
				this.pos += 2;
				while (this.pos < this.text.length && this.text[this.pos] !== "\n") this.pos += 1;
				continue;
			}
			if (ch === "/" && this.text[this.pos + 1] === "*") {
				this.pos += 2;
				while (this.pos < this.text.length) {
					if (this.text[this.pos] === "*" && this.text[this.pos + 1] === "/") {
						this.pos += 2;
						break;
					}
					this.pos += 1;
				}
				continue;
			}
			break;
		}
	}

	private readPrefix() {
		const start = this.pos;
		while (this.pos < this.text.length && digitMap.has(this.text[this.pos] ?? "")) this.pos += 1;
		const end = this.pos;
		return { start, end, value: decodePrefix(this.text, start, end), raw: this.text.slice(start, end) };
	}

	private ensure(char: string) {
		if (this.text[this.pos] !== char) throw new Error(`Expected '${char}' at ${this.pos}`);
		this.pos += 1;
	}

	private hasMoreBefore(close: string) {
		const save = this.pos;
		this.skipNonCode();
		const more = this.pos < this.text.length && this.text[this.pos] !== close;
		this.pos = save;
		return more;
	}

	private readBindingVarIfPresent(): string | undefined {
		const save = this.pos;
		this.skipNonCode();
		const prefix = this.readPrefix();
		const tag = this.text[this.pos];
		if (tag === "$" && prefix.raw.length > 0) {
			this.pos += 1;
			return prefix.raw;
		}
		this.pos = save;
		return undefined;
	}

	private evalValue(): unknown {
		this.skipNonCode();
		const prefix = this.readPrefix();
		const tag = this.text[this.pos];
		if (!tag) throw new Error("Unexpected end of input");

		switch (tag) {
			case "+":
				this.pos += 1;
				return decodeZigzag(prefix.value);
			case "*": {
				this.pos += 1;
				const power = decodeZigzag(prefix.value);
				const significand = this.evalValue();
				if (typeof significand !== "number") throw new Error("Decimal significand must be numeric");
				return significand * 10 ** power;
			}
			case ":":
				this.pos += 1;
				return prefix.raw;
			case "%":
				this.pos += 1;
				return this.opcodeMarkers[prefix.value] ?? { __opcode: prefix.value };
			case "@":
				this.pos += 1;
				return this.readSelf(prefix.value);
			case "'":
				this.pos += 1;
				return this.state.refs[prefix.value];
			case "$":
				this.pos += 1;
				return this.state.vars[prefix.raw];
			case ",": {
				this.pos += 1;
				const start = this.pos;
				const end = start + prefix.value;
				if (end > this.text.length) throw new Error("String container overflows input");
				const value = this.text.slice(start, end);
				this.pos = end;
				return value;
			}
			case "^": {
				this.pos += 1;
				const target = this.pos + prefix.value;
				if (this.pointerCache.has(target)) return this.pointerCache.get(target);
				const save = this.pos;
				this.pos = target;
				const value = this.evalValue();
				this.pos = save;
				this.pointerCache.set(target, value);
				return value;
			}
			case "=": {
				this.pos += 1;
				const place = this.readPlace();
				const value = this.evalValue();
				this.writePlace(place, value);
				return value;
			}
			case "~": {
				this.pos += 1;
				const place = this.readPlace();
				this.deletePlace(place);
				return undefined;
			}
			case ";": {
				this.pos += 1;
				const kind: LoopControl["kind"] = prefix.value % 2 === 0 ? "break" : "continue";
				const depth = Math.floor(prefix.value / 2) + 1;
				return { kind, depth } satisfies LoopControl;
			}
			case "(":
				return this.evalCall(prefix.value);
			case "[":
				return this.evalArray(prefix.value);
			case "{":
				return this.evalObject(prefix.value);
			case "?":
			case "!":
			case "|":
			case "&":
				return this.evalFlowParen(tag);
			case ">":
			case "<":
				return this.evalLoopLike(tag);
			default:
				throw new Error(`Unexpected tag '${tag}' at ${this.pos}`);
		}
	}

	private evalCall(_prefix: number) {
		this.ensure("(");
		this.skipNonCode();
		if (this.text[this.pos] === ")") {
			this.pos += 1;
			return undefined;
		}
		const callee = this.evalValue();
		const args: unknown[] = [];
		while (true) {
			this.skipNonCode();
			if (this.text[this.pos] === ")") break;
			args.push(this.evalValue());
		}
		this.ensure(")");

		if (typeof callee === "object" && callee && "__opcode" in callee) {
			return this.applyOpcode((callee as OpcodeMarker).__opcode, args);
		}
		return this.navigate(callee, args);
	}

	private evalArray(_prefix: number) {
		this.ensure("[");
		this.skipNonCode();
		this.skipIndexHeaderIfPresent();
		const out: unknown[] = [];
		while (true) {
			this.skipNonCode();
			if (this.text[this.pos] === "]") break;
			out.push(this.evalValue());
		}
		this.ensure("]");
		return out;
	}

	private evalObject(_prefix: number) {
		this.ensure("{");
		this.skipNonCode();
		this.skipIndexHeaderIfPresent();
		const out: Record<string, unknown> = {};
		while (true) {
			this.skipNonCode();
			if (this.text[this.pos] === "}") break;
			const key = this.evalValue();
			const value = this.evalValue();
			out[String(key)] = value;
		}
		this.ensure("}");
		return out;
	}

	private skipIndexHeaderIfPresent() {
		const save = this.pos;
		const countPrefix = this.readPrefix();
		if (this.text[this.pos] !== "#") {
			this.pos = save;
			return;
		}
		this.pos += 1;
		const widthChar = this.text[this.pos];
		const width = widthChar ? digitMap.get(widthChar) ?? 0 : 0;
		if (widthChar) this.pos += 1;
		const skipLen = countPrefix.value * width;
		this.pos += skipLen;
	}

	private evalFlowParen(tag: "?" | "!" | "|" | "&") {
		this.pos += 1;
		this.ensure("(");
		if (tag === "?") {
			const cond = this.evalValue();
			if (isDefined(cond)) {
				const thenValue = this.evalValue();
				if (this.hasMoreBefore(")")) this.skipValue();
				this.ensure(")");
				return thenValue;
			}
			this.skipValue();
			let elseValue: unknown = undefined;
			if (this.hasMoreBefore(")")) elseValue = this.evalValue();
			this.ensure(")");
			return elseValue;
		}
		if (tag === "!") {
			const cond = this.evalValue();
			if (!isDefined(cond)) {
				const thenValue = this.evalValue();
				if (this.hasMoreBefore(")")) this.skipValue();
				this.ensure(")");
				return thenValue;
			}
			this.skipValue();
			let elseValue: unknown = undefined;
			if (this.hasMoreBefore(")")) elseValue = this.evalValue();
			this.ensure(")");
			return elseValue;
		}
		if (tag === "|") {
			let result: unknown = undefined;
			while (this.hasMoreBefore(")")) {
				if (isDefined(result)) this.skipValue();
				else result = this.evalValue();
			}
			this.ensure(")");
			return result;
		}
		let result: unknown = undefined;
		while (this.hasMoreBefore(")")) {
			if (!isDefined(result) && result !== undefined) {
				this.skipValue();
				continue;
			}
			result = this.evalValue();
			if (!isDefined(result)) {
				while (this.hasMoreBefore(")")) this.skipValue();
				break;
			}
		}
		this.ensure(")");
		return result;
	}

	private evalLoopLike(tag: ">" | "<") {
		this.pos += 1;
		const open = this.text[this.pos];
		if (!open || !"([{".includes(open)) throw new Error(`Expected loop opener after '${tag}'`);
		const close = open === "(" ? ")" : open === "[" ? "]" : "}";
		this.pos += 1;

		const iterable = this.evalValue();
		const afterIterable = this.pos;
		const bodyValueCount = open === "{" ? 2 : 1;

		let varA: string | undefined;
		let varB: string | undefined;
		let bodyStart = afterIterable;
		let bodyEnd = afterIterable;

		let matched = false;
		for (const bindingCount of [2, 1, 0]) {
			this.pos = afterIterable;
			const vars: string[] = [];
			let bindingsOk = true;
			for (let index = 0; index < bindingCount; index += 1) {
				const binding = this.readBindingVarIfPresent();
				if (!binding) {
					bindingsOk = false;
					break;
				}
				vars.push(binding);
			}
			if (!bindingsOk) continue;

			const candidateBodyStart = this.pos;
			let cursor = candidateBodyStart;
			let bodyOk = true;
			for (let index = 0; index < bodyValueCount; index += 1) {
				const next = this.skipValueFrom(cursor);
				if (next <= cursor) {
					bodyOk = false;
					break;
				}
				cursor = next;
			}
			if (!bodyOk) continue;

			this.pos = cursor;
			this.skipNonCode();
			if (this.text[this.pos] !== close) continue;

			varA = vars[0];
			varB = vars[1];
			bodyStart = candidateBodyStart;
			bodyEnd = cursor;
			matched = true;
			break;
		}

		if (!matched) throw new Error(`Invalid loop/comprehension body before '${close}' at ${this.pos}`);
		this.pos = bodyEnd;
		this.ensure(close);

		if (open === "[") return this.evalArrayComprehension(iterable, varA, varB, bodyStart, bodyEnd, tag === "<");
		if (open === "{") return this.evalObjectComprehension(iterable, varA, varB, bodyStart, bodyEnd, tag === "<");
		return this.evalForLoop(iterable, varA, varB, bodyStart, bodyEnd, tag === "<");
	}

	private iterate(iterable: unknown, keysOnly: boolean): Array<{ key: unknown; value: unknown }> {
		if (Array.isArray(iterable)) {
			if (keysOnly) return iterable.map((_value, index) => ({ key: index, value: index }));
			return iterable.map((value, index) => ({ key: index, value }));
		}
		if (iterable && typeof iterable === "object") {
			const entries = Object.entries(iterable as Record<string, unknown>);
			if (keysOnly) return entries.map(([key]) => ({ key, value: key }));
			return entries.map(([key, value]) => ({ key, value }));
		}
		if (typeof iterable === "number" && Number.isFinite(iterable) && iterable > 0) {
			const out: Array<{ key: unknown; value: unknown }> = [];
			for (let index = 0; index < Math.floor(iterable); index += 1) {
				if (keysOnly) out.push({ key: index, value: index });
				else out.push({ key: index, value: index + 1 });
			}
			return out;
		}
		return [];
	}

	private evalBodySlice(start: number, end: number): unknown {
		const save = this.pos;
		this.pos = start;
		const value = this.evalValue();
		this.pos = end;
		this.pos = save;
		return value;
	}

	private handleLoopControl(value: unknown): LoopControl | undefined {
		if (value && typeof value === "object" && "kind" in value && "depth" in value) {
			return value as LoopControl;
		}
		return undefined;
	}

	private evalForLoop(iterable: unknown, varA: string | undefined, varB: string | undefined, bodyStart: number, bodyEnd: number, keysOnly: boolean): unknown {
		const items = this.iterate(iterable, keysOnly);
		let last: unknown = undefined;
		for (const item of items) {
			const currentSelf = keysOnly ? item.key : item.value;
			this.selfStack.push(currentSelf);
			if (varA && varB) {
				this.state.vars[varA] = item.key;
				this.state.vars[varB] = keysOnly ? item.key : item.value;
			}
			else if (varA) {
				this.state.vars[varA] = keysOnly ? item.key : item.value;
			}
			last = this.evalBodySlice(bodyStart, bodyEnd);
			this.selfStack.pop();
			const control = this.handleLoopControl(last);
			if (!control) continue;
			if (control.depth > 1) return { kind: control.kind, depth: control.depth - 1 } satisfies LoopControl;
			if (control.kind === "break") return undefined;
			last = undefined;
			continue;
		}
		return last;
	}

	private evalArrayComprehension(iterable: unknown, varA: string | undefined, varB: string | undefined, bodyStart: number, bodyEnd: number, keysOnly: boolean): unknown[] | LoopControl {
		const items = this.iterate(iterable, keysOnly);
		const out: unknown[] = [];
		for (const item of items) {
			const currentSelf = keysOnly ? item.key : item.value;
			this.selfStack.push(currentSelf);
			if (varA && varB) {
				this.state.vars[varA] = item.key;
				this.state.vars[varB] = keysOnly ? item.key : item.value;
			}
			else if (varA) {
				this.state.vars[varA] = keysOnly ? item.key : item.value;
			}
			const value = this.evalBodySlice(bodyStart, bodyEnd);
			this.selfStack.pop();
			const control = this.handleLoopControl(value);
			if (control) {
				if (control.depth > 1) return { kind: control.kind, depth: control.depth - 1 } satisfies LoopControl;
				if (control.kind === "break") break;
				continue;
			}
			if (isDefined(value)) out.push(value);
		}
		return out;
	}

	private evalObjectComprehension(iterable: unknown, varA: string | undefined, varB: string | undefined, bodyStart: number, bodyEnd: number, keysOnly: boolean): Record<string, unknown> | LoopControl {
		const items = this.iterate(iterable, keysOnly);
		const out: Record<string, unknown> = {};
		for (const item of items) {
			const currentSelf = keysOnly ? item.key : item.value;
			this.selfStack.push(currentSelf);
			if (varA && varB) {
				this.state.vars[varA] = item.key;
				this.state.vars[varB] = keysOnly ? item.key : item.value;
			}
			else if (varA) {
				this.state.vars[varA] = keysOnly ? item.key : item.value;
			}
			const save = this.pos;
			this.pos = bodyStart;
			const key = this.evalValue();
			const value = this.evalValue();
			this.pos = save;
			this.selfStack.pop();
			const control = this.handleLoopControl(value);
			if (control) {
				if (control.depth > 1) return { kind: control.kind, depth: control.depth - 1 } satisfies LoopControl;
				if (control.kind === "break") break;
				continue;
			}
			if (isDefined(value)) out[String(key)] = value;
		}
		return out;
	}

	private applyOpcode(id: number, args: unknown[]): unknown {
		const custom = this.customOpcodes.get(id);
		if (custom) return custom(args, this.state);
		switch (id) {
			case OPCODES.do:
				return args.length ? args[args.length - 1] : undefined;
			case OPCODES.add:
				return Number(args[0] ?? 0) + Number(args[1] ?? 0);
			case OPCODES.sub:
				return Number(args[0] ?? 0) - Number(args[1] ?? 0);
			case OPCODES.mul:
				return Number(args[0] ?? 0) * Number(args[1] ?? 0);
			case OPCODES.div:
				return Number(args[0] ?? 0) / Number(args[1] ?? 0);
			case OPCODES.mod:
				return Number(args[0] ?? 0) % Number(args[1] ?? 0);
			case OPCODES.neg:
				return -Number(args[0] ?? 0);
			case OPCODES.not: {
				const value = args[0];
				if (typeof value === "boolean") return !value;
				return ~Number(value ?? 0);
			}
			case OPCODES.and: {
				const [a, b] = args;
				if (typeof a === "boolean" || typeof b === "boolean") return Boolean(a) && Boolean(b);
				return Number(a ?? 0) & Number(b ?? 0);
			}
			case OPCODES.or: {
				const [a, b] = args;
				if (typeof a === "boolean" || typeof b === "boolean") return Boolean(a) || Boolean(b);
				return Number(a ?? 0) | Number(b ?? 0);
			}
			case OPCODES.xor: {
				const [a, b] = args;
				if (typeof a === "boolean" || typeof b === "boolean") return Boolean(a) !== Boolean(b);
				return Number(a ?? 0) ^ Number(b ?? 0);
			}
			case OPCODES.eq:
				return args[0] === args[1] ? args[0] : undefined;
			case OPCODES.neq:
				return args[0] !== args[1] ? args[0] : undefined;
			case OPCODES.gt:
				return Number(args[0]) > Number(args[1]) ? args[0] : undefined;
			case OPCODES.gte:
				return Number(args[0]) >= Number(args[1]) ? args[0] : undefined;
			case OPCODES.lt:
				return Number(args[0]) < Number(args[1]) ? args[0] : undefined;
			case OPCODES.lte:
				return Number(args[0]) <= Number(args[1]) ? args[0] : undefined;
			case OPCODES.boolean:
				return typeof args[0] === "boolean" ? args[0] : undefined;
			case OPCODES.number:
				return typeof args[0] === "number" ? args[0] : undefined;
			case OPCODES.string:
				return typeof args[0] === "string" ? args[0] : undefined;
			case OPCODES.array:
				return Array.isArray(args[0]) ? args[0] : undefined;
			case OPCODES.object:
				return args[0] && typeof args[0] === "object" && !Array.isArray(args[0]) ? args[0] : undefined;
			default:
				throw new Error(`Unknown opcode ${id}`);
		}
	}

	private navigate(base: unknown, keys: unknown[]): unknown {
		let current = base;
		for (const key of keys) {
			if (current === undefined || current === null) return undefined;
			current = (current as Record<string, unknown>)[String(key)];
		}
		return current;
	}

	private readPlace(): { root: string | number; keys: unknown[]; isRef: boolean } {
		this.skipNonCode();
		const prefix = this.readPrefix();
		const tag = this.text[this.pos];
		if (tag === "$" || tag === "'") {
			this.pos += 1;
			const keys: unknown[] = [];
			this.skipNonCode();
			if (this.text[this.pos] === "(") {
				this.pos += 1;
				while (true) {
					this.skipNonCode();
					if (this.text[this.pos] === ")") break;
					keys.push(this.evalValue());
				}
				this.pos += 1;
			}
			return {
				root: tag === "$" ? prefix.raw : prefix.value,
				keys,
				isRef: tag === "'",
			};
		}
		throw new Error(`Invalid place at ${this.pos}`);
	}

	private writePlace(place: { root: string | number; keys: unknown[]; isRef: boolean }, value: unknown) {
		const rootTable = place.isRef ? this.state.refs : this.state.vars;
		const rootKey = String(place.root);
		if (place.keys.length === 0) {
			rootTable[rootKey] = value;
			return;
		}
		let target = rootTable[rootKey];
		if (!target || typeof target !== "object") {
			target = {};
			rootTable[rootKey] = target;
		}
		for (let index = 0; index < place.keys.length - 1; index += 1) {
			const key = String(place.keys[index]);
			const next = (target as Record<string, unknown>)[key];
			if (!next || typeof next !== "object") (target as Record<string, unknown>)[key] = {};
			target = (target as Record<string, unknown>)[key];
		}
		(target as Record<string, unknown>)[String(place.keys[place.keys.length - 1])] = value;
	}

	private deletePlace(place: { root: string | number; keys: unknown[]; isRef: boolean }) {
		const rootTable = place.isRef ? this.state.refs : this.state.vars;
		const rootKey = String(place.root);
		if (place.keys.length === 0) {
			delete rootTable[rootKey];
			return;
		}
		let target = rootTable[rootKey];
		if (!target || typeof target !== "object") return;
		for (let index = 0; index < place.keys.length - 1; index += 1) {
			target = (target as Record<string, unknown>)[String(place.keys[index])];
			if (!target || typeof target !== "object") return;
		}
		delete (target as Record<string, unknown>)[String(place.keys[place.keys.length - 1])];
	}

	private skipValue() {
		this.pos = this.skipValueFrom(this.pos);
	}

	private skipValueFrom(startPos: number): number {
		const save = this.pos;
		this.pos = startPos;
		this.skipNonCode();
		const prefix = this.readPrefix();
		const tag = this.text[this.pos];
		if (!tag) {
			this.pos = save;
			return startPos;
		}

		if (tag === ",") {
			this.pos += 1 + prefix.value;
			const end = this.pos;
			this.pos = save;
			return end;
		}
		if (tag === "=") {
			this.pos += 1;
			this.skipValue();
			this.skipValue();
			const end = this.pos;
			this.pos = save;
			return end;
		}
		if (tag === "~") {
			this.pos += 1;
			this.skipValue();
			const end = this.pos;
			this.pos = save;
			return end;
		}
		if (tag === "*") {
			this.pos += 1;
			this.skipValue();
			const end = this.pos;
			this.pos = save;
			return end;
		}
		if ("+:%$@'^;".includes(tag)) {
			this.pos += 1;
			const end = this.pos;
			this.pos = save;
			return end;
		}
		if (tag === "?" || tag === "!" || tag === "|" || tag === "&" || tag === ">" || tag === "<") {
			this.pos += 1;
		}
		const opener = this.text[this.pos];
		if (opener && "([{".includes(opener)) {
			const close = opener === "(" ? ")" : opener === "[" ? "]" : "}";
			if (prefix.value > 0) {
				this.pos += 1 + prefix.value + 1;
				const end = this.pos;
				this.pos = save;
				return end;
			}
			this.pos += 1;
			while (true) {
				this.skipNonCode();
				if (this.text[this.pos] === close) break;
				this.skipValue();
			}
			this.pos += 1;
			const end = this.pos;
			this.pos = save;
			return end;
		}
		this.pos += 1;
		const end = this.pos;
		this.pos = save;
		return end;
	}
}

export function evaluateRexc(text: string, ctx: RexcContext = {}) {
	const interpreter = new CursorInterpreter(text, ctx);
	const value = interpreter.evaluateTopLevel();
	return { value, state: interpreter.runtimeState };
}

export function evaluateSource(source: string, ctx: RexcContext = {}) {
	return evaluateRexc(compile(source), ctx);
}

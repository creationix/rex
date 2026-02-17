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

function parseNumber(raw: string) {
	if (/^-?0x/i.test(raw)) return parseInt(raw, 16);
	if (/^-?0b/i.test(raw)) return parseInt(raw, 2);
	return Number(raw);
}

function unwrap(node: { toIR(): IRNode }): IRNode {
	return node.toIR();
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

function buildNavigation(base: IRNode, tailNodes: IRNode[]) {
	const nav: Extract<IRNode, { type: "navigation" }> = {
		type: "navigation",
		target: base,
		segments: [],
	};
	for (const tail of tailNodes) {
		if (tail.type === "call") {
			if (nav.segments.length === 0) {
				return { type: "call", callee: nav.target, args: tail.args } satisfies IRNode;
			}
			return { type: "call", callee: nav, args: tail.args } satisfies IRNode;
		}
		if (tail.type === "navigation") {
			nav.segments.push(...tail.segments);
		}
	}
	return nav.segments.length === 0 ? base : nav;
}

semantics.addOperation("toIR", {
	_iter(...children) {
		return children.map((child) => child.toIR());
	},
	_terminal() {
		return this.sourceString;
	},
	_nonterminal(...children) {
		if (children.length === 1) return children[0].toIR();
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
		return { type: "binary", op: map[op.sourceString], left: left.toIR(), right: right.toIR() } satisfies IRNode;
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
		return { type: "unary", op: "neg", value: value.toIR() } satisfies IRNode;
	},
	UnaryExpr_not(_op, value) {
		return { type: "unary", op: "not", value: value.toIR() } satisfies IRNode;
	},
	UnaryExpr_delete(_del, place) {
		return { type: "unary", op: "delete", value: place.toIR() } satisfies IRNode;
	},

	PostfixExpr_chain(base, tails) {
		return buildNavigation(base.toIR(), tails.children.map(unwrap));
	},
	Place(base, tails) {
		return buildNavigation(base.toIR(), tails.children.map(unwrap));
	},
	PostfixTail_navStatic(_dot, key) {
		return {
			type: "navigation",
			target: { type: "identifier", name: "$placeholder" },
			segments: [{ type: "static", key: key.sourceString }],
		} satisfies IRNode;
	},
	PostfixTail_navDynamic(_dotOpen, key, _close) {
		return {
			type: "navigation",
			target: { type: "identifier", name: "$placeholder" },
			segments: [{ type: "dynamic", key: key.toIR() }],
		} satisfies IRNode;
	},
	PostfixTail_callEmpty(_open, _close) {
		return { type: "call", callee: { type: "identifier", name: "$placeholder" }, args: [] } satisfies IRNode;
	},
	PostfixTail_call(_open, args, _close) {
		return {
			type: "call",
			callee: { type: "identifier", name: "$placeholder" },
			args: normalizeList(args.toIR()) as IRNode[],
		} satisfies IRNode;
	},

	ConditionalExpr(head, condition, _do, thenBlock, elseBranch, _end) {
		return {
			type: "conditional",
			head: head.toIR() as "when" | "unless",
			condition: condition.toIR(),
			thenBlock: thenBlock.toIR() as IRNode[],
			elseBranch: elseBranch.children.length ? (elseBranch.children[0].toIR() as IRConditionalElse) : undefined,
		} satisfies IRNode;
	},
	ConditionalHead(_kw) {
		return this.sourceString as "when" | "unless";
	},
	ConditionalElse_elseChain(_else, head, condition, _do, thenBlock, elseBranch) {
		return {
			type: "elseChain",
			head: head.toIR() as "when" | "unless",
			condition: condition.toIR(),
			thenBlock: thenBlock.toIR() as IRNode[],
			elseBranch: elseBranch.children.length ? (elseBranch.children[0].toIR() as IRConditionalElse) : undefined,
		} satisfies IRConditionalElse;
	},
	ConditionalElse_else(_else, block) {
		return { type: "else", block: block.toIR() as IRNode[] } satisfies IRConditionalElse;
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

import rexGrammar from "./rex.ohm-bundle.js";

export const grammar = rexGrammar;
export const semantics = rexGrammar.createSemantics();

semantics.addOperation("toJSON", {
	_iter(...children) {
		return children.map((child) => child.toJSON());
	},
	Program(expressions) {
		if (expressions.children.length > 1) {
			return ["$do", ...expressions.children.map((child) => child.toJSON())];
		}
		if (expressions.children.length === 1 && expressions.children[0]) {
			const [child] = expressions.children;
			return child.toJSON();
		}
		return null;
	},
	Expr_atom(atom) {
		return atom.toJSON();
	},
	Expr_binding(ident, _eq, expr) {
		// console.log({ ident, expr });
		throw new Error("TODO: Expr_binding");
	},
	Expr_call(_lp, action, args, _rp) {
		console.log({
			action: action.sourceString,
			args: args.children.map((arg) => arg.toJSON()),
		});
		// console.log({ action, args });
		throw new Error("TODO: Expr_call");
	},
	Atom_string(string) {
		throw new Error("TODO: Atom_string");
	},
	hexNumber(arg0, _0x, arg1) {
		return (
			parseInt(arg1.sourceString, 16) * (arg0.sourceString === "-" ? -1 : 1)
		);
	},
	binaryNumber(arg0, _0b, arg1) {
		return (
			parseInt(arg1.sourceString, 2) * (arg0.sourceString === "-" ? -1 : 1)
		);
	},
	decimalNumber(_neg, _intPart, _fracPart, _expPart, _signPart) {
		return parseFloat(this.sourceString);
	},
	Atom_bytes(bytes) {
		throw new Error("TODO: Atom_bytes");
	},
	Atom_boolean(bool) {
		throw new Error("TODO: Atom_boolean");
	},
	Atom_null(_null) {
		return null;
	},
	Atom_undefined(_undefined) {
		return undefined;
	},
	Atom_array(array) {
		throw new Error("TODO: Atom_array");
	},
	Atom_object(object) {
		throw new Error("TODO: Atom_object");
	},
	Atom_symbol(symbol) {
		throw new Error("TODO: Atom_symbol");
	},
	Array(arg0, arg1, arg2) {
		throw new Error("TODO: Array");
	},
	Object(arg0, arg1, arg2) {
		throw new Error("TODO: Object");
	},
	Bytes(arg0, arg1, arg2) {
		throw new Error("TODO: Bytes");
	},
	HexByte(arg0, arg1) {
		throw new Error("TODO: HexByte");
	},
	Pair(arg0, arg1, arg2) {
		throw new Error("TODO: Pair");
	},
	Any(arg0, arg1) {
		throw new Error("TODO: Any");
	},
	String(arg0) {
		throw new Error("TODO: String");
	},
	DQuotedString(arg0, arg1, arg2) {
		throw new Error("TODO: DQuotedString");
	},
	dStringChar(arg0) {
		throw new Error("TODO: dStringChar");
	},
	SQuotedString(arg0, arg1, arg2) {
		throw new Error("TODO: SQuotedString");
	},
	sStringChar(arg0) {
		throw new Error("TODO: sStringChar");
	},
	escape(arg0, arg1) {
		throw new Error("TODO: escape");
	},
	unicodeEscape(arg0, arg1, arg2, arg3, arg4) {
		throw new Error("TODO: unicodeEscape");
	},
	hex(arg0) {
		throw new Error("TODO: hex");
	},
	decimalInteger_nonZero(arg0, arg1) {
		throw new Error("TODO: decimalInteger_nonZero");
	},
	decimalInteger_zero(arg0) {
		throw new Error("TODO: decimalInteger_zero");
	},
	decimalInteger(arg0) {
		throw new Error("TODO: decimalInteger");
	},
	bit(arg0) {
		throw new Error("TODO: bit");
	},
	exponent(arg0, arg1, arg2) {
		throw new Error("TODO: exponent");
	},
	bareWord(arg0, arg1) {
		throw new Error("TODO: bareWord");
	},
	Boolean(arg0) {
		throw new Error("TODO: Boolean");
	},
	Null(arg0) {
		throw new Error("TODO: Null");
	},
	Undefined(arg0) {
		throw new Error("TODO: Undefined");
	},
	nonZeroDigit(arg0) {
		throw new Error("TODO: nonZeroDigit");
	},
	comment(arg0) {
		throw new Error("TODO: comment");
	},
	lineComment(arg0, arg1, arg2) {
		throw new Error("TODO: lineComment");
	},
	blockComment(arg0, arg1, arg2) {
		throw new Error("TODO: blockComment");
	},
});

export default semantics;

export type {
	RexActionDict,
	RexSemantics,
} from "./rex.ohm-bundle.js";

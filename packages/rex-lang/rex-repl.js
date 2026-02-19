// rex-repl.ts
import * as readline from "node:readline";
import { createRequire as createRequire2 } from "node:module";

// rex.ts
import { createRequire } from "node:module";
var require2 = createRequire(import.meta.url);
var rexGrammarModule = require2("./rex.ohm-bundle.cjs");
var rexGrammar = rexGrammarModule?.default ?? rexGrammarModule;
var grammar = rexGrammar;
var semantics = rexGrammar.createSemantics();
var DIGITS = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";
function byteLength(value) {
  return Buffer.byteLength(value, "utf8");
}
var OPCODE_IDS = {
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
  neg: 21
};
var FIRST_NON_RESERVED_REF = 5;
var DOMAIN_DIGIT_INDEX = new Map(Array.from(DIGITS).map((char, index) => [char, index]));
var BINARY_TO_OPCODE = {
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
  lte: "lte"
};
var ASSIGN_COMPOUND_TO_OPCODE = {
  "+=": "add",
  "-=": "sub",
  "*=": "mul",
  "/=": "div",
  "%=": "mod",
  "&=": "and",
  "|=": "or",
  "^=": "xor"
};
function encodeUint(value) {
  if (!Number.isInteger(value) || value < 0)
    throw new Error(`Cannot encode non-uint value: ${value}`);
  if (value === 0)
    return "";
  let current = value;
  let out = "";
  while (current > 0) {
    const digit = current % 64;
    out = `${DIGITS[digit]}${out}`;
    current = Math.floor(current / 64);
  }
  return out;
}
function encodeZigzag(value) {
  if (!Number.isInteger(value))
    throw new Error(`Cannot zigzag non-integer: ${value}`);
  const encoded = value >= 0 ? value * 2 : -value * 2 - 1;
  return encodeUint(encoded);
}
function encodeInt(value) {
  return `${encodeZigzag(value)}+`;
}
function canUseBareString(value) {
  for (const char of value) {
    if (!DIGITS.includes(char))
      return false;
  }
  return true;
}
function decodeStringLiteral(raw) {
  const quote = raw[0];
  if (quote !== '"' && quote !== "'" || raw[raw.length - 1] !== quote) {
    throw new Error(`Invalid string literal: ${raw}`);
  }
  let out = "";
  for (let index = 1;index < raw.length - 1; index += 1) {
    const char = raw[index];
    if (char !== "\\") {
      out += char;
      continue;
    }
    index += 1;
    const esc = raw[index];
    if (esc === undefined)
      throw new Error(`Invalid escape sequence in ${raw}`);
    if (esc === "n")
      out += `
`;
    else if (esc === "r")
      out += "\r";
    else if (esc === "t")
      out += "\t";
    else if (esc === "b")
      out += "\b";
    else if (esc === "f")
      out += "\f";
    else if (esc === "v")
      out += "\v";
    else if (esc === "0")
      out += "\x00";
    else if (esc === "x") {
      const hex = raw.slice(index + 1, index + 3);
      if (!/^[0-9a-fA-F]{2}$/.test(hex))
        throw new Error(`Invalid hex escape in ${raw}`);
      out += String.fromCodePoint(parseInt(hex, 16));
      index += 2;
    } else if (esc === "u") {
      const hex = raw.slice(index + 1, index + 5);
      if (!/^[0-9a-fA-F]{4}$/.test(hex))
        throw new Error(`Invalid unicode escape in ${raw}`);
      out += String.fromCodePoint(parseInt(hex, 16));
      index += 4;
    } else {
      out += esc;
    }
  }
  return out;
}
function encodeBareOrLengthString(value) {
  if (canUseBareString(value))
    return `${value}:`;
  return `${encodeUint(byteLength(value))},${value}`;
}
function encodeNumberNode(node) {
  const numberValue = node.value;
  if (!Number.isFinite(numberValue))
    throw new Error(`Cannot encode non-finite number: ${node.raw}`);
  if (Number.isInteger(numberValue))
    return encodeInt(numberValue);
  const raw = node.raw.toLowerCase();
  const sign = raw.startsWith("-") ? -1 : 1;
  const unsigned = sign < 0 ? raw.slice(1) : raw;
  const splitExp = unsigned.split("e");
  const mantissaText = splitExp[0];
  const exponentText = splitExp[1] ?? "0";
  if (!mantissaText)
    throw new Error(`Invalid decimal literal: ${node.raw}`);
  const exponent = Number(exponentText);
  if (!Number.isInteger(exponent))
    throw new Error(`Invalid decimal exponent: ${node.raw}`);
  const dotIndex = mantissaText.indexOf(".");
  const decimals = dotIndex === -1 ? 0 : mantissaText.length - dotIndex - 1;
  const digits = mantissaText.replace(".", "");
  if (!/^\d+$/.test(digits))
    throw new Error(`Invalid decimal digits: ${node.raw}`);
  let significand = Number(digits) * sign;
  let power = exponent - decimals;
  while (significand !== 0 && significand % 10 === 0) {
    significand /= 10;
    power += 1;
  }
  return `${encodeZigzag(power)}*${encodeInt(significand)}`;
}
function encodeOpcode(opcode) {
  return `${encodeUint(OPCODE_IDS[opcode])}%`;
}
function encodeCallParts(parts) {
  return `(${parts.join("")})`;
}
function needsOptionalPrefix(encoded) {
  const first = encoded[0];
  if (!first)
    return false;
  return first === "[" || first === "{" || first === "(" || first === "=" || first === "~" || first === "?" || first === "!" || first === "|" || first === "&" || first === ">" || first === "<" || first === "#";
}
function addOptionalPrefix(encoded) {
  if (!needsOptionalPrefix(encoded))
    return encoded;
  let payload = encoded;
  if (encoded.startsWith("?(") || encoded.startsWith("!(") || encoded.startsWith("|(") || encoded.startsWith("&(") || encoded.startsWith(">(") || encoded.startsWith("<(") || encoded.startsWith("#(")) {
    payload = encoded.slice(2, -1);
  } else if (encoded.startsWith(">[") || encoded.startsWith(">{")) {
    payload = encoded.slice(2, -1);
  } else if (encoded.startsWith("[") || encoded.startsWith("{") || encoded.startsWith("(")) {
    payload = encoded.slice(1, -1);
  } else if (encoded.startsWith("=") || encoded.startsWith("~")) {
    payload = encoded.slice(1);
  }
  return `${encodeUint(byteLength(payload))}${encoded}`;
}
function encodeBlockExpression(block) {
  if (block.length === 0)
    return "4'";
  if (block.length === 1)
    return encodeNode(block[0]);
  return encodeCallParts([encodeOpcode("do"), ...block.map((node) => encodeNode(node))]);
}
function encodeConditionalElse(elseBranch) {
  if (elseBranch.type === "else")
    return encodeBlockExpression(elseBranch.block);
  const nested = {
    type: "conditional",
    head: elseBranch.head,
    condition: elseBranch.condition,
    thenBlock: elseBranch.thenBlock,
    elseBranch: elseBranch.elseBranch
  };
  return encodeNode(nested);
}
function encodeNavigation(node) {
  const domainRefs = activeEncodeOptions?.domainRefs;
  if (domainRefs && node.target.type === "identifier") {
    const staticPath = [node.target.name];
    for (const segment of node.segments) {
      if (segment.type !== "static")
        break;
      staticPath.push(segment.key);
    }
    for (let pathLength = staticPath.length;pathLength >= 1; pathLength -= 1) {
      const dottedName = staticPath.slice(0, pathLength).join(".");
      const domainRef = domainRefs[dottedName];
      if (domainRef === undefined)
        continue;
      const consumedStaticSegments = pathLength - 1;
      if (consumedStaticSegments === node.segments.length) {
        return `${encodeUint(domainRef)}'`;
      }
      const parts2 = [`${encodeUint(domainRef)}'`];
      for (const segment of node.segments.slice(consumedStaticSegments)) {
        if (segment.type === "static")
          parts2.push(encodeBareOrLengthString(segment.key));
        else
          parts2.push(encodeNode(segment.key));
      }
      return encodeCallParts(parts2);
    }
  }
  const parts = [encodeNode(node.target)];
  for (const segment of node.segments) {
    if (segment.type === "static")
      parts.push(encodeBareOrLengthString(segment.key));
    else
      parts.push(encodeNode(segment.key));
  }
  return encodeCallParts(parts);
}
function encodeWhile(node) {
  const cond = encodeNode(node.condition);
  const body = addOptionalPrefix(encodeBlockExpression(node.body));
  return `#(${cond}${body})`;
}
function encodeFor(node) {
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
function encodeArrayComprehension(node) {
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
function encodeObjectComprehension(node) {
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
var activeEncodeOptions;
function encodeNode(node) {
  switch (node.type) {
    case "program":
      return encodeBlockExpression(node.body);
    case "identifier": {
      const domainRef = activeEncodeOptions?.domainRefs?.[node.name];
      if (domainRef !== undefined)
        return `${encodeUint(domainRef)}'`;
      return `${node.name}$`;
    }
    case "self":
      return "@";
    case "selfDepth": {
      if (!Number.isInteger(node.depth) || node.depth < 1)
        throw new Error(`Invalid self depth: ${node.depth}`);
      if (node.depth === 1)
        return "@";
      return `${encodeUint(node.depth - 1)}@`;
    }
    case "boolean":
      return node.value ? "1'" : "2'";
    case "null":
      return "3'";
    case "undefined":
      return "4'";
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
      const body = node.entries.map(({ key, value }) => `${encodeNode(key)}${addOptionalPrefix(encodeNode(value))}`).join("");
      return `{${body}}`;
    }
    case "objectComprehension":
      return encodeObjectComprehension(node);
    case "key":
      return encodeBareOrLengthString(node.name);
    case "group":
      return encodeNode(node.expression);
    case "unary":
      if (node.op === "delete")
        return `~${encodeNode(node.value)}`;
      if (node.op === "neg")
        return encodeCallParts([encodeOpcode("neg"), encodeNode(node.value)]);
      return encodeCallParts([encodeOpcode("not"), encodeNode(node.value)]);
    case "binary":
      if (node.op === "and") {
        const operands = collectLogicalChain(node, "and");
        const body = operands.map((operand, index) => {
          const encoded = encodeNode(operand);
          return index === 0 ? encoded : addOptionalPrefix(encoded);
        }).join("");
        return `&(${body})`;
      }
      if (node.op === "or") {
        const operands = collectLogicalChain(node, "or");
        const body = operands.map((operand, index) => {
          const encoded = encodeNode(operand);
          return index === 0 ? encoded : addOptionalPrefix(encoded);
        }).join("");
        return `|(${body})`;
      }
      return encodeCallParts([
        encodeOpcode(BINARY_TO_OPCODE[node.op]),
        encodeNode(node.left),
        encodeNode(node.right)
      ]);
    case "assign": {
      if (node.op === "=")
        return `=${encodeNode(node.place)}${addOptionalPrefix(encodeNode(node.value))}`;
      const opcode = ASSIGN_COMPOUND_TO_OPCODE[node.op];
      if (!opcode)
        throw new Error(`Unsupported assignment op: ${node.op}`);
      const computedValue = encodeCallParts([encodeOpcode(opcode), encodeNode(node.place), encodeNode(node.value)]);
      return `=${encodeNode(node.place)}${addOptionalPrefix(computedValue)}`;
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
    case "while":
      return encodeWhile(node);
    case "break":
      return ";";
    case "continue":
      return "1;";
    default: {
      const exhaustive = node;
      throw new Error(`Unsupported IR node ${exhaustive.type ?? "unknown"}`);
    }
  }
}
function collectLogicalChain(node, op) {
  if (node.type !== "binary" || node.op !== op)
    return [node];
  return [...collectLogicalChain(node.left, op), ...collectLogicalChain(node.right, op)];
}
function parseToIR(source) {
  const match = grammar.match(source);
  if (!match.succeeded()) {
    const failure = match;
    throw new Error(failure.message ?? "Parse failed");
  }
  return semantics(match).toIR();
}
function isPlainObject(value) {
  if (!value || typeof value !== "object" || Array.isArray(value))
    return false;
  const proto = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
}
function isBareKeyName(key) {
  return /^[A-Za-z_][A-Za-z0-9_-]*$/.test(key);
}
function stringifyString(value) {
  return JSON.stringify(value);
}
function stringifyInline(value) {
  if (value === undefined)
    return "undefined";
  if (value === null)
    return "null";
  if (typeof value === "boolean")
    return value ? "true" : "false";
  if (typeof value === "number") {
    if (!Number.isFinite(value))
      throw new Error("Rex stringify() cannot encode non-finite numbers");
    return String(value);
  }
  if (typeof value === "string")
    return stringifyString(value);
  if (Array.isArray(value)) {
    if (value.length === 0)
      return "[]";
    return `[${value.map((item) => stringifyInline(item)).join(" ")}]`;
  }
  if (isPlainObject(value)) {
    const entries = Object.entries(value);
    if (entries.length === 0)
      return "{}";
    const body = entries.map(([key, item]) => `${isBareKeyName(key) ? key : stringifyString(key)}: ${stringifyInline(item)}`).join(" ");
    return `{${body}}`;
  }
  throw new Error(`Rex stringify() cannot encode value of type ${typeof value}`);
}
function fitsInline(rendered, depth, indentSize, maxWidth) {
  if (rendered.includes(`
`))
    return false;
  return depth * indentSize + rendered.length <= maxWidth;
}
function stringifyPretty(value, depth, indentSize, maxWidth) {
  const inline = stringifyInline(value);
  if (fitsInline(inline, depth, indentSize, maxWidth))
    return inline;
  const indent = " ".repeat(depth * indentSize);
  const childIndent = " ".repeat((depth + 1) * indentSize);
  if (Array.isArray(value)) {
    if (value.length === 0)
      return "[]";
    const lines = value.map((item) => {
      const rendered = stringifyPretty(item, depth + 1, indentSize, maxWidth);
      if (!rendered.includes(`
`))
        return `${childIndent}${rendered}`;
      return `${childIndent}${rendered}`;
    });
    return `[
${lines.join(`
`)}
${indent}]`;
  }
  if (isPlainObject(value)) {
    const entries = Object.entries(value);
    if (entries.length === 0)
      return "{}";
    const lines = entries.map(([key, item]) => {
      const keyText = isBareKeyName(key) ? key : stringifyString(key);
      const rendered = stringifyPretty(item, depth + 1, indentSize, maxWidth);
      return `${childIndent}${keyText}: ${rendered}`;
    });
    return `{
${lines.join(`
`)}
${indent}}`;
  }
  return inline;
}
function domainRefsFromConfig(config) {
  if (!config || typeof config !== "object" || Array.isArray(config)) {
    throw new Error("Domain config must be an object");
  }
  const refs = {};
  for (const section of Object.values(config)) {
    if (!section || typeof section !== "object" || Array.isArray(section))
      continue;
    mapConfigEntries(section, refs);
  }
  return refs;
}
function decodeDomainRefKey(refText) {
  if (!refText)
    throw new Error("Domain ref key cannot be empty");
  if (!/^[0-9A-Za-z_-]+$/.test(refText)) {
    throw new Error(`Invalid domain ref key '${refText}' (must use base64 alphabet 0-9a-zA-Z-_)`);
  }
  if (refText.length > 1 && refText[0] === "0") {
    throw new Error(`Invalid domain ref key '${refText}' (leading zeroes are not allowed)`);
  }
  if (/^[1-9]$/.test(refText)) {
    throw new Error(`Invalid domain ref key '${refText}' (reserved by core language)`);
  }
  let value = 0;
  for (const char of refText) {
    const digit = DOMAIN_DIGIT_INDEX.get(char);
    if (digit === undefined)
      throw new Error(`Invalid domain ref key '${refText}'`);
    value = value * 64 + digit;
    if (value > Number.MAX_SAFE_INTEGER) {
      throw new Error(`Invalid domain ref key '${refText}' (must fit in 53 bits)`);
    }
  }
  if (value < FIRST_NON_RESERVED_REF) {
    throw new Error(`Invalid domain ref key '${refText}' (maps to reserved id ${value})`);
  }
  return value;
}
function mapConfigEntries(entries, refs) {
  const sourceKindByRoot = new Map;
  for (const root of Object.keys(refs)) {
    sourceKindByRoot.set(root, "explicit");
  }
  for (const [refText, rawEntry] of Object.entries(entries)) {
    const entry = rawEntry;
    if (!entry || typeof entry !== "object")
      continue;
    if (!Array.isArray(entry.names))
      continue;
    const refId = decodeDomainRefKey(refText);
    for (const rawName of entry.names) {
      if (typeof rawName !== "string")
        continue;
      const existingNameRef = refs[rawName];
      if (existingNameRef !== undefined && existingNameRef !== refId) {
        throw new Error(`Conflicting refs for '${rawName}': ${existingNameRef} vs ${refId}`);
      }
      refs[rawName] = refId;
      const root = rawName.split(".")[0];
      if (!root)
        continue;
      const currentKind = rawName.includes(".") ? "implicit" : "explicit";
      const existing = refs[root];
      if (existing !== undefined) {
        if (existing === refId)
          continue;
        const existingKind = sourceKindByRoot.get(root) ?? "explicit";
        if (currentKind === "explicit") {
          throw new Error(`Conflicting refs for '${root}': ${existing} vs ${refId}`);
        }
        if (existingKind === "explicit")
          continue;
        continue;
      }
      refs[root] = refId;
      sourceKindByRoot.set(root, currentKind);
    }
  }
}
function stringify(value, options) {
  const indent = options?.indent ?? 2;
  const maxWidth = options?.maxWidth ?? 80;
  if (!Number.isInteger(indent) || indent < 0)
    throw new Error("Rex stringify() indent must be a non-negative integer");
  if (!Number.isInteger(maxWidth) || maxWidth < 20)
    throw new Error("Rex stringify() maxWidth must be an integer >= 20");
  return stringifyPretty(value, 0, indent, maxWidth);
}
var DIGIT_SET = new Set(DIGITS.split(""));
var DIGIT_INDEX = new Map(Array.from(DIGITS).map((char, index) => [char, index]));
function readPrefixAt(text, start) {
  let index = start;
  while (index < text.length && DIGIT_SET.has(text[index]))
    index += 1;
  const raw = text.slice(start, index);
  let value = 0;
  for (const char of raw) {
    const digit = DIGIT_INDEX.get(char);
    if (digit === undefined)
      throw new Error(`Invalid prefix in encoded stream at ${start}`);
    value = value * 64 + digit;
  }
  return { end: index, raw, value };
}
function parsePlaceEnd(text, start, out) {
  if (text[start] === "(") {
    let index2 = start + 1;
    while (index2 < text.length && text[index2] !== ")") {
      index2 = parseValueEnd(text, index2, out).end;
    }
    if (text[index2] !== ")")
      throw new Error(`Unterminated place at ${start}`);
    return index2 + 1;
  }
  const prefix = readPrefixAt(text, start);
  const tag = text[prefix.end];
  if (tag !== "$" && tag !== "'")
    throw new Error(`Invalid place at ${start}`);
  let index = prefix.end + 1;
  if (text[index] !== "(")
    return index;
  index += 1;
  while (index < text.length && text[index] !== ")") {
    index = parseValueEnd(text, index, out).end;
  }
  if (text[index] !== ")")
    throw new Error(`Unterminated place at ${start}`);
  return index + 1;
}
function parseValueEnd(text, start, out) {
  const prefix = readPrefixAt(text, start);
  const tag = text[prefix.end];
  if (!tag)
    throw new Error(`Unexpected end of encoded stream at ${start}`);
  if (tag === ",") {
    const strStart = prefix.end + 1;
    const strEnd = strStart + prefix.value;
    if (strEnd > text.length)
      throw new Error(`String overflows encoded stream at ${start}`);
    const raw = text.slice(start, strEnd);
    if (Buffer.byteLength(text.slice(strStart, strEnd), "utf8") !== prefix.value) {
      throw new Error(`Non-ASCII length-string not currently dedupe-safe at ${start}`);
    }
    const span2 = { start, end: strEnd, raw };
    if (out)
      out.push(span2);
    return span2;
  }
  if (tag === "=") {
    const placeEnd = parsePlaceEnd(text, prefix.end + 1, out);
    const valueEnd = parseValueEnd(text, placeEnd, out).end;
    const span2 = { start, end: valueEnd, raw: text.slice(start, valueEnd) };
    if (out)
      out.push(span2);
    return span2;
  }
  if (tag === "~") {
    const placeEnd = parsePlaceEnd(text, prefix.end + 1, out);
    const span2 = { start, end: placeEnd, raw: text.slice(start, placeEnd) };
    if (out)
      out.push(span2);
    return span2;
  }
  if (tag === "(" || tag === "[" || tag === "{") {
    const close = tag === "(" ? ")" : tag === "[" ? "]" : "}";
    let index = prefix.end + 1;
    while (index < text.length && text[index] !== close) {
      index = parseValueEnd(text, index, out).end;
    }
    if (text[index] !== close)
      throw new Error(`Unterminated container at ${start}`);
    const span2 = { start, end: index + 1, raw: text.slice(start, index + 1) };
    if (out)
      out.push(span2);
    return span2;
  }
  if (tag === "?" || tag === "!" || tag === "|" || tag === "&") {
    if (text[prefix.end + 1] !== "(")
      throw new Error(`Expected '(' after '${tag}' at ${start}`);
    let index = prefix.end + 2;
    while (index < text.length && text[index] !== ")") {
      index = parseValueEnd(text, index, out).end;
    }
    if (text[index] !== ")")
      throw new Error(`Unterminated flow container at ${start}`);
    const span2 = { start, end: index + 1, raw: text.slice(start, index + 1) };
    if (out)
      out.push(span2);
    return span2;
  }
  if (tag === ">" || tag === "<") {
    const open = text[prefix.end + 1];
    if (open !== "(" && open !== "[" && open !== "{")
      throw new Error(`Invalid loop opener at ${start}`);
    const close = open === "(" ? ")" : open === "[" ? "]" : "}";
    let index = prefix.end + 2;
    while (index < text.length && text[index] !== close) {
      index = parseValueEnd(text, index, out).end;
    }
    if (text[index] !== close)
      throw new Error(`Unterminated loop container at ${start}`);
    const span2 = { start, end: index + 1, raw: text.slice(start, index + 1) };
    if (out)
      out.push(span2);
    return span2;
  }
  const span = { start, end: prefix.end + 1, raw: text.slice(start, prefix.end + 1) };
  if (out)
    out.push(span);
  return span;
}
function gatherEncodedValueSpans(text) {
  const spans = [];
  let index = 0;
  while (index < text.length) {
    const span = parseValueEnd(text, index, spans);
    index = span.end;
  }
  return spans;
}
function buildPointerToken(pointerStart, targetStart) {
  let offset = targetStart - (pointerStart + 1);
  if (offset < 0)
    return;
  for (let guard = 0;guard < 8; guard += 1) {
    const prefix = encodeUint(offset);
    const recalculated = targetStart - (pointerStart + prefix.length + 1);
    if (recalculated === offset)
      return `${prefix}^`;
    offset = recalculated;
    if (offset < 0)
      return;
  }
  return;
}
function buildDedupeCandidateTable(encoded, minBytes) {
  const spans = gatherEncodedValueSpans(encoded);
  const table = new Map;
  for (const span of spans) {
    const sizeBytes = span.raw.length;
    if (sizeBytes < minBytes)
      continue;
    const prefix = readPrefixAt(encoded, span.start);
    const tag = encoded[prefix.end];
    if (tag !== "{" && tag !== "[" && tag !== "," && tag !== ":")
      continue;
    const offsetFromEnd = encoded.length - span.end;
    const entry = {
      span,
      sizeBytes,
      offsetFromEnd
    };
    if (!table.has(span.raw))
      table.set(span.raw, []);
    table.get(span.raw).push(entry);
  }
  return table;
}
function dedupeLargeEncodedValues(encoded, minBytes = 4) {
  const effectiveMinBytes = Math.max(1, minBytes);
  let current = encoded;
  while (true) {
    const groups = buildDedupeCandidateTable(current, effectiveMinBytes);
    let replaced = false;
    for (const [value, occurrences] of groups.entries()) {
      if (occurrences.length < 2)
        continue;
      const canonical = occurrences[occurrences.length - 1];
      for (let index = occurrences.length - 2;index >= 0; index -= 1) {
        const occurrence = occurrences[index];
        if (occurrence.span.end > canonical.span.start)
          continue;
        if (current.slice(occurrence.span.start, occurrence.span.end) !== value)
          continue;
        const canonicalCurrentStart = current.length - canonical.offsetFromEnd - canonical.sizeBytes;
        const pointerToken = buildPointerToken(occurrence.span.start, canonicalCurrentStart);
        if (!pointerToken)
          continue;
        if (pointerToken.length >= occurrence.sizeBytes)
          continue;
        current = `${current.slice(0, occurrence.span.start)}${pointerToken}${current.slice(occurrence.span.end)}`;
        replaced = true;
        break;
      }
      if (replaced)
        break;
    }
    if (!replaced)
      return current;
  }
}
function encodeIR(node, options) {
  const previous = activeEncodeOptions;
  activeEncodeOptions = options;
  try {
    const encoded = encodeNode(node);
    if (options?.dedupeValues) {
      return dedupeLargeEncodedValues(encoded, options.dedupeMinBytes ?? 4);
    }
    return encoded;
  } finally {
    activeEncodeOptions = previous;
  }
}
function cloneNode(node) {
  return structuredClone(node);
}
function emptyOptimizeEnv() {
  return { constants: {}, selfCaptures: {} };
}
function cloneOptimizeEnv(env) {
  return {
    constants: { ...env.constants },
    selfCaptures: { ...env.selfCaptures }
  };
}
function clearOptimizeEnv(env) {
  for (const key of Object.keys(env.constants))
    delete env.constants[key];
  for (const key of Object.keys(env.selfCaptures))
    delete env.selfCaptures[key];
}
function clearBinding(env, name) {
  delete env.constants[name];
  delete env.selfCaptures[name];
}
function selfTargetFromNode(node, currentDepth) {
  if (node.type === "self")
    return currentDepth;
  if (node.type === "selfDepth") {
    const target = currentDepth - (node.depth - 1);
    if (target >= 1)
      return target;
  }
  return;
}
function selfNodeFromTarget(targetDepth, currentDepth) {
  const relDepth = currentDepth - targetDepth + 1;
  if (!Number.isInteger(relDepth) || relDepth < 1)
    return;
  if (relDepth === 1)
    return { type: "self" };
  return { type: "selfDepth", depth: relDepth };
}
function dropBindingNames(env, binding) {
  if (binding.type === "binding:valueIn") {
    clearBinding(env, binding.value);
    return;
  }
  if (binding.type === "binding:keyValueIn") {
    clearBinding(env, binding.key);
    clearBinding(env, binding.value);
    return;
  }
  if (binding.type === "binding:keyOf") {
    clearBinding(env, binding.key);
  }
}
function collectReads(node, out) {
  switch (node.type) {
    case "identifier":
      out.add(node.name);
      return;
    case "group":
      collectReads(node.expression, out);
      return;
    case "array":
      for (const item of node.items)
        collectReads(item, out);
      return;
    case "object":
      for (const entry of node.entries) {
        collectReads(entry.key, out);
        collectReads(entry.value, out);
      }
      return;
    case "arrayComprehension":
      collectReads(node.binding.source, out);
      collectReads(node.body, out);
      return;
    case "objectComprehension":
      collectReads(node.binding.source, out);
      collectReads(node.key, out);
      collectReads(node.value, out);
      return;
    case "unary":
      collectReads(node.value, out);
      return;
    case "binary":
      collectReads(node.left, out);
      collectReads(node.right, out);
      return;
    case "assign":
      if (!(node.op === "=" && node.place.type === "identifier"))
        collectReads(node.place, out);
      collectReads(node.value, out);
      return;
    case "navigation":
      collectReads(node.target, out);
      for (const segment of node.segments) {
        if (segment.type === "dynamic")
          collectReads(segment.key, out);
      }
      return;
    case "call":
      collectReads(node.callee, out);
      for (const arg of node.args)
        collectReads(arg, out);
      return;
    case "conditional":
      collectReads(node.condition, out);
      for (const part of node.thenBlock)
        collectReads(part, out);
      if (node.elseBranch)
        collectReadsElse(node.elseBranch, out);
      return;
    case "for":
      collectReads(node.binding.source, out);
      for (const part of node.body)
        collectReads(part, out);
      return;
    case "program":
      for (const part of node.body)
        collectReads(part, out);
      return;
    default:
      return;
  }
}
function collectReadsElse(elseBranch, out) {
  if (elseBranch.type === "else") {
    for (const part of elseBranch.block)
      collectReads(part, out);
    return;
  }
  collectReads(elseBranch.condition, out);
  for (const part of elseBranch.thenBlock)
    collectReads(part, out);
  if (elseBranch.elseBranch)
    collectReadsElse(elseBranch.elseBranch, out);
}
function isPureNode(node) {
  switch (node.type) {
    case "identifier":
    case "self":
    case "selfDepth":
    case "boolean":
    case "null":
    case "undefined":
    case "number":
    case "string":
    case "key":
      return true;
    case "group":
      return isPureNode(node.expression);
    case "array":
      return node.items.every((item) => isPureNode(item));
    case "object":
      return node.entries.every((entry) => isPureNode(entry.key) && isPureNode(entry.value));
    case "navigation":
      return isPureNode(node.target) && node.segments.every((segment) => segment.type === "static" || isPureNode(segment.key));
    case "unary":
      return node.op !== "delete" && isPureNode(node.value);
    case "binary":
      return isPureNode(node.left) && isPureNode(node.right);
    default:
      return false;
  }
}
function eliminateDeadAssignments(block) {
  const needed = new Set;
  const out = [];
  for (let index = block.length - 1;index >= 0; index -= 1) {
    const node = block[index];
    if (node.type === "conditional") {
      let rewritten = node;
      if (node.condition.type === "assign" && node.condition.op === "=" && node.condition.place.type === "identifier") {
        const name = node.condition.place.name;
        const branchReads = new Set;
        for (const part of node.thenBlock)
          collectReads(part, branchReads);
        if (node.elseBranch)
          collectReadsElse(node.elseBranch, branchReads);
        if (!needed.has(name) && !branchReads.has(name)) {
          rewritten = {
            type: "conditional",
            head: node.head,
            condition: node.condition.value,
            thenBlock: node.thenBlock,
            elseBranch: node.elseBranch
          };
        }
      }
      collectReads(rewritten, needed);
      out.push(rewritten);
      continue;
    }
    if (node.type === "assign" && node.op === "=" && node.place.type === "identifier") {
      collectReads(node.value, needed);
      const name = node.place.name;
      const canDrop = !needed.has(name) && isPureNode(node.value);
      needed.delete(name);
      if (canDrop)
        continue;
      out.push(node);
      continue;
    }
    collectReads(node, needed);
    out.push(node);
  }
  out.reverse();
  return out;
}
function hasIdentifierRead(node, name, asPlace = false) {
  if (node.type === "identifier")
    return !asPlace && node.name === name;
  switch (node.type) {
    case "group":
      return hasIdentifierRead(node.expression, name);
    case "array":
      return node.items.some((item) => hasIdentifierRead(item, name));
    case "object":
      return node.entries.some((entry) => hasIdentifierRead(entry.key, name) || hasIdentifierRead(entry.value, name));
    case "navigation":
      return hasIdentifierRead(node.target, name) || node.segments.some((segment) => segment.type === "dynamic" && hasIdentifierRead(segment.key, name));
    case "unary":
      return hasIdentifierRead(node.value, name, node.op === "delete");
    case "binary":
      return hasIdentifierRead(node.left, name) || hasIdentifierRead(node.right, name);
    case "assign":
      return hasIdentifierRead(node.place, name, true) || hasIdentifierRead(node.value, name);
    default:
      return false;
  }
}
function countIdentifierReads(node, name, asPlace = false) {
  if (node.type === "identifier")
    return !asPlace && node.name === name ? 1 : 0;
  switch (node.type) {
    case "group":
      return countIdentifierReads(node.expression, name);
    case "array":
      return node.items.reduce((sum, item) => sum + countIdentifierReads(item, name), 0);
    case "object":
      return node.entries.reduce((sum, entry) => sum + countIdentifierReads(entry.key, name) + countIdentifierReads(entry.value, name), 0);
    case "navigation":
      return countIdentifierReads(node.target, name) + node.segments.reduce((sum, segment) => sum + (segment.type === "dynamic" ? countIdentifierReads(segment.key, name) : 0), 0);
    case "unary":
      return countIdentifierReads(node.value, name, node.op === "delete");
    case "binary":
      return countIdentifierReads(node.left, name) + countIdentifierReads(node.right, name);
    case "assign":
      return countIdentifierReads(node.place, name, true) + countIdentifierReads(node.value, name);
    default:
      return 0;
  }
}
function replaceIdentifier(node, name, replacement, asPlace = false) {
  if (node.type === "identifier") {
    if (!asPlace && node.name === name)
      return cloneNode(replacement);
    return node;
  }
  switch (node.type) {
    case "group":
      return {
        type: "group",
        expression: replaceIdentifier(node.expression, name, replacement)
      };
    case "array":
      return { type: "array", items: node.items.map((item) => replaceIdentifier(item, name, replacement)) };
    case "object":
      return {
        type: "object",
        entries: node.entries.map((entry) => ({
          key: replaceIdentifier(entry.key, name, replacement),
          value: replaceIdentifier(entry.value, name, replacement)
        }))
      };
    case "navigation":
      return {
        type: "navigation",
        target: replaceIdentifier(node.target, name, replacement),
        segments: node.segments.map((segment) => segment.type === "static" ? segment : { type: "dynamic", key: replaceIdentifier(segment.key, name, replacement) })
      };
    case "unary":
      return {
        type: "unary",
        op: node.op,
        value: replaceIdentifier(node.value, name, replacement, node.op === "delete")
      };
    case "binary":
      return {
        type: "binary",
        op: node.op,
        left: replaceIdentifier(node.left, name, replacement),
        right: replaceIdentifier(node.right, name, replacement)
      };
    case "assign":
      return {
        type: "assign",
        op: node.op,
        place: replaceIdentifier(node.place, name, replacement, true),
        value: replaceIdentifier(node.value, name, replacement)
      };
    default:
      return node;
  }
}
function isSafeInlineTargetNode(node) {
  if (isPureNode(node))
    return true;
  if (node.type === "assign" && node.op === "=") {
    return isPureNode(node.place) && isPureNode(node.value);
  }
  return false;
}
function inlineAdjacentPureAssignments(block) {
  const out = [...block];
  let changed = true;
  while (changed) {
    changed = false;
    for (let index = 0;index < out.length - 1; index += 1) {
      const current = out[index];
      if (current.type !== "assign" || current.op !== "=" || current.place.type !== "identifier")
        continue;
      if (!isPureNode(current.value))
        continue;
      const name = current.place.name;
      if (hasIdentifierRead(current.value, name))
        continue;
      const next = out[index + 1];
      if (!isSafeInlineTargetNode(next))
        continue;
      if (countIdentifierReads(next, name) !== 1)
        continue;
      out[index + 1] = replaceIdentifier(next, name, current.value);
      out.splice(index, 1);
      changed = true;
      break;
    }
  }
  return out;
}
function toNumberNode(value) {
  return { type: "number", raw: String(value), value };
}
function toStringNode(value) {
  return { type: "string", raw: JSON.stringify(value) };
}
function toLiteralNode(value) {
  if (value === undefined)
    return { type: "undefined" };
  if (value === null)
    return { type: "null" };
  if (typeof value === "boolean")
    return { type: "boolean", value };
  if (typeof value === "number" && Number.isFinite(value))
    return toNumberNode(value);
  if (typeof value === "string")
    return toStringNode(value);
  if (Array.isArray(value)) {
    const items = [];
    for (const item of value) {
      const lowered = toLiteralNode(item);
      if (!lowered)
        return;
      items.push(lowered);
    }
    return { type: "array", items };
  }
  if (value && typeof value === "object") {
    const entries = [];
    for (const [key, entryValue] of Object.entries(value)) {
      const loweredValue = toLiteralNode(entryValue);
      if (!loweredValue)
        return;
      entries.push({ key: { type: "key", name: key }, value: loweredValue });
    }
    return { type: "object", entries };
  }
  return;
}
function constValue(node) {
  switch (node.type) {
    case "undefined":
      return;
    case "null":
      return null;
    case "boolean":
      return node.value;
    case "number":
      return node.value;
    case "string":
      return decodeStringLiteral(node.raw);
    case "key":
      return node.name;
    case "array": {
      const out = [];
      for (const item of node.items) {
        const value = constValue(item);
        if (value === undefined && item.type !== "undefined")
          return;
        out.push(value);
      }
      return out;
    }
    case "object": {
      const out = {};
      for (const entry of node.entries) {
        const key = constValue(entry.key);
        if (key === undefined && entry.key.type !== "undefined")
          return;
        const value = constValue(entry.value);
        if (value === undefined && entry.value.type !== "undefined")
          return;
        out[String(key)] = value;
      }
      return out;
    }
    default:
      return;
  }
}
function isDefinedValue(value) {
  return value !== undefined;
}
function foldUnary(op, value) {
  if (op === "neg") {
    if (typeof value !== "number")
      return;
    return -value;
  }
  if (op === "not") {
    if (typeof value === "boolean")
      return !value;
    if (typeof value === "number")
      return ~value;
    return;
  }
  return;
}
function foldBinary(op, left, right) {
  if (op === "add" || op === "sub" || op === "mul" || op === "div" || op === "mod") {
    if (typeof left !== "number" || typeof right !== "number")
      return;
    if (op === "add")
      return left + right;
    if (op === "sub")
      return left - right;
    if (op === "mul")
      return left * right;
    if (op === "div")
      return left / right;
    return left % right;
  }
  if (op === "bitAnd" || op === "bitOr" || op === "bitXor") {
    if (typeof left !== "number" || typeof right !== "number")
      return;
    if (op === "bitAnd")
      return left & right;
    if (op === "bitOr")
      return left | right;
    return left ^ right;
  }
  if (op === "eq")
    return left === right ? left : undefined;
  if (op === "neq")
    return left !== right ? left : undefined;
  if (op === "gt" || op === "gte" || op === "lt" || op === "lte") {
    if (typeof left !== "number" || typeof right !== "number")
      return;
    if (op === "gt")
      return left > right ? left : undefined;
    if (op === "gte")
      return left >= right ? left : undefined;
    if (op === "lt")
      return left < right ? left : undefined;
    return left <= right ? left : undefined;
  }
  if (op === "and")
    return isDefinedValue(left) ? right : undefined;
  if (op === "or")
    return isDefinedValue(left) ? left : right;
  return;
}
function optimizeElse(elseBranch, env, currentDepth) {
  if (!elseBranch)
    return;
  if (elseBranch.type === "else") {
    return { type: "else", block: optimizeBlock(elseBranch.block, cloneOptimizeEnv(env), currentDepth) };
  }
  const optimizedCondition = optimizeNode(elseBranch.condition, env, currentDepth);
  const foldedCondition = constValue(optimizedCondition);
  if (foldedCondition !== undefined || optimizedCondition.type === "undefined") {
    const passes = elseBranch.head === "when" ? isDefinedValue(foldedCondition) : !isDefinedValue(foldedCondition);
    if (passes) {
      return {
        type: "else",
        block: optimizeBlock(elseBranch.thenBlock, cloneOptimizeEnv(env), currentDepth)
      };
    }
    return optimizeElse(elseBranch.elseBranch, env, currentDepth);
  }
  return {
    type: "elseChain",
    head: elseBranch.head,
    condition: optimizedCondition,
    thenBlock: optimizeBlock(elseBranch.thenBlock, cloneOptimizeEnv(env), currentDepth),
    elseBranch: optimizeElse(elseBranch.elseBranch, cloneOptimizeEnv(env), currentDepth)
  };
}
function optimizeBlock(block, env, currentDepth) {
  const out = [];
  for (const node of block) {
    const optimized = optimizeNode(node, env, currentDepth);
    out.push(optimized);
    if (optimized.type === "break" || optimized.type === "continue")
      break;
    if (optimized.type === "assign" && optimized.op === "=" && optimized.place.type === "identifier") {
      const selfTarget = selfTargetFromNode(optimized.value, currentDepth);
      if (selfTarget !== undefined) {
        env.selfCaptures[optimized.place.name] = selfTarget;
        delete env.constants[optimized.place.name];
        continue;
      }
      const folded = constValue(optimized.value);
      if (folded !== undefined || optimized.value.type === "undefined") {
        env.constants[optimized.place.name] = cloneNode(optimized.value);
        delete env.selfCaptures[optimized.place.name];
      } else {
        clearBinding(env, optimized.place.name);
      }
      continue;
    }
    if (optimized.type === "unary" && optimized.op === "delete" && optimized.value.type === "identifier") {
      clearBinding(env, optimized.value.name);
      continue;
    }
    if (optimized.type === "assign" && optimized.place.type === "identifier") {
      clearBinding(env, optimized.place.name);
      continue;
    }
    if (optimized.type === "assign" || optimized.type === "for" || optimized.type === "call") {
      clearOptimizeEnv(env);
    }
  }
  return inlineAdjacentPureAssignments(eliminateDeadAssignments(out));
}
function optimizeNode(node, env, currentDepth, asPlace = false) {
  switch (node.type) {
    case "program": {
      const body = optimizeBlock(node.body, cloneOptimizeEnv(env), currentDepth);
      if (body.length === 0)
        return { type: "undefined" };
      if (body.length === 1)
        return body[0];
      return { type: "program", body };
    }
    case "identifier": {
      if (asPlace)
        return node;
      const selfTarget = env.selfCaptures[node.name];
      if (selfTarget !== undefined) {
        const rewritten = selfNodeFromTarget(selfTarget, currentDepth);
        if (rewritten)
          return rewritten;
      }
      const replacement = env.constants[node.name];
      return replacement ? cloneNode(replacement) : node;
    }
    case "group": {
      return optimizeNode(node.expression, env, currentDepth);
    }
    case "array": {
      return { type: "array", items: node.items.map((item) => optimizeNode(item, env, currentDepth)) };
    }
    case "object": {
      return {
        type: "object",
        entries: node.entries.map((entry) => ({
          key: optimizeNode(entry.key, env, currentDepth),
          value: optimizeNode(entry.value, env, currentDepth)
        }))
      };
    }
    case "unary": {
      const value = optimizeNode(node.value, env, currentDepth, node.op === "delete");
      const foldedValue = constValue(value);
      if (foldedValue !== undefined || value.type === "undefined") {
        const folded = foldUnary(node.op, foldedValue);
        const literal = folded === undefined ? undefined : toLiteralNode(folded);
        if (literal)
          return literal;
      }
      return { type: "unary", op: node.op, value };
    }
    case "binary": {
      const left = optimizeNode(node.left, env, currentDepth);
      const right = optimizeNode(node.right, env, currentDepth);
      const leftValue = constValue(left);
      const rightValue = constValue(right);
      if ((leftValue !== undefined || left.type === "undefined") && (rightValue !== undefined || right.type === "undefined")) {
        const folded = foldBinary(node.op, leftValue, rightValue);
        const literal = folded === undefined ? undefined : toLiteralNode(folded);
        if (literal)
          return literal;
      }
      return { type: "binary", op: node.op, left, right };
    }
    case "navigation": {
      const target = optimizeNode(node.target, env, currentDepth);
      const segments = node.segments.map((segment) => segment.type === "static" ? segment : { type: "dynamic", key: optimizeNode(segment.key, env, currentDepth) });
      const targetValue = constValue(target);
      if (targetValue !== undefined || target.type === "undefined") {
        let current = targetValue;
        let foldable = true;
        for (const segment of segments) {
          if (!foldable)
            break;
          const key = segment.type === "static" ? segment.key : constValue(segment.key);
          if (segment.type === "dynamic" && key === undefined && segment.key.type !== "undefined") {
            foldable = false;
            break;
          }
          if (current === null || current === undefined) {
            current = undefined;
            continue;
          }
          current = current[String(key)];
        }
        if (foldable) {
          const literal = toLiteralNode(current);
          if (literal)
            return literal;
        }
      }
      return {
        type: "navigation",
        target,
        segments
      };
    }
    case "call": {
      return {
        type: "call",
        callee: optimizeNode(node.callee, env, currentDepth),
        args: node.args.map((arg) => optimizeNode(arg, env, currentDepth))
      };
    }
    case "assign": {
      return {
        type: "assign",
        op: node.op,
        place: optimizeNode(node.place, env, currentDepth, true),
        value: optimizeNode(node.value, env, currentDepth)
      };
    }
    case "conditional": {
      const condition = optimizeNode(node.condition, env, currentDepth);
      const thenEnv = cloneOptimizeEnv(env);
      if (condition.type === "assign" && condition.op === "=" && condition.place.type === "identifier") {
        thenEnv.selfCaptures[condition.place.name] = currentDepth;
        delete thenEnv.constants[condition.place.name];
      }
      const conditionValue = constValue(condition);
      if (conditionValue !== undefined || condition.type === "undefined") {
        const passes = node.head === "when" ? isDefinedValue(conditionValue) : !isDefinedValue(conditionValue);
        if (passes) {
          const thenBlock2 = optimizeBlock(node.thenBlock, thenEnv, currentDepth);
          if (thenBlock2.length === 0)
            return { type: "undefined" };
          if (thenBlock2.length === 1)
            return thenBlock2[0];
          return { type: "program", body: thenBlock2 };
        }
        if (!node.elseBranch)
          return { type: "undefined" };
        const loweredElse = optimizeElse(node.elseBranch, cloneOptimizeEnv(env), currentDepth);
        if (!loweredElse)
          return { type: "undefined" };
        if (loweredElse.type === "else") {
          if (loweredElse.block.length === 0)
            return { type: "undefined" };
          if (loweredElse.block.length === 1)
            return loweredElse.block[0];
          return { type: "program", body: loweredElse.block };
        }
        return {
          type: "conditional",
          head: loweredElse.head,
          condition: loweredElse.condition,
          thenBlock: loweredElse.thenBlock,
          elseBranch: loweredElse.elseBranch
        };
      }
      const thenBlock = optimizeBlock(node.thenBlock, thenEnv, currentDepth);
      const elseBranch = optimizeElse(node.elseBranch, cloneOptimizeEnv(env), currentDepth);
      let finalCondition = condition;
      if (condition.type === "assign" && condition.op === "=" && condition.place.type === "identifier") {
        const name = condition.place.name;
        const reads = new Set;
        for (const part of thenBlock)
          collectReads(part, reads);
        if (elseBranch)
          collectReadsElse(elseBranch, reads);
        if (!reads.has(name)) {
          finalCondition = condition.value;
        }
      }
      return {
        type: "conditional",
        head: node.head,
        condition: finalCondition,
        thenBlock,
        elseBranch
      };
    }
    case "for": {
      const sourceEnv = cloneOptimizeEnv(env);
      const binding = (() => {
        if (node.binding.type === "binding:expr") {
          return { type: "binding:expr", source: optimizeNode(node.binding.source, sourceEnv, currentDepth) };
        }
        if (node.binding.type === "binding:valueIn") {
          return {
            type: "binding:valueIn",
            value: node.binding.value,
            source: optimizeNode(node.binding.source, sourceEnv, currentDepth)
          };
        }
        if (node.binding.type === "binding:keyValueIn") {
          return {
            type: "binding:keyValueIn",
            key: node.binding.key,
            value: node.binding.value,
            source: optimizeNode(node.binding.source, sourceEnv, currentDepth)
          };
        }
        return {
          type: "binding:keyOf",
          key: node.binding.key,
          source: optimizeNode(node.binding.source, sourceEnv, currentDepth)
        };
      })();
      const bodyEnv = cloneOptimizeEnv(env);
      dropBindingNames(bodyEnv, binding);
      return {
        type: "for",
        binding,
        body: optimizeBlock(node.body, bodyEnv, currentDepth + 1)
      };
    }
    case "arrayComprehension": {
      const sourceEnv = cloneOptimizeEnv(env);
      const binding = node.binding.type === "binding:expr" ? { type: "binding:expr", source: optimizeNode(node.binding.source, sourceEnv, currentDepth) } : node.binding.type === "binding:valueIn" ? {
        type: "binding:valueIn",
        value: node.binding.value,
        source: optimizeNode(node.binding.source, sourceEnv, currentDepth)
      } : node.binding.type === "binding:keyValueIn" ? {
        type: "binding:keyValueIn",
        key: node.binding.key,
        value: node.binding.value,
        source: optimizeNode(node.binding.source, sourceEnv, currentDepth)
      } : {
        type: "binding:keyOf",
        key: node.binding.key,
        source: optimizeNode(node.binding.source, sourceEnv, currentDepth)
      };
      const bodyEnv = cloneOptimizeEnv(env);
      dropBindingNames(bodyEnv, binding);
      return {
        type: "arrayComprehension",
        binding,
        body: optimizeNode(node.body, bodyEnv, currentDepth + 1)
      };
    }
    case "objectComprehension": {
      const sourceEnv = cloneOptimizeEnv(env);
      const binding = node.binding.type === "binding:expr" ? { type: "binding:expr", source: optimizeNode(node.binding.source, sourceEnv, currentDepth) } : node.binding.type === "binding:valueIn" ? {
        type: "binding:valueIn",
        value: node.binding.value,
        source: optimizeNode(node.binding.source, sourceEnv, currentDepth)
      } : node.binding.type === "binding:keyValueIn" ? {
        type: "binding:keyValueIn",
        key: node.binding.key,
        value: node.binding.value,
        source: optimizeNode(node.binding.source, sourceEnv, currentDepth)
      } : {
        type: "binding:keyOf",
        key: node.binding.key,
        source: optimizeNode(node.binding.source, sourceEnv, currentDepth)
      };
      const bodyEnv = cloneOptimizeEnv(env);
      dropBindingNames(bodyEnv, binding);
      return {
        type: "objectComprehension",
        binding,
        key: optimizeNode(node.key, bodyEnv, currentDepth + 1),
        value: optimizeNode(node.value, bodyEnv, currentDepth + 1)
      };
    }
    default:
      return node;
  }
}
function optimizeIR(node) {
  return optimizeNode(node, emptyOptimizeEnv(), 1);
}
function collectLocalBindings(node, locals) {
  switch (node.type) {
    case "assign":
      if (node.place.type === "identifier")
        locals.add(node.place.name);
      collectLocalBindings(node.place, locals);
      collectLocalBindings(node.value, locals);
      return;
    case "program":
      for (const part of node.body)
        collectLocalBindings(part, locals);
      return;
    case "group":
      collectLocalBindings(node.expression, locals);
      return;
    case "array":
      for (const item of node.items)
        collectLocalBindings(item, locals);
      return;
    case "object":
      for (const entry of node.entries) {
        collectLocalBindings(entry.key, locals);
        collectLocalBindings(entry.value, locals);
      }
      return;
    case "navigation":
      collectLocalBindings(node.target, locals);
      for (const segment of node.segments) {
        if (segment.type === "dynamic")
          collectLocalBindings(segment.key, locals);
      }
      return;
    case "call":
      collectLocalBindings(node.callee, locals);
      for (const arg of node.args)
        collectLocalBindings(arg, locals);
      return;
    case "unary":
      collectLocalBindings(node.value, locals);
      return;
    case "binary":
      collectLocalBindings(node.left, locals);
      collectLocalBindings(node.right, locals);
      return;
    case "conditional":
      collectLocalBindings(node.condition, locals);
      for (const part of node.thenBlock)
        collectLocalBindings(part, locals);
      if (node.elseBranch)
        collectLocalBindingsElse(node.elseBranch, locals);
      return;
    case "for":
      collectLocalBindingFromBinding(node.binding, locals);
      for (const part of node.body)
        collectLocalBindings(part, locals);
      return;
    case "arrayComprehension":
      collectLocalBindingFromBinding(node.binding, locals);
      collectLocalBindings(node.body, locals);
      return;
    case "objectComprehension":
      collectLocalBindingFromBinding(node.binding, locals);
      collectLocalBindings(node.key, locals);
      collectLocalBindings(node.value, locals);
      return;
    default:
      return;
  }
}
function collectLocalBindingFromBinding(binding, locals) {
  if (binding.type === "binding:valueIn") {
    locals.add(binding.value);
    collectLocalBindings(binding.source, locals);
    return;
  }
  if (binding.type === "binding:keyValueIn") {
    locals.add(binding.key);
    locals.add(binding.value);
    collectLocalBindings(binding.source, locals);
    return;
  }
  if (binding.type === "binding:keyOf") {
    locals.add(binding.key);
    collectLocalBindings(binding.source, locals);
    return;
  }
  collectLocalBindings(binding.source, locals);
}
function collectLocalBindingsElse(elseBranch, locals) {
  if (elseBranch.type === "else") {
    for (const part of elseBranch.block)
      collectLocalBindings(part, locals);
    return;
  }
  collectLocalBindings(elseBranch.condition, locals);
  for (const part of elseBranch.thenBlock)
    collectLocalBindings(part, locals);
  if (elseBranch.elseBranch)
    collectLocalBindingsElse(elseBranch.elseBranch, locals);
}
function bumpNameFrequency(name, locals, frequencies, order, nextOrder) {
  if (!locals.has(name))
    return;
  if (!order.has(name)) {
    order.set(name, nextOrder.value);
    nextOrder.value += 1;
  }
  frequencies.set(name, (frequencies.get(name) ?? 0) + 1);
}
function collectNameFrequencies(node, locals, frequencies, order, nextOrder) {
  switch (node.type) {
    case "identifier":
      bumpNameFrequency(node.name, locals, frequencies, order, nextOrder);
      return;
    case "assign":
      if (node.place.type === "identifier")
        bumpNameFrequency(node.place.name, locals, frequencies, order, nextOrder);
      collectNameFrequencies(node.place, locals, frequencies, order, nextOrder);
      collectNameFrequencies(node.value, locals, frequencies, order, nextOrder);
      return;
    case "program":
      for (const part of node.body)
        collectNameFrequencies(part, locals, frequencies, order, nextOrder);
      return;
    case "group":
      collectNameFrequencies(node.expression, locals, frequencies, order, nextOrder);
      return;
    case "array":
      for (const item of node.items)
        collectNameFrequencies(item, locals, frequencies, order, nextOrder);
      return;
    case "object":
      for (const entry of node.entries) {
        collectNameFrequencies(entry.key, locals, frequencies, order, nextOrder);
        collectNameFrequencies(entry.value, locals, frequencies, order, nextOrder);
      }
      return;
    case "navigation":
      collectNameFrequencies(node.target, locals, frequencies, order, nextOrder);
      for (const segment of node.segments) {
        if (segment.type === "dynamic")
          collectNameFrequencies(segment.key, locals, frequencies, order, nextOrder);
      }
      return;
    case "call":
      collectNameFrequencies(node.callee, locals, frequencies, order, nextOrder);
      for (const arg of node.args)
        collectNameFrequencies(arg, locals, frequencies, order, nextOrder);
      return;
    case "unary":
      collectNameFrequencies(node.value, locals, frequencies, order, nextOrder);
      return;
    case "binary":
      collectNameFrequencies(node.left, locals, frequencies, order, nextOrder);
      collectNameFrequencies(node.right, locals, frequencies, order, nextOrder);
      return;
    case "conditional":
      collectNameFrequencies(node.condition, locals, frequencies, order, nextOrder);
      for (const part of node.thenBlock)
        collectNameFrequencies(part, locals, frequencies, order, nextOrder);
      if (node.elseBranch)
        collectNameFrequenciesElse(node.elseBranch, locals, frequencies, order, nextOrder);
      return;
    case "for":
      collectNameFrequenciesBinding(node.binding, locals, frequencies, order, nextOrder);
      for (const part of node.body)
        collectNameFrequencies(part, locals, frequencies, order, nextOrder);
      return;
    case "arrayComprehension":
      collectNameFrequenciesBinding(node.binding, locals, frequencies, order, nextOrder);
      collectNameFrequencies(node.body, locals, frequencies, order, nextOrder);
      return;
    case "objectComprehension":
      collectNameFrequenciesBinding(node.binding, locals, frequencies, order, nextOrder);
      collectNameFrequencies(node.key, locals, frequencies, order, nextOrder);
      collectNameFrequencies(node.value, locals, frequencies, order, nextOrder);
      return;
    default:
      return;
  }
}
function collectNameFrequenciesBinding(binding, locals, frequencies, order, nextOrder) {
  if (binding.type === "binding:valueIn") {
    bumpNameFrequency(binding.value, locals, frequencies, order, nextOrder);
    collectNameFrequencies(binding.source, locals, frequencies, order, nextOrder);
    return;
  }
  if (binding.type === "binding:keyValueIn") {
    bumpNameFrequency(binding.key, locals, frequencies, order, nextOrder);
    bumpNameFrequency(binding.value, locals, frequencies, order, nextOrder);
    collectNameFrequencies(binding.source, locals, frequencies, order, nextOrder);
    return;
  }
  if (binding.type === "binding:keyOf") {
    bumpNameFrequency(binding.key, locals, frequencies, order, nextOrder);
    collectNameFrequencies(binding.source, locals, frequencies, order, nextOrder);
    return;
  }
  collectNameFrequencies(binding.source, locals, frequencies, order, nextOrder);
}
function collectNameFrequenciesElse(elseBranch, locals, frequencies, order, nextOrder) {
  if (elseBranch.type === "else") {
    for (const part of elseBranch.block)
      collectNameFrequencies(part, locals, frequencies, order, nextOrder);
    return;
  }
  collectNameFrequencies(elseBranch.condition, locals, frequencies, order, nextOrder);
  for (const part of elseBranch.thenBlock)
    collectNameFrequencies(part, locals, frequencies, order, nextOrder);
  if (elseBranch.elseBranch)
    collectNameFrequenciesElse(elseBranch.elseBranch, locals, frequencies, order, nextOrder);
}
function renameLocalNames(node, map) {
  switch (node.type) {
    case "identifier":
      return map.has(node.name) ? { type: "identifier", name: map.get(node.name) } : node;
    case "program":
      return { type: "program", body: node.body.map((part) => renameLocalNames(part, map)) };
    case "group":
      return { type: "group", expression: renameLocalNames(node.expression, map) };
    case "array":
      return { type: "array", items: node.items.map((item) => renameLocalNames(item, map)) };
    case "object":
      return {
        type: "object",
        entries: node.entries.map((entry) => ({
          key: renameLocalNames(entry.key, map),
          value: renameLocalNames(entry.value, map)
        }))
      };
    case "navigation":
      return {
        type: "navigation",
        target: renameLocalNames(node.target, map),
        segments: node.segments.map((segment) => segment.type === "static" ? segment : { type: "dynamic", key: renameLocalNames(segment.key, map) })
      };
    case "call":
      return {
        type: "call",
        callee: renameLocalNames(node.callee, map),
        args: node.args.map((arg) => renameLocalNames(arg, map))
      };
    case "unary":
      return { type: "unary", op: node.op, value: renameLocalNames(node.value, map) };
    case "binary":
      return {
        type: "binary",
        op: node.op,
        left: renameLocalNames(node.left, map),
        right: renameLocalNames(node.right, map)
      };
    case "assign": {
      const place = node.place.type === "identifier" && map.has(node.place.name) ? { type: "identifier", name: map.get(node.place.name) } : renameLocalNames(node.place, map);
      return {
        type: "assign",
        op: node.op,
        place,
        value: renameLocalNames(node.value, map)
      };
    }
    case "conditional":
      return {
        type: "conditional",
        head: node.head,
        condition: renameLocalNames(node.condition, map),
        thenBlock: node.thenBlock.map((part) => renameLocalNames(part, map)),
        elseBranch: node.elseBranch ? renameLocalNamesElse(node.elseBranch, map) : undefined
      };
    case "for":
      return {
        type: "for",
        binding: renameLocalNamesBinding(node.binding, map),
        body: node.body.map((part) => renameLocalNames(part, map))
      };
    case "arrayComprehension":
      return {
        type: "arrayComprehension",
        binding: renameLocalNamesBinding(node.binding, map),
        body: renameLocalNames(node.body, map)
      };
    case "objectComprehension":
      return {
        type: "objectComprehension",
        binding: renameLocalNamesBinding(node.binding, map),
        key: renameLocalNames(node.key, map),
        value: renameLocalNames(node.value, map)
      };
    default:
      return node;
  }
}
function renameLocalNamesBinding(binding, map) {
  if (binding.type === "binding:expr") {
    return { type: "binding:expr", source: renameLocalNames(binding.source, map) };
  }
  if (binding.type === "binding:valueIn") {
    return {
      type: "binding:valueIn",
      value: map.get(binding.value) ?? binding.value,
      source: renameLocalNames(binding.source, map)
    };
  }
  if (binding.type === "binding:keyValueIn") {
    return {
      type: "binding:keyValueIn",
      key: map.get(binding.key) ?? binding.key,
      value: map.get(binding.value) ?? binding.value,
      source: renameLocalNames(binding.source, map)
    };
  }
  return {
    type: "binding:keyOf",
    key: map.get(binding.key) ?? binding.key,
    source: renameLocalNames(binding.source, map)
  };
}
function renameLocalNamesElse(elseBranch, map) {
  if (elseBranch.type === "else") {
    return {
      type: "else",
      block: elseBranch.block.map((part) => renameLocalNames(part, map))
    };
  }
  return {
    type: "elseChain",
    head: elseBranch.head,
    condition: renameLocalNames(elseBranch.condition, map),
    thenBlock: elseBranch.thenBlock.map((part) => renameLocalNames(part, map)),
    elseBranch: elseBranch.elseBranch ? renameLocalNamesElse(elseBranch.elseBranch, map) : undefined
  };
}
function minifyLocalNamesIR(node) {
  const locals = new Set;
  collectLocalBindings(node, locals);
  if (locals.size === 0)
    return node;
  const frequencies = new Map;
  const order = new Map;
  collectNameFrequencies(node, locals, frequencies, order, { value: 0 });
  const ranked = Array.from(locals).sort((a, b) => {
    const freqA = frequencies.get(a) ?? 0;
    const freqB = frequencies.get(b) ?? 0;
    if (freqA !== freqB)
      return freqB - freqA;
    const orderA = order.get(a) ?? Number.MAX_SAFE_INTEGER;
    const orderB = order.get(b) ?? Number.MAX_SAFE_INTEGER;
    if (orderA !== orderB)
      return orderA - orderB;
    return a.localeCompare(b);
  });
  const renameMap = new Map;
  ranked.forEach((name, index) => {
    renameMap.set(name, encodeUint(index));
  });
  return renameLocalNames(node, renameMap);
}
function compile(source, options) {
  const ir = parseToIR(source);
  let lowered = options?.optimize ? optimizeIR(ir) : ir;
  if (options?.minifyNames)
    lowered = minifyLocalNamesIR(lowered);
  const domainRefs = options?.domainConfig ? domainRefsFromConfig(options.domainConfig) : undefined;
  return encodeIR(lowered, {
    domainRefs,
    dedupeValues: options?.dedupeValues,
    dedupeMinBytes: options?.dedupeMinBytes
  });
}
function parseNumber(raw) {
  if (/^-?0x/i.test(raw))
    return parseInt(raw, 16);
  if (/^-?0b/i.test(raw)) {
    const isNegative = raw.startsWith("-");
    const digits = raw.replace(/^-?0b/i, "");
    const value = parseInt(digits, 2);
    return isNegative ? -value : value;
  }
  return Number(raw);
}
function collectStructured(value, out) {
  if (Array.isArray(value)) {
    for (const part of value)
      collectStructured(part, out);
    return;
  }
  if (!value || typeof value !== "object")
    return;
  if ("type" in value || "key" in value && "value" in value) {
    out.push(value);
  }
}
function normalizeList(value) {
  const out = [];
  collectStructured(value, out);
  return out;
}
function collectPostfixSteps(value, out) {
  if (Array.isArray(value)) {
    for (const part of value)
      collectPostfixSteps(part, out);
    return;
  }
  if (!value || typeof value !== "object")
    return;
  if ("kind" in value)
    out.push(value);
}
function normalizePostfixSteps(value) {
  const out = [];
  collectPostfixSteps(value, out);
  return out;
}
function buildPostfix(base, steps) {
  let current = base;
  let pendingSegments = [];
  const flushSegments = () => {
    if (pendingSegments.length === 0)
      return;
    current = {
      type: "navigation",
      target: current,
      segments: pendingSegments
    };
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
    current = { type: "call", callee: current, args: step.args };
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
    if (children.length === 1 && children[0])
      return children[0].toIR();
    return children.map((child) => child.toIR());
  },
  Program(expressions) {
    const body = normalizeList(expressions.toIR());
    if (body.length === 1)
      return body[0];
    return { type: "program", body };
  },
  Block(expressions) {
    return normalizeList(expressions.toIR());
  },
  Elements(first, separatorsAndItems, maybeTrailingComma, maybeEmpty) {
    return normalizeList([
      first.toIR(),
      separatorsAndItems.toIR(),
      maybeTrailingComma.toIR(),
      maybeEmpty.toIR()
    ]);
  },
  AssignExpr_assign(place, op, value) {
    return {
      type: "assign",
      op: op.sourceString,
      place: place.toIR(),
      value: value.toIR()
    };
  },
  ExistenceExpr_and(left, _and, right) {
    return { type: "binary", op: "and", left: left.toIR(), right: right.toIR() };
  },
  ExistenceExpr_or(left, _or, right) {
    return { type: "binary", op: "or", left: left.toIR(), right: right.toIR() };
  },
  BitExpr_and(left, _op, right) {
    return { type: "binary", op: "bitAnd", left: left.toIR(), right: right.toIR() };
  },
  BitExpr_xor(left, _op, right) {
    return { type: "binary", op: "bitXor", left: left.toIR(), right: right.toIR() };
  },
  BitExpr_or(left, _op, right) {
    return { type: "binary", op: "bitOr", left: left.toIR(), right: right.toIR() };
  },
  CompareExpr_binary(left, op, right) {
    const map = {
      "==": "eq",
      "!=": "neq",
      ">": "gt",
      ">=": "gte",
      "<": "lt",
      "<=": "lte"
    };
    const mapped = map[op.sourceString];
    if (!mapped)
      throw new Error(`Unsupported compare op: ${op.sourceString}`);
    return { type: "binary", op: mapped, left: left.toIR(), right: right.toIR() };
  },
  AddExpr_add(left, _op, right) {
    return { type: "binary", op: "add", left: left.toIR(), right: right.toIR() };
  },
  AddExpr_sub(left, _op, right) {
    return { type: "binary", op: "sub", left: left.toIR(), right: right.toIR() };
  },
  MulExpr_mul(left, _op, right) {
    return { type: "binary", op: "mul", left: left.toIR(), right: right.toIR() };
  },
  MulExpr_div(left, _op, right) {
    return { type: "binary", op: "div", left: left.toIR(), right: right.toIR() };
  },
  MulExpr_mod(left, _op, right) {
    return { type: "binary", op: "mod", left: left.toIR(), right: right.toIR() };
  },
  UnaryExpr_neg(_op, value) {
    const lowered = value.toIR();
    if (lowered.type === "number") {
      const raw = lowered.raw.startsWith("-") ? lowered.raw.slice(1) : `-${lowered.raw}`;
      return { type: "number", raw, value: -lowered.value };
    }
    return { type: "unary", op: "neg", value: lowered };
  },
  UnaryExpr_not(_op, value) {
    return { type: "unary", op: "not", value: value.toIR() };
  },
  UnaryExpr_delete(_del, place) {
    return { type: "unary", op: "delete", value: place.toIR() };
  },
  PostfixExpr_chain(base, tails) {
    return buildPostfix(base.toIR(), normalizePostfixSteps(tails.toIR()));
  },
  Place(base, tails) {
    return buildPostfix(base.toIR(), normalizePostfixSteps(tails.toIR()));
  },
  PlaceTail_navStatic(_dot, key) {
    return { kind: "navStatic", key: key.sourceString };
  },
  PlaceTail_navDynamic(_dotOpen, key, _close) {
    return { kind: "navDynamic", key: key.toIR() };
  },
  PostfixTail_navStatic(_dot, key) {
    return { kind: "navStatic", key: key.sourceString };
  },
  PostfixTail_navDynamic(_dotOpen, key, _close) {
    return { kind: "navDynamic", key: key.toIR() };
  },
  PostfixTail_callEmpty(_open, _close) {
    return { kind: "call", args: [] };
  },
  PostfixTail_call(_open, args, _close) {
    return { kind: "call", args: normalizeList(args.toIR()) };
  },
  ConditionalExpr(head, condition, _do, thenBlock, elseBranch, _end) {
    const nextElse = elseBranch.children[0];
    return {
      type: "conditional",
      head: head.toIR(),
      condition: condition.toIR(),
      thenBlock: thenBlock.toIR(),
      elseBranch: nextElse ? nextElse.toIR() : undefined
    };
  },
  ConditionalHead(_kw) {
    return this.sourceString;
  },
  ConditionalElse_elseChain(_else, head, condition, _do, thenBlock, elseBranch) {
    const nextElse = elseBranch.children[0];
    return {
      type: "elseChain",
      head: head.toIR(),
      condition: condition.toIR(),
      thenBlock: thenBlock.toIR(),
      elseBranch: nextElse ? nextElse.toIR() : undefined
    };
  },
  ConditionalElse_else(_else, block) {
    return { type: "else", block: block.toIR() };
  },
  DoExpr(_do, block, _end) {
    const body = block.toIR();
    if (body.length === 0)
      return { type: "undefined" };
    if (body.length === 1)
      return body[0];
    return { type: "program", body };
  },
  WhileExpr(_while, condition, _do, block, _end) {
    return {
      type: "while",
      condition: condition.toIR(),
      body: block.toIR()
    };
  },
  ForExpr(_for, binding, _do, block, _end) {
    return {
      type: "for",
      binding: binding.toIR(),
      body: block.toIR()
    };
  },
  BindingExpr(iterOrExpr) {
    const node = iterOrExpr.toIR();
    if (typeof node === "object" && node && "type" in node && String(node.type).startsWith("binding:")) {
      return node;
    }
    return { type: "binding:expr", source: node };
  },
  Array_empty(_open, _close) {
    return { type: "array", items: [] };
  },
  Array_comprehension(_open, binding, _semi, body, _close) {
    return {
      type: "arrayComprehension",
      binding: binding.toIR(),
      body: body.toIR()
    };
  },
  Array_values(_open, items, _close) {
    return { type: "array", items: normalizeList(items.toIR()) };
  },
  Object_empty(_open, _close) {
    return { type: "object", entries: [] };
  },
  Object_comprehension(_open, binding, _semi, key, _colon, value, _close) {
    return {
      type: "objectComprehension",
      binding: binding.toIR(),
      key: key.toIR(),
      value: value.toIR()
    };
  },
  Object_pairs(_open, pairs, _close) {
    return {
      type: "object",
      entries: normalizeList(pairs.toIR())
    };
  },
  IterBinding_keyValueIn(key, _comma, value, _in, source) {
    return {
      type: "binding:keyValueIn",
      key: key.sourceString,
      value: value.sourceString,
      source: source.toIR()
    };
  },
  IterBinding_valueIn(value, _in, source) {
    return {
      type: "binding:valueIn",
      value: value.sourceString,
      source: source.toIR()
    };
  },
  IterBinding_keyOf(key, _of, source) {
    return {
      type: "binding:keyOf",
      key: key.sourceString,
      source: source.toIR()
    };
  },
  Pair(key, _colon, value) {
    return { key: key.toIR(), value: value.toIR() };
  },
  ObjKey_bare(key) {
    return { type: "key", name: key.sourceString };
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
    return { type: "break" };
  },
  ContinueKw(_kw) {
    return { type: "continue" };
  },
  SelfExpr_depth(_self, _at, depth) {
    const value = depth.toIR();
    if (value.type !== "number" || !Number.isInteger(value.value) || value.value < 1) {
      throw new Error("self depth must be a positive integer literal");
    }
    if (value.value === 1)
      return { type: "self" };
    return { type: "selfDepth", depth: value.value };
  },
  SelfExpr_plain(selfKw) {
    return selfKw.toIR();
  },
  SelfKw(_kw) {
    return { type: "self" };
  },
  TrueKw(_kw) {
    return { type: "boolean", value: true };
  },
  FalseKw(_kw) {
    return { type: "boolean", value: false };
  },
  NullKw(_kw) {
    return { type: "null" };
  },
  UndefinedKw(_kw) {
    return { type: "undefined" };
  },
  StringKw(_kw) {
    return { type: "identifier", name: "string" };
  },
  NumberKw(_kw) {
    return { type: "identifier", name: "number" };
  },
  ObjectKw(_kw) {
    return { type: "identifier", name: "object" };
  },
  ArrayKw(_kw) {
    return { type: "identifier", name: "array" };
  },
  BooleanKw(_kw) {
    return { type: "identifier", name: "boolean" };
  },
  identifier(_a, _b) {
    return { type: "identifier", name: this.sourceString };
  },
  String(_value) {
    return { type: "string", raw: this.sourceString };
  },
  Number(_value) {
    return { type: "number", raw: this.sourceString, value: parseNumber(this.sourceString) };
  },
  PrimaryExpr_group(_open, expr, _close) {
    return { type: "group", expression: expr.toIR() };
  }
});

// rexc-interpreter.ts
var DIGITS2 = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";
var digitMap = new Map(Array.from(DIGITS2).map((char, index) => [char, index]));
var OPCODES = {
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
  neg: 21
};
function decodePrefix(text, start, end) {
  let value = 0;
  for (let index = start;index < end; index += 1) {
    const digit = digitMap.get(text[index] ?? "") ?? -1;
    if (digit < 0)
      throw new Error(`Invalid digit '${text[index]}'`);
    value = value * 64 + digit;
  }
  return value;
}
function decodeZigzag(value) {
  return value % 2 === 0 ? value / 2 : -(value + 1) / 2;
}
function isDefined(value) {
  return value !== undefined;
}

class CursorInterpreter {
  text;
  pos = 0;
  state;
  selfStack;
  opcodeMarkers;
  pointerCache = new Map;
  gasLimit;
  gas = 0;
  tick() {
    if (this.gasLimit && ++this.gas > this.gasLimit) {
      throw new Error("Gas limit exceeded (too many loop iterations)");
    }
  }
  constructor(text, ctx = {}) {
    const initialSelf = ctx.selfStack && ctx.selfStack.length > 0 ? ctx.selfStack[ctx.selfStack.length - 1] : ctx.self;
    this.text = text;
    this.state = {
      vars: ctx.vars ?? {},
      refs: {
        0: ctx.refs?.[0],
        1: ctx.refs?.[1] ?? true,
        2: ctx.refs?.[2] ?? false,
        3: ctx.refs?.[3] ?? null,
        4: ctx.refs?.[4] ?? undefined
      }
    };
    this.selfStack = ctx.selfStack && ctx.selfStack.length > 0 ? [...ctx.selfStack] : [initialSelf];
    this.gasLimit = ctx.gasLimit ?? 0;
    this.opcodeMarkers = Array.from({ length: 256 }, (_, id) => ({ __opcode: id }));
    for (const [idText, value] of Object.entries(ctx.refs ?? {})) {
      const id = Number(idText);
      if (Number.isInteger(id))
        this.state.refs[id] = value;
    }
    if (ctx.opcodes) {
      for (const [idText, op] of Object.entries(ctx.opcodes)) {
        if (op)
          this.customOpcodes.set(Number(idText), op);
      }
    }
  }
  customOpcodes = new Map;
  readSelf(depthPrefix) {
    const depth = depthPrefix + 1;
    const index = this.selfStack.length - depth;
    if (index < 0)
      return;
    return this.selfStack[index];
  }
  get runtimeState() {
    return this.state;
  }
  evaluateTopLevel() {
    this.skipNonCode();
    if (this.pos >= this.text.length)
      return;
    const value = this.evalValue();
    this.skipNonCode();
    if (this.pos < this.text.length)
      throw new Error(`Unexpected trailing input at ${this.pos}`);
    return value;
  }
  skipNonCode() {
    while (this.pos < this.text.length) {
      const ch = this.text[this.pos];
      if (ch === " " || ch === "\t" || ch === `
` || ch === "\r") {
        this.pos += 1;
        continue;
      }
      if (ch === "/" && this.text[this.pos + 1] === "/") {
        this.pos += 2;
        while (this.pos < this.text.length && this.text[this.pos] !== `
`)
          this.pos += 1;
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
  readPrefix() {
    const start = this.pos;
    while (this.pos < this.text.length && digitMap.has(this.text[this.pos] ?? ""))
      this.pos += 1;
    const end = this.pos;
    return { start, end, value: decodePrefix(this.text, start, end), raw: this.text.slice(start, end) };
  }
  ensure(char) {
    if (this.text[this.pos] !== char)
      throw new Error(`Expected '${char}' at ${this.pos}`);
    this.pos += 1;
  }
  hasMoreBefore(close) {
    const save = this.pos;
    this.skipNonCode();
    const more = this.pos < this.text.length && this.text[this.pos] !== close;
    this.pos = save;
    return more;
  }
  readBindingVarIfPresent() {
    const save = this.pos;
    this.skipNonCode();
    const prefix = this.readPrefix();
    const tag = this.text[this.pos];
    if (tag === "$" && prefix.raw.length > 0) {
      this.pos += 1;
      return prefix.raw;
    }
    this.pos = save;
    return;
  }
  evalValue() {
    this.skipNonCode();
    const prefix = this.readPrefix();
    const tag = this.text[this.pos];
    if (!tag)
      throw new Error("Unexpected end of input");
    switch (tag) {
      case "+":
        this.pos += 1;
        return decodeZigzag(prefix.value);
      case "*": {
        this.pos += 1;
        const power = decodeZigzag(prefix.value);
        const significand = this.evalValue();
        if (typeof significand !== "number")
          throw new Error("Decimal significand must be numeric");
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
        if (end > this.text.length)
          throw new Error("String container overflows input");
        const value = this.text.slice(start, end);
        this.pos = end;
        return value;
      }
      case "^": {
        this.pos += 1;
        const target = this.pos + prefix.value;
        if (this.pointerCache.has(target))
          return this.pointerCache.get(target);
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
        return;
      }
      case ";": {
        this.pos += 1;
        const kind = prefix.value % 2 === 0 ? "break" : "continue";
        const depth = Math.floor(prefix.value / 2) + 1;
        return { kind, depth };
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
      case "#":
        return this.evalWhileLoop();
      default:
        throw new Error(`Unexpected tag '${tag}' at ${this.pos}`);
    }
  }
  evalCall(_prefix) {
    this.ensure("(");
    this.skipNonCode();
    if (this.text[this.pos] === ")") {
      this.pos += 1;
      return;
    }
    const callee = this.evalValue();
    const args = [];
    while (true) {
      this.skipNonCode();
      if (this.text[this.pos] === ")")
        break;
      args.push(this.evalValue());
    }
    this.ensure(")");
    if (typeof callee === "object" && callee && "__opcode" in callee) {
      return this.applyOpcode(callee.__opcode, args);
    }
    return this.navigate(callee, args);
  }
  evalArray(_prefix) {
    this.ensure("[");
    this.skipNonCode();
    this.skipIndexHeaderIfPresent();
    const out = [];
    while (true) {
      this.skipNonCode();
      if (this.text[this.pos] === "]")
        break;
      out.push(this.evalValue());
    }
    this.ensure("]");
    return out;
  }
  evalObject(_prefix) {
    this.ensure("{");
    this.skipNonCode();
    this.skipIndexHeaderIfPresent();
    const out = {};
    while (true) {
      this.skipNonCode();
      if (this.text[this.pos] === "}")
        break;
      const key = this.evalValue();
      const value = this.evalValue();
      out[String(key)] = value;
    }
    this.ensure("}");
    return out;
  }
  skipIndexHeaderIfPresent() {
    const save = this.pos;
    const countPrefix = this.readPrefix();
    if (this.text[this.pos] !== "#") {
      this.pos = save;
      return;
    }
    this.pos += 1;
    const widthChar = this.text[this.pos];
    const width = widthChar ? digitMap.get(widthChar) ?? 0 : 0;
    if (widthChar)
      this.pos += 1;
    const skipLen = countPrefix.value * width;
    this.pos += skipLen;
  }
  evalFlowParen(tag) {
    this.pos += 1;
    this.ensure("(");
    if (tag === "?") {
      const cond = this.evalValue();
      if (isDefined(cond)) {
        this.selfStack.push(cond);
        const thenValue = this.evalValue();
        this.selfStack.pop();
        if (this.hasMoreBefore(")"))
          this.skipValue();
        this.ensure(")");
        return thenValue;
      }
      this.skipValue();
      let elseValue = undefined;
      if (this.hasMoreBefore(")"))
        elseValue = this.evalValue();
      this.ensure(")");
      return elseValue;
    }
    if (tag === "!") {
      const cond = this.evalValue();
      if (!isDefined(cond)) {
        const thenValue = this.evalValue();
        if (this.hasMoreBefore(")"))
          this.skipValue();
        this.ensure(")");
        return thenValue;
      }
      this.skipValue();
      let elseValue = undefined;
      if (this.hasMoreBefore(")")) {
        this.selfStack.push(cond);
        elseValue = this.evalValue();
        this.selfStack.pop();
      }
      this.ensure(")");
      return elseValue;
    }
    if (tag === "|") {
      let result2 = undefined;
      while (this.hasMoreBefore(")")) {
        if (isDefined(result2))
          this.skipValue();
        else
          result2 = this.evalValue();
      }
      this.ensure(")");
      return result2;
    }
    let result = undefined;
    while (this.hasMoreBefore(")")) {
      if (!isDefined(result) && result !== undefined) {
        this.skipValue();
        continue;
      }
      result = this.evalValue();
      if (!isDefined(result)) {
        while (this.hasMoreBefore(")"))
          this.skipValue();
        break;
      }
    }
    this.ensure(")");
    return result;
  }
  evalLoopLike(tag) {
    this.pos += 1;
    const open = this.text[this.pos];
    if (!open || !"([{".includes(open))
      throw new Error(`Expected loop opener after '${tag}'`);
    const close = open === "(" ? ")" : open === "[" ? "]" : "}";
    this.pos += 1;
    const iterable = this.evalValue();
    const afterIterable = this.pos;
    const bodyValueCount = open === "{" ? 2 : 1;
    let varA;
    let varB;
    let bodyStart = afterIterable;
    let bodyEnd = afterIterable;
    let matched = false;
    for (const bindingCount of [2, 1, 0]) {
      this.pos = afterIterable;
      const vars = [];
      let bindingsOk = true;
      for (let index = 0;index < bindingCount; index += 1) {
        const binding = this.readBindingVarIfPresent();
        if (!binding) {
          bindingsOk = false;
          break;
        }
        vars.push(binding);
      }
      if (!bindingsOk)
        continue;
      const candidateBodyStart = this.pos;
      let cursor = candidateBodyStart;
      let bodyOk = true;
      for (let index = 0;index < bodyValueCount; index += 1) {
        const next = this.skipValueFrom(cursor);
        if (next <= cursor) {
          bodyOk = false;
          break;
        }
        cursor = next;
      }
      if (!bodyOk)
        continue;
      this.pos = cursor;
      this.skipNonCode();
      if (this.text[this.pos] !== close)
        continue;
      varA = vars[0];
      varB = vars[1];
      bodyStart = candidateBodyStart;
      bodyEnd = cursor;
      matched = true;
      break;
    }
    if (!matched)
      throw new Error(`Invalid loop/comprehension body before '${close}' at ${this.pos}`);
    this.pos = bodyEnd;
    this.ensure(close);
    if (open === "[")
      return this.evalArrayComprehension(iterable, varA, varB, bodyStart, bodyEnd, tag === "<");
    if (open === "{")
      return this.evalObjectComprehension(iterable, varA, varB, bodyStart, bodyEnd, tag === "<");
    return this.evalForLoop(iterable, varA, varB, bodyStart, bodyEnd, tag === "<");
  }
  iterate(iterable, keysOnly) {
    if (Array.isArray(iterable)) {
      if (keysOnly)
        return iterable.map((_value, index) => ({ key: index, value: index }));
      return iterable.map((value, index) => ({ key: index, value }));
    }
    if (iterable && typeof iterable === "object") {
      const entries = Object.entries(iterable);
      if (keysOnly)
        return entries.map(([key]) => ({ key, value: key }));
      return entries.map(([key, value]) => ({ key, value }));
    }
    if (typeof iterable === "number" && Number.isFinite(iterable) && iterable > 0) {
      const out = [];
      for (let index = 0;index < Math.floor(iterable); index += 1) {
        if (keysOnly)
          out.push({ key: index, value: index });
        else
          out.push({ key: index, value: index + 1 });
      }
      return out;
    }
    return [];
  }
  evalBodySlice(start, end) {
    const save = this.pos;
    this.pos = start;
    const value = this.evalValue();
    this.pos = end;
    this.pos = save;
    return value;
  }
  handleLoopControl(value) {
    if (value && typeof value === "object" && "kind" in value && "depth" in value) {
      return value;
    }
    return;
  }
  evalForLoop(iterable, varA, varB, bodyStart, bodyEnd, keysOnly) {
    const items = this.iterate(iterable, keysOnly);
    let last = undefined;
    for (const item of items) {
      this.tick();
      const currentSelf = keysOnly ? item.key : item.value;
      this.selfStack.push(currentSelf);
      if (varA && varB) {
        this.state.vars[varA] = item.key;
        this.state.vars[varB] = keysOnly ? item.key : item.value;
      } else if (varA) {
        this.state.vars[varA] = keysOnly ? item.key : item.value;
      }
      last = this.evalBodySlice(bodyStart, bodyEnd);
      this.selfStack.pop();
      const control = this.handleLoopControl(last);
      if (!control)
        continue;
      if (control.depth > 1)
        return { kind: control.kind, depth: control.depth - 1 };
      if (control.kind === "break")
        return;
      last = undefined;
      continue;
    }
    return last;
  }
  evalWhileLoop() {
    this.pos += 1;
    this.ensure("(");
    const condStart = this.pos;
    const condValue = this.evalValue();
    const bodyStart = this.pos;
    const bodyValueCount = 1;
    let cursor = bodyStart;
    for (let index = 0;index < bodyValueCount; index += 1) {
      cursor = this.skipValueFrom(cursor);
    }
    const bodyEnd = cursor;
    this.pos = bodyEnd;
    this.ensure(")");
    const afterClose = this.pos;
    let last = undefined;
    let currentCond = condValue;
    while (isDefined(currentCond)) {
      this.tick();
      this.selfStack.push(currentCond);
      last = this.evalBodySlice(bodyStart, bodyEnd);
      this.selfStack.pop();
      const control = this.handleLoopControl(last);
      if (control) {
        if (control.depth > 1)
          return { kind: control.kind, depth: control.depth - 1 };
        if (control.kind === "break")
          return;
        last = undefined;
      }
      currentCond = this.evalBodySlice(condStart, bodyStart);
    }
    this.pos = afterClose;
    return last;
  }
  evalArrayComprehension(iterable, varA, varB, bodyStart, bodyEnd, keysOnly) {
    const items = this.iterate(iterable, keysOnly);
    const out = [];
    for (const item of items) {
      this.tick();
      const currentSelf = keysOnly ? item.key : item.value;
      this.selfStack.push(currentSelf);
      if (varA && varB) {
        this.state.vars[varA] = item.key;
        this.state.vars[varB] = keysOnly ? item.key : item.value;
      } else if (varA) {
        this.state.vars[varA] = keysOnly ? item.key : item.value;
      }
      const value = this.evalBodySlice(bodyStart, bodyEnd);
      this.selfStack.pop();
      const control = this.handleLoopControl(value);
      if (control) {
        if (control.depth > 1)
          return { kind: control.kind, depth: control.depth - 1 };
        if (control.kind === "break")
          break;
        continue;
      }
      if (isDefined(value))
        out.push(value);
    }
    return out;
  }
  evalObjectComprehension(iterable, varA, varB, bodyStart, bodyEnd, keysOnly) {
    const items = this.iterate(iterable, keysOnly);
    const out = {};
    for (const item of items) {
      this.tick();
      const currentSelf = keysOnly ? item.key : item.value;
      this.selfStack.push(currentSelf);
      if (varA && varB) {
        this.state.vars[varA] = item.key;
        this.state.vars[varB] = keysOnly ? item.key : item.value;
      } else if (varA) {
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
        if (control.depth > 1)
          return { kind: control.kind, depth: control.depth - 1 };
        if (control.kind === "break")
          break;
        continue;
      }
      if (isDefined(value))
        out[String(key)] = value;
    }
    return out;
  }
  applyOpcode(id, args) {
    const custom = this.customOpcodes.get(id);
    if (custom)
      return custom(args, this.state);
    switch (id) {
      case OPCODES.do:
        return args.length ? args[args.length - 1] : undefined;
      case OPCODES.add:
        if (typeof args[0] === "string" || typeof args[1] === "string") {
          return String(args[0] ?? "") + String(args[1] ?? "");
        }
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
        if (typeof value === "boolean")
          return !value;
        return ~Number(value ?? 0);
      }
      case OPCODES.and: {
        const [a, b] = args;
        if (typeof a === "boolean" || typeof b === "boolean")
          return Boolean(a) && Boolean(b);
        return Number(a ?? 0) & Number(b ?? 0);
      }
      case OPCODES.or: {
        const [a, b] = args;
        if (typeof a === "boolean" || typeof b === "boolean")
          return Boolean(a) || Boolean(b);
        return Number(a ?? 0) | Number(b ?? 0);
      }
      case OPCODES.xor: {
        const [a, b] = args;
        if (typeof a === "boolean" || typeof b === "boolean")
          return Boolean(a) !== Boolean(b);
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
  navigate(base, keys) {
    let current = base;
    for (const key of keys) {
      if (current === undefined || current === null)
        return;
      current = current[String(key)];
    }
    return current;
  }
  readPlace() {
    this.skipNonCode();
    const direct = this.readRootVarOrRefIfPresent();
    if (direct) {
      const keys = [];
      this.skipNonCode();
      if (this.text[this.pos] === "(") {
        this.pos += 1;
        while (true) {
          this.skipNonCode();
          if (this.text[this.pos] === ")")
            break;
          keys.push(this.evalValue());
        }
        this.pos += 1;
      }
      return {
        root: direct.root,
        keys,
        isRef: direct.isRef
      };
    }
    if (this.text[this.pos] === "(") {
      this.pos += 1;
      this.skipNonCode();
      const rootFromNav = this.readRootVarOrRefIfPresent();
      if (!rootFromNav)
        throw new Error(`Invalid place root at ${this.pos}`);
      const keys = [];
      while (true) {
        this.skipNonCode();
        if (this.text[this.pos] === ")")
          break;
        keys.push(this.evalValue());
      }
      this.pos += 1;
      return {
        root: rootFromNav.root,
        keys,
        isRef: rootFromNav.isRef
      };
    }
    throw new Error(`Invalid place at ${this.pos}`);
  }
  readRootVarOrRefIfPresent() {
    const save = this.pos;
    const prefix = this.readPrefix();
    const tag = this.text[this.pos];
    if (tag !== "$" && tag !== "'") {
      this.pos = save;
      return;
    }
    this.pos += 1;
    return {
      root: tag === "$" ? prefix.raw : prefix.value,
      isRef: tag === "'"
    };
  }
  writePlace(place, value) {
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
    for (let index = 0;index < place.keys.length - 1; index += 1) {
      const key = String(place.keys[index]);
      const next = target[key];
      if (!next || typeof next !== "object")
        target[key] = {};
      target = target[key];
    }
    target[String(place.keys[place.keys.length - 1])] = value;
  }
  deletePlace(place) {
    const rootTable = place.isRef ? this.state.refs : this.state.vars;
    const rootKey = String(place.root);
    if (place.keys.length === 0) {
      delete rootTable[rootKey];
      return;
    }
    let target = rootTable[rootKey];
    if (!target || typeof target !== "object")
      return;
    for (let index = 0;index < place.keys.length - 1; index += 1) {
      target = target[String(place.keys[index])];
      if (!target || typeof target !== "object")
        return;
    }
    delete target[String(place.keys[place.keys.length - 1])];
  }
  skipValue() {
    this.pos = this.skipValueFrom(this.pos);
  }
  skipValueFrom(startPos) {
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
      const end2 = this.pos;
      this.pos = save;
      return end2;
    }
    if (tag === "=") {
      this.pos += 1;
      this.skipValue();
      this.skipValue();
      const end2 = this.pos;
      this.pos = save;
      return end2;
    }
    if (tag === "~") {
      this.pos += 1;
      this.skipValue();
      const end2 = this.pos;
      this.pos = save;
      return end2;
    }
    if (tag === "*") {
      this.pos += 1;
      this.skipValue();
      const end2 = this.pos;
      this.pos = save;
      return end2;
    }
    if ("+:%$@'^;".includes(tag)) {
      this.pos += 1;
      const end2 = this.pos;
      this.pos = save;
      return end2;
    }
    if (tag === "?" || tag === "!" || tag === "|" || tag === "&" || tag === ">" || tag === "<" || tag === "#") {
      this.pos += 1;
    }
    const opener = this.text[this.pos];
    if (opener && "([{".includes(opener)) {
      const close = opener === "(" ? ")" : opener === "[" ? "]" : "}";
      if (prefix.value > 0) {
        this.pos += 1 + prefix.value + 1;
        const end3 = this.pos;
        this.pos = save;
        return end3;
      }
      this.pos += 1;
      while (true) {
        this.skipNonCode();
        if (this.text[this.pos] === close)
          break;
        this.skipValue();
      }
      this.pos += 1;
      const end2 = this.pos;
      this.pos = save;
      return end2;
    }
    this.pos += 1;
    const end = this.pos;
    this.pos = save;
    return end;
  }
}
function evaluateRexc(text, ctx = {}) {
  const interpreter = new CursorInterpreter(text, ctx);
  const value = interpreter.evaluateTopLevel();
  return { value, state: interpreter.runtimeState };
}

// rex-repl.ts
var req = createRequire2(import.meta.url);
var { version } = req("./package.json");
var C = {
  reset: "\x1B[0m",
  bold: "\x1B[1m",
  dim: "\x1B[2m",
  red: "\x1B[31m",
  green: "\x1B[32m",
  yellow: "\x1B[33m",
  blue: "\x1B[34m",
  cyan: "\x1B[36m",
  gray: "\x1B[90m",
  boldBlue: "\x1B[1;34m"
};
var TOKEN_RE = /(?<blockComment>\/\*[\s\S]*?(?:\*\/|$))|(?<lineComment>\/\/[^\n]*)|(?<dstring>"(?:[^"\\]|\\.)*"?)|(?<sstring>'(?:[^'\\]|\\.)*'?)|(?<keyword>\b(?:when|unless|while|for|do|end|in|of|and|or|else|break|continue|delete|self)(?![a-zA-Z0-9_-]))|(?<literal>\b(?:true|false|null|undefined)(?![a-zA-Z0-9_-]))|(?<typePred>\b(?:string|number|object|array|boolean)(?![a-zA-Z0-9_-]))|(?<num>\b(?:0x[0-9a-fA-F]+|0b[01]+|(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)\b)/g;
function highlightLine(line) {
  let result = "";
  let lastIndex = 0;
  TOKEN_RE.lastIndex = 0;
  for (const m of line.matchAll(TOKEN_RE)) {
    result += line.slice(lastIndex, m.index);
    const text = m[0];
    const g = m.groups;
    if (g.blockComment || g.lineComment) {
      result += C.gray + text + C.reset;
    } else if (g.dstring || g.sstring) {
      result += C.green + text + C.reset;
    } else if (g.keyword) {
      result += C.boldBlue + text + C.reset;
    } else if (g.literal) {
      result += C.yellow + text + C.reset;
    } else if (g.typePred) {
      result += C.cyan + text + C.reset;
    } else if (g.num) {
      result += C.cyan + text + C.reset;
    } else {
      result += text;
    }
    lastIndex = m.index + text.length;
  }
  result += line.slice(lastIndex);
  return result;
}
var REXC_DIGITS = new Set("0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_");
function highlightRexc(text) {
  let out = "";
  let i = 0;
  function readPrefix() {
    const start = i;
    while (i < text.length && REXC_DIGITS.has(text[i]))
      i++;
    return text.slice(start, i);
  }
  while (i < text.length) {
    const ch = text[i];
    if (ch === " " || ch === "\t" || ch === `
` || ch === "\r") {
      out += ch;
      i++;
      continue;
    }
    if (ch === "/" && text[i + 1] === "/") {
      const start = i;
      i += 2;
      while (i < text.length && text[i] !== `
`)
        i++;
      out += C.gray + text.slice(start, i) + C.reset;
      continue;
    }
    if (ch === "/" && text[i + 1] === "*") {
      const start = i;
      i += 2;
      while (i < text.length && !(text[i] === "*" && text[i + 1] === "/"))
        i++;
      if (i < text.length)
        i += 2;
      out += C.gray + text.slice(start, i) + C.reset;
      continue;
    }
    const prefix = readPrefix();
    if (i >= text.length) {
      out += prefix;
      break;
    }
    const tag = text[i];
    switch (tag) {
      case "+":
      case "*":
        out += C.cyan + prefix + tag + C.reset;
        i++;
        break;
      case ":":
        out += C.dim + prefix + tag + C.reset;
        i++;
        break;
      case "%":
        out += C.boldBlue + prefix + tag + C.reset;
        i++;
        break;
      case "$":
        out += C.yellow + prefix + tag + C.reset;
        i++;
        break;
      case "@":
        out += C.yellow + prefix + tag + C.reset;
        i++;
        break;
      case "'":
        out += C.dim + prefix + tag + C.reset;
        i++;
        break;
      case ",": {
        i++;
        let len = 0;
        for (const ch2 of prefix)
          len = len * 64 + (REXC_DIGITS.has(ch2) ? "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_".indexOf(ch2) : 0);
        const content = text.slice(i, i + len);
        i += len;
        out += C.green + prefix + "," + content + C.reset;
        break;
      }
      case "=":
      case "~":
        out += C.red + prefix + tag + C.reset;
        i++;
        break;
      case "?":
      case "!":
      case "|":
      case "&":
      case ">":
      case "<":
      case "#":
        out += C.boldBlue + prefix + tag + C.reset;
        i++;
        break;
      case ";":
        out += C.boldBlue + prefix + tag + C.reset;
        i++;
        break;
      case "^":
        out += C.dim + prefix + tag + C.reset;
        i++;
        break;
      case "(":
      case ")":
      case "[":
      case "]":
      case "{":
      case "}":
        out += C.dim + prefix + C.reset + tag;
        i++;
        break;
      default:
        out += prefix + tag;
        i++;
        break;
    }
  }
  return out;
}
function stripStringsAndComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'/g, (m) => " ".repeat(m.length));
}
function countWord(text, word) {
  const re = new RegExp(`\\b${word}(?![a-zA-Z0-9_-])`, "g");
  return (text.match(re) || []).length;
}
function isIncomplete(buffer) {
  try {
    if (grammar.match(buffer).succeeded())
      return false;
  } catch {}
  const stripped = stripStringsAndComments(buffer);
  let parens = 0, brackets = 0, braces = 0;
  for (const ch of stripped) {
    if (ch === "(")
      parens++;
    else if (ch === ")")
      parens--;
    else if (ch === "[")
      brackets++;
    else if (ch === "]")
      brackets--;
    else if (ch === "{")
      braces++;
    else if (ch === "}")
      braces--;
  }
  if (parens > 0 || brackets > 0 || braces > 0)
    return true;
  const doCount = countWord(stripped, "do");
  const endCount = countWord(stripped, "end");
  if (doCount > endCount)
    return true;
  const trimmed = buffer.trimEnd();
  if (/[+\-*/%&|^=<>]$/.test(trimmed))
    return true;
  if (/\b(?:and|or|do|in|of)\s*$/.test(trimmed))
    return true;
  return false;
}
function formatResult(value) {
  let text;
  try {
    text = stringify(value, { maxWidth: 60 });
  } catch {
    text = String(value);
  }
  return `${C.gray}→${C.reset} ${highlightLine(text)}`;
}
function formatVarState(vars) {
  const entries = Object.entries(vars);
  if (entries.length === 0)
    return "";
  const MAX_LINE = 70;
  const MAX_VALUE = 30;
  const parts = [];
  let totalLen = 0;
  for (const [key, val] of entries) {
    let valStr;
    try {
      valStr = stringify(val, { maxWidth: MAX_VALUE });
    } catch {
      valStr = String(val);
    }
    if (valStr.length > MAX_VALUE) {
      valStr = valStr.slice(0, MAX_VALUE - 1) + "…";
    }
    const part = `${key} = ${valStr}`;
    if (totalLen + part.length + 2 > MAX_LINE && parts.length > 0) {
      parts.push("…");
      break;
    }
    parts.push(part);
    totalLen += part.length + 2;
  }
  return `${C.dim}  ${parts.join(", ")}${C.reset}`;
}
var KEYWORDS = [
  "when",
  "unless",
  "while",
  "for",
  "do",
  "end",
  "in",
  "of",
  "and",
  "or",
  "else",
  "break",
  "continue",
  "delete",
  "self",
  "true",
  "false",
  "null",
  "undefined",
  "string",
  "number",
  "object",
  "array",
  "boolean"
];
function completer(state) {
  return (line) => {
    const match = line.match(/[a-zA-Z_][a-zA-Z0-9_.-]*$/);
    const partial = match ? match[0] : "";
    if (!partial)
      return [[], ""];
    const varNames = Object.keys(state.vars);
    const all = [...new Set([...KEYWORDS, ...varNames])];
    const hits = all.filter((w) => w.startsWith(partial));
    return [hits, partial];
  };
}
function handleDotCommand(cmd, state, rl) {
  function toggleLabel(on) {
    return on ? `${C.green}on${C.reset}` : `${C.dim}off${C.reset}`;
  }
  switch (cmd) {
    case ".help":
      console.log([
        `${C.boldBlue}Rex REPL Commands:${C.reset}`,
        "  .help   Show this help message",
        "  .vars   Show all current variables",
        "  .clear  Clear all variables",
        "  .ir     Toggle showing IR JSON after parsing",
        "  .rexc   Toggle showing compiled rexc before execution",
        "  .opt    Toggle IR optimizations",
        "  .exit   Exit the REPL",
        "",
        "Enter Rex expressions to evaluate them.",
        "Multi-line: open brackets or do/end blocks continue on the next line.",
        "Ctrl-C cancels multi-line input.",
        "Ctrl-D exits."
      ].join(`
`));
      return true;
    case ".ir":
      state.showIR = !state.showIR;
      console.log(`${C.dim}  IR display: ${toggleLabel(state.showIR)}${C.reset}`);
      return true;
    case ".rexc":
      state.showRexc = !state.showRexc;
      console.log(`${C.dim}  Rexc display: ${toggleLabel(state.showRexc)}${C.reset}`);
      return true;
    case ".opt":
      state.optimize = !state.optimize;
      console.log(`${C.dim}  Optimizations: ${toggleLabel(state.optimize)}${C.reset}`);
      return true;
    case ".vars": {
      const entries = Object.entries(state.vars);
      if (entries.length === 0) {
        console.log(`${C.dim}  (no variables)${C.reset}`);
      } else {
        for (const [key, val] of entries) {
          let valStr;
          try {
            valStr = stringify(val, { maxWidth: 60 });
          } catch {
            valStr = String(val);
          }
          console.log(`  ${key} = ${highlightLine(valStr)}`);
        }
      }
      return true;
    }
    case ".clear":
      state.vars = {};
      state.refs = {};
      console.log(`${C.dim}  Variables cleared.${C.reset}`);
      return true;
    case ".exit":
      rl.close();
      return true;
    default:
      if (cmd.startsWith(".")) {
        console.log(`${C.red}  Unknown command: ${cmd}. Type .help for available commands.${C.reset}`);
        return true;
      }
      return false;
  }
}
var GAS_LIMIT = 1e7;
async function startRepl() {
  const state = { vars: {}, refs: {}, showIR: false, showRexc: false, optimize: false };
  let multiLineBuffer = "";
  const PRIMARY_PROMPT = "rex> ";
  const CONT_PROMPT = "...  ";
  const STYLED_PRIMARY = `${C.boldBlue}rex${C.reset}> `;
  const STYLED_CONT = `${C.dim}...${C.reset}  `;
  let currentPrompt = PRIMARY_PROMPT;
  let styledPrompt = STYLED_PRIMARY;
  console.log(`${C.boldBlue}Rex${C.reset} v${version} — type ${C.dim}.help${C.reset} for commands`);
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    prompt: PRIMARY_PROMPT,
    historySize: 500,
    completer: completer(state),
    terminal: true
  });
  process.stdin.on("keypress", () => {
    process.nextTick(() => {
      if (!rl.line && rl.line !== "")
        return;
      readline.clearLine(process.stdout, 0);
      readline.cursorTo(process.stdout, 0);
      process.stdout.write(styledPrompt + highlightLine(rl.line));
      readline.cursorTo(process.stdout, currentPrompt.length + rl.cursor);
    });
  });
  process.stdin.on("keypress", (_ch, key) => {
    if (key?.ctrl && key.name === "l") {
      readline.cursorTo(process.stdout, 0, 0);
      readline.clearScreenDown(process.stdout);
      rl.prompt();
    }
  });
  rl.on("SIGINT", () => {
    if (multiLineBuffer) {
      multiLineBuffer = "";
      currentPrompt = PRIMARY_PROMPT;
      styledPrompt = STYLED_PRIMARY;
      rl.setPrompt(PRIMARY_PROMPT);
      process.stdout.write(`
`);
      rl.prompt();
    } else {
      console.log();
      rl.close();
    }
  });
  function resetPrompt() {
    currentPrompt = PRIMARY_PROMPT;
    styledPrompt = STYLED_PRIMARY;
    rl.setPrompt(PRIMARY_PROMPT);
    rl.prompt();
  }
  rl.on("line", (line) => {
    const trimmed = line.trim();
    if (!multiLineBuffer && trimmed.startsWith(".")) {
      if (handleDotCommand(trimmed, state, rl)) {
        rl.prompt();
        return;
      }
    }
    multiLineBuffer += (multiLineBuffer ? `
` : "") + line;
    if (multiLineBuffer.trim() === "") {
      multiLineBuffer = "";
      rl.prompt();
      return;
    }
    if (isIncomplete(multiLineBuffer)) {
      currentPrompt = CONT_PROMPT;
      styledPrompt = STYLED_CONT;
      rl.setPrompt(CONT_PROMPT);
      rl.prompt();
      return;
    }
    const source = multiLineBuffer;
    multiLineBuffer = "";
    const match = grammar.match(source);
    if (!match.succeeded()) {
      console.log(`${C.red}  ${match.message}${C.reset}`);
      resetPrompt();
      return;
    }
    try {
      const ir = parseToIR(source);
      const lowered = state.optimize ? optimizeIR(ir) : ir;
      if (state.showIR) {
        console.log(`${C.dim}  IR: ${JSON.stringify(lowered)}${C.reset}`);
      }
      const rexc = compile(source, { optimize: state.optimize });
      if (state.showRexc) {
        console.log(`${C.dim}  rexc:${C.reset} ${highlightRexc(rexc)}`);
      }
      const result = evaluateRexc(rexc, {
        vars: { ...state.vars },
        refs: { ...state.refs },
        gasLimit: GAS_LIMIT
      });
      state.vars = result.state.vars;
      state.refs = result.state.refs;
      console.log(formatResult(result.value));
      const varLine = formatVarState(state.vars);
      if (varLine)
        console.log(varLine);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (message.includes("Gas limit exceeded")) {
        console.log(`${C.yellow}  ${message}${C.reset}`);
      } else {
        console.log(`${C.red}  Error: ${message}${C.reset}`);
      }
    }
    resetPrompt();
  });
  rl.on("close", () => {
    process.exit(0);
  });
  rl.prompt();
}
export {
  startRepl,
  isIncomplete,
  highlightRexc,
  highlightLine,
  formatVarState
};

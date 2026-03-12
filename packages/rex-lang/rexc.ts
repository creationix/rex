export interface RexCEncodeOptions {
	// Enable path chains. (substring de-dupe in paths, requires pointers)
	chainSplit?: string | false;
	// indexes (lists/maps greater than or equal to this have indexes added))
	indexes?: number | false;
	// Stream to callback instead of returning buffer.
	onChunk?: (chunk: Uint8Array, offset: number) => void;
	// External dictionary of known values (UPPERCASE KEYS)
	refs?: Record<string, unknown>;
}

export interface RexCDecodeOptions {
	// Lazy parse on access using Proxy object
	lazy?: boolean;
	// External dictionary of known values, Must match encoder.
	refs?: Record<string, unknown>;
}

export const BUILTIN_REFS: Record<string, unknown> = {
	n: null,
	t: true,
	f: false,
	u: undefined,
	nan: NaN,
	inf: Infinity,
	nif: -Infinity,
};

const ENCODE_DEFAULTS = {
	chainSplit: "/",
	indexes: 32,
	refs: {},
} as const satisfies Partial<RexCEncodeOptions>;

const DECODE_DEFAULTS = {
	lazy: false,
	refs: {},
} as const satisfies Partial<RexCDecodeOptions>;

// Encode a signed integer as an unsigned zigzag value
export function toZigZag(num: number): number {
	// For small numbers, we can do this with bitwise operations.
	if (num >= -0x80000000 && num <= 0x7fffffff) {
		return (num << 1) ^ (num >> 31);
	}
	// For larger numbers, we need to use arithmetic to avoid overflow issues.
	return num < 0 ? num * -2 - 1 : num * 2;
}

// Decode an unsigned zigzag value back to a signed integer
export function fromZigZag(num: number): number {
	// For small numbers, we can do this with bitwise operations.
	if (num <= 0xffffffff) {
		return (num >>> 1) ^ -(num & 1);
	}
	// For larger numbers, we need to use arithmetic to avoid overflow issues.
	return num % 2 === 0 ? num / 2 : (num + 1) / -2;
}

const b64Chars =
	"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";

// charCode -> digit value (0xff = invalid)
const b64Lookup = new Uint8Array(128).fill(0xff);
// digit value -> charCode
const b64Codes = new Uint8Array(64);
for (let i = 0; i < 64; i++) {
	const code = b64Chars.charCodeAt(i);
	b64Lookup[code] = i;
	b64Codes[i] = code;
}

export function toB64Signed(num: number): string {
	return toB64(toZigZag(num));
}

export function fromB64Signed(str: string): number {
	return fromZigZag(fromB64(str));
}

const b64Regex = /^[0-9a-zA-Z\-_]*$/;

function writeStringPair(tag: string, value: string) {
	if (!b64Regex.test(value)) {
		throw new TypeError(
			`String contains invalid characters for inline encoding: ${value}`,
		);
	}
	return tag + value;
}

function writeUnsigned(tag: string, value: number) {
	if (value < 0) {
		throw new RangeError(`Value must be non-negative, got ${value}`);
	}
	return `${tag}${toB64(value)}`;
}

function writeSigned(tag: string, value: number) {
	return `${tag}${toB64(toZigZag(value))}`;
}

export function toB64(num: number): string {
	let result = "";
	while (num > 0) {
		result = b64Chars[num % 64] + result;
		num = Math.floor(num / 64);
	}
	return result;
}
export function fromB64(str: string): number {
	let result = 0;
	for (let i = 0; i < str.length; i++) {
		const value = b64Lookup[str.charCodeAt(i)] as number;
		if (value === 0xff) {
			throw new Error(`Invalid base64 character: ${str[i]}`);
		}
		result = result * 64 + value;
	}
	return result;
}

// Scratch space for digit extraction (max 9 digits for MAX_SAFE_INTEGER in base64)
const b64Scratch = new Uint8Array(10);

// Writes the base64 varint encoding of the value into the buffer at the given offset,
// and returns the new offset.
export function writeB64(
	buffer: Uint8Array,
	offset: number,
	value: number,
): number {
	if (value === 0) return offset;
	// Extract digits LSB-first into scratch
	let len = 0;
	while (value > 0) {
		b64Scratch[len++] = b64Codes[value % 64] as number;
		value = Math.floor(value / 64);
	}
	// Write MSB-first into buffer
	for (let i = len - 1; i >= 0; i--) {
		buffer[offset++] = b64Scratch[i] as number;
	}
	return offset;
}

export function readB64(
	buffer: Uint8Array,
	offset: number,
	length: number,
): number {
	let result = 0;
	for (let i = 0; i < length; i++) {
		const code = buffer[offset + i] as number;
		const value = b64Lookup[code] as number;
		if (value === 0xff) {
			throw new Error(`Invalid base64 character: ${String.fromCharCode(code)}`);
		}
		result = result * 64 + value;
	}
	return result;
}

export type RexCStringifyOptions = Omit<RexCEncodeOptions, "onChunk"> & {
	onChunk?: (chunk: string, offset: number) => void;
};

export function stringify(
	value: unknown,
	options: RexCStringifyOptions & {
		onChunk: (chunk: string, offset: number) => void;
	},
): undefined;
export function stringify(
	value: unknown,
	options?: RexCStringifyOptions,
): string;
export function stringify(
	value: unknown,
	options?: RexCStringifyOptions,
): string | undefined {
	const { onChunk, ...rest } = options ?? {};
	if (onChunk) {
		encode(value, {
			...rest,
			onChunk: (chunk, offset) =>
				onChunk(new TextDecoder().decode(chunk), offset),
		});
		return undefined;
	}
	return new TextDecoder().decode(encode(value, rest));
}

export function encode(
	value: unknown,
	options: RexCEncodeOptions & {
		onChunk: (chunk: Uint8Array, offset: number) => void;
	},
): undefined;
export function encode(value: unknown, options?: RexCEncodeOptions): Uint8Array;
export function encode(
	rootValue: unknown,
	options?: RexCEncodeOptions,
): Uint8Array | undefined {
	const opts = { ...ENCODE_DEFAULTS, ...options };
	const parts: Uint8Array[] = [];
	let byteLength = 0;
	const onChunk = opts.onChunk ?? ((chunk) => parts.push(chunk));
	const chainSplit = opts.chainSplit;
	const refs = Object.fromEntries(
		(Object.entries({ ...opts.refs }) as [string, unknown][]).map(
			([key, val]) => [makeKey(val), key],
		),
	);
	const indexThreshold =
		typeof opts.indexes === "number" ? opts.indexes : Infinity;
	// Map from value identity to encoded offset, used for pointers
	const seenOffsets: Record<string, number> = {};
	// Map from schema identity to offset of either array of object with same shape
	const schemaOffsets: Record<string, number> = {};
	const seenCosts: Record<string, number> = {};

	// Pre-scan the dataset to find reused path prefixes
	const duplicatePrefixes = new Set<string>();
	const seenPrefixes = new Set<string>();
	scanPrefixes(rootValue);
	function scanPrefixes(value: unknown) {
		if (!chainSplit) return;
		if (typeof value === "string" && value.indexOf(chainSplit) >= 0) {
			let offset = 0;
			if (!seenPrefixes.has(value)) {
				while (offset < value.length) {
					const nextDelimiter = value.indexOf(chainSplit, offset + 1);
					if (nextDelimiter === -1) break;
					const prefix = value.slice(0, nextDelimiter);
					if (seenPrefixes.has(prefix)) {
						duplicatePrefixes.add(prefix);
					} else {
						seenPrefixes.add(prefix);
					}
					offset = nextDelimiter;
				}
			}
		} else if (value && typeof value === "object") {
			if (Array.isArray(value)) {
				for (const item of value) {
					scanPrefixes(item);
				}
			} else {
				for (const [key, val] of Object.entries(value)) {
					scanPrefixes(key);
					scanPrefixes(val);
				}
			}
		}
	}

	writeAny(rootValue);

	// In streaming mode, there is no final buffer to return.
	if (opts.onChunk) return undefined;
	const output = new Uint8Array(byteLength);
	let offset = 0;
	for (const chunk of parts) {
		output.set(chunk, offset);
		offset += chunk.byteLength;
	}
	return output;

	function pushBytes(bytes: Uint8Array) {
		onChunk(bytes, byteLength);
		return (byteLength += bytes.byteLength);
	}

	function pushString(str: string) {
		const bytes = new TextEncoder().encode(str);
		return pushBytes(bytes);
	}

	function writeAny(value: unknown) {
		const key = makeKey(value);
		const refKey = refs[key];
		if (refKey !== undefined) {
			return pushString(writeStringPair("'", refKey));
		}
		const seenOffset = seenOffsets[key];
		if (seenOffset !== undefined) {
			const delta = byteLength - seenOffset;
			const seenCost = seenCosts[key] ?? 0;
			const pointerCost = Math.ceil(Math.log(delta + 1) / Math.log(64)) + 1;
			if (pointerCost < seenCost) {
				return pushString(writeUnsigned("^", delta));
			}
		}

		const before = byteLength;
		const ret = writeAnyInner(value);
		seenOffsets[key] = byteLength;
		seenCosts[key] = byteLength - before;
		return ret;
	}

	function writeAnyInner(value: unknown) {
		switch (typeof value) {
			case "string":
				return writeString(value);
			case "number":
				return writeNumber(value);
			case "boolean":
				return pushString(writeStringPair("'", value ? "t" : "f"));
			case "undefined":
				return pushString(writeStringPair("'", "u"));
			case "object":
				if (value === null) return pushString(writeStringPair("'", "n"));
				if (Array.isArray(value)) return writeArray(value);
				return writeObject(value as Record<string, unknown>);
			default:
				throw new TypeError(`Unsupported value type: ${typeof value}`);
		}
	}

	function writeString(value: string) {
		if (chainSplit && value.indexOf(chainSplit) >= 0) {
			// We need to write the string last-segments first, but only split when needed
			let offset = value.length;
			let head: string | undefined;
			let tail: string | undefined;
			while (offset > 0) {
				offset = value.lastIndexOf(chainSplit, offset - 1);
				if (offset <= 0) break;
				const prefix = value.slice(0, offset);
				if (duplicatePrefixes.has(prefix)) {
					// Grab head and tail
					head = prefix;
					tail = value.substring(offset);
					break;
				}
			}
			if (head && tail) {
				const before = byteLength;
				writeAny(tail);
				writeAny(head);
				const size = byteLength - before;
				return pushString(writeUnsigned(".", size));
			}
		}
		const utf8 = new TextEncoder().encode(value);
		pushBytes(utf8);
		return pushString(writeUnsigned(",", utf8.byteLength));
	}

	function writeNumber(value: number) {
		if (Number.isNaN(value)) {
			return pushString(writeStringPair("'", "nan"));
		}
		if (value === Infinity) {
			return pushString(writeStringPair("'", "inf"));
		}
		if (value === -Infinity) {
			return pushString(writeStringPair("'", "nif"));
		}
		const [base, exp] = splitNumber(value);
		if (
			exp >= 0 &&
			exp < 5 &&
			Number.isInteger(base) &&
			Number.isSafeInteger(base)
		) {
			return pushString(writeSigned("+", value));
		}
		pushString(writeSigned("+", base));
		return pushString(writeSigned("*", exp));
	}

	function writeArray(value: unknown[]) {
		const start = byteLength;
		writeValues(value);
		return pushString(writeUnsigned(";", byteLength - start));
	}

	// Write values in reverse order, and optionally write an index last for O(1) random access.
	function writeValues(values: unknown[]) {
		const length = values.length;
		const offsets = length > indexThreshold ? new Array(length) : undefined;
		for (let f = length - 1, i = f; i >= 0; i--) {
			writeAny(values[i]);
			if (offsets) {
				offsets[i] = byteLength;
			}
		}
		if (offsets) {
			const lastOffset = offsets[offsets.length - 1] as number;
			const width = Math.ceil(
				Math.log(byteLength - lastOffset + 1) / Math.log(64),
			);
			const pointers = offsets
				.map((offset) => toB64(byteLength - offset).padStart(width, "0"))
				.join("");
			pushString(pointers);
			if (width > 8) {
				throw new Error(
					`Index width exceeds maximum of 8 characters: ${width}`,
				);
			}
			pushString(writeUnsigned("#", (values.length << 3) | (width - 1)));
		}
	}

	function writeObject(value: Record<string, unknown>) {
		const keys = Object.keys(value);
		const length = keys.length;
		if (length === 0) {
			return pushString(":");
		}

		// Check for schemas
		const keysKey = makeKey(keys);
		const schemaRef = refs[keysKey];
		const schemaTarget = schemaOffsets[keysKey] ?? seenOffsets[keysKey];
		if (schemaRef !== undefined || schemaTarget !== undefined) {
			return writeSchemaObject(
				value,
				(schemaRef ?? schemaTarget) as string | number,
			);
		}
		const before = byteLength;
		const offsets =
			length > indexThreshold ? ({} as Record<string, number>) : undefined;
		let lastOffset: number | undefined;
		const entries = Object.entries(value);
		for (let f = entries.length - 1, i = f; i >= 0; i--) {
			const [key, val] = entries[i] as [string, unknown];
			writeAny(val);
			writeAny(key);
			if (offsets) {
				offsets[key] = byteLength;
				lastOffset = lastOffset ?? byteLength;
			}
		}

		if (offsets && lastOffset !== undefined) {
			const width = Math.ceil(
				Math.log(byteLength - lastOffset + 1) / Math.log(64),
			);
			const pointers = Object.entries(offsets)
				// Sort by UTF-8 representation of keys
				.sort(([a], [b]) => utf8Sort(a, b))
				// Map to width width base64 offsets
				.map(([, offset]) => toB64(byteLength - offset).padStart(width, "0"))
				.join("");
			pushString(pointers);
			if (width > 8) {
				throw new Error(
					`Index width exceeds maximum of 8 characters: ${width}`,
				);
			}
			pushString(writeUnsigned("#", (length << 3) | (width - 1)));
		}
		const ret = pushString(writeUnsigned(":", byteLength - before));
		schemaOffsets[keysKey] = byteLength;
		return ret;
	}

	function writeSchemaObject(
		value: Record<string, unknown>,
		target: string | number,
	) {
		const before = byteLength;
		writeValues(Object.values(value));
		if (typeof target === "string") {
			pushString(writeStringPair("'", target));
		} else {
			pushString(writeUnsigned("^", byteLength - target));
		}
		return pushString(writeUnsigned(":", byteLength - before));
	}
}

// These fields are for parser context and are not enumerable
// This means the tests don't need to include them in expected values
// And they are not included when encoding nodes as JSON.
type RxContext = Readonly<{
	data: Uint8Array;
	refs: Record<string, unknown>;
}>

type RxCommon = RxContext & Readonly<{
	type: string;
	left: number;
	right: number;
}>;

export type RxNode = RxCommon &
	(
		| RxPrimitive
		| RxPointer
		| RxChain
		| RxObject
		| RxArray
	);
// Primitives are string, number, boolean, null or undefined values.
export type RxPrimitive = RxCommon & Readonly<{
	type: "primitive";
	value: string | number | boolean | null | undefined;
}>;
export type RxChain = RxCommon & Readonly<{
	type: "chain";
}>;
export type RxPointer = RxCommon & Readonly<{
	type: "pointer";
	target: string | number;
}>;
export type RxObject = RxCommon & Readonly<{
	type: "object";
	content: number;
	schema?: string | number;
	index?: RxIndex;
}>;
export type RxArray = RxCommon & Readonly<{
	type: "array";
	content: number;
	index?: RxIndex;
}>;
export type RxIndex = Readonly<{
	width: number;
	count: number;
}>;

// Return true if byte is 0-9, a-z, A-Z, '-' or '_'
function isB64(byte: number): boolean {
	return (
		(byte >= 48 && byte <= 57) ||
		(byte >= 97 && byte <= 122) ||
		(byte >= 65 && byte <= 90) ||
		byte === 45 ||
		byte === 95
	);
}

const b64ValueLookup = new Uint8Array(256).fill(0xff);
for (let i = 0; i < 64; i++) {
	b64ValueLookup[b64Chars.charCodeAt(i)] = i;
}

// Skip backwards till we reach the end of b64 digits
// offset is a right-side boundary, the result is a left-side boundary
export function b64Skip(data: Uint8Array, offset: number) {
	while (isB64(data[--offset] ?? 0));
	return offset + 1;
}

// right is after last base64 digit
// left is before first base64 digit
// Digits are big-endian so the left-most digit is the most significant
export function b64Decode(
	data: Uint8Array,
	left: number,
	right: number,
): number {
	let result = 0;
	for (let i = left; i < right; i++) {
		result = result * 64 + b64ValueLookup[data[i]!]!;
	}
	return result;
}

// Convert from unsigned zigzag value back to signed integer
export function zigzagDecode(num: number): number {
	if (num <= 0xffffffff) {
		return (num >>> 1) ^ -(num & 1);
	}
	return num % 2 === 0 ? num / 2 : (num + 1) / -2;
}

function peek(data: Uint8Array, right: number): [number, number] {
	let left = b64Skip(data, right);
	if (left <= 0) {
		throw new SyntaxError("Unexpected end of input seeking for non-b64 tag");
	}
	const tag = data[--left]!;
	return [left, tag]
}

function unpackIndex(data: Uint8Array, left: number, right: number): RxIndex {
	const b64 = b64Decode(data, left, right);
	return {
		width: (b64 & 0b111) + 1,
		count: b64 >> 3,
	}
}

export function get(data: Uint8Array, refs: Record<string, unknown> = {}, right = data.length): Readonly<RxNode> {
	// console.log({
	// 	data,
	// 	offset,
	// 	str: new TextDecoder().decode(
	// 		data.subarray(Math.max(0, offset - 40), offset),
	// 	),
	// });
	let [left, tag] = peek(data, right);
	if (tag === 0x27) {
		// ' -- builtin reference
		const ref: string = new TextDecoder().decode(
			data.subarray(left + 1, right),
		);
		if (ref in BUILTIN_REFS) {
			return withData(data, refs, {
				type: "primitive",
				left,
				right,
				value: BUILTIN_REFS[ref],
			}) as Readonly<RxPrimitive>;
		}
		return withData(data, refs, {
			type: "pointer",
			left,
			right,
			target: ref,
		}) as Readonly<RxPointer>;
	}
	const b64 = b64Decode(data, left + 1, right);
	switch (tag) {
		case 0x2c: // , -- string
			return withData(data, refs, {
				type: "primitive",
				left: left - b64,
				right,
				value: new TextDecoder().decode(data.subarray(left - b64, left)),
			}) as Readonly<RxPrimitive>;
		case 0x3b: {
			// ; -- array
			let content = left;
			left -= b64;
			let index: RxIndex | undefined;
			if (content > left) {
				const [l, t] = peek(data, content);
				if (t === 0x23) { // # -- index
					index = unpackIndex(data, l + 1, content);
					content = l - index.width * index.count;
				}
			}
			return withData(data, refs, {
				type: "array",
				left,
				right,
				content,
				index,
			}) as Readonly<RxArray>;
		}
		case 0x3a: {
			// : -- object
			let content = left;
			left -= b64;
			let index: RxIndex | undefined;
			let schema: string | number | undefined;
			while (content > left) {
				const [l, t] = peek(data, content);
				if (t === 0x23 && index === undefined) {
					// # -- index
					index = unpackIndex(data, l + 1, content);
					content = l - index.width * index.count;
				} else if ((t === 0x27 || t === 0x5e) && schema === undefined) {
					// ' -- schema reference or ^ -- schema pointer
					const ptr = get(data, refs, content) as RxPointer;
					if (ptr.type !== "pointer") {
						throw new SyntaxError("Unexpected schema reference type");
					}
					schema = ptr.target;
					content = l;
				} else {
					break;
				}
			}
			return withData(data, refs, {
				type: "object",
				left,
				right,
				content,
				schema,
				index,
			}) as Readonly<RxObject>;
		}
		case 0x5e: // ^ -- pointer
			return withData(data, refs, {
				type: "pointer",
				left,
				right,
				target: left - b64,
			}) as Readonly<RxPointer>;
		case 0x2e: { // . -- chain
			return withData(data, refs, {
				type: "chain",
				left,
				right,
			}) as Readonly<RxChain>;
			throw new Error("TODO: implement string chain reading");
		}
		case 0x2a: {
			// * -- decimal exponent
			const int = get(data, refs, left);
			if (int.type === "primitive" && typeof int.value === "number") {
				return withData(data, refs, {
					type: "primitive",
					left: int.left,
					right,
					value: parseFloat(`${int.value}e${zigzagDecode(b64)}`),
				}) as Readonly<RxPrimitive>;
			}
			throw new SyntaxError("Invalid number format in decimal");
		}
		case 0x2b: // + -- integer base
			return withData(data, refs, {
				type: "primitive",
				left,
				right,
				value: zigzagDecode(b64)
			}) as Readonly<RxPrimitive>;
		default:
			throw new SyntaxError(`Unknown tag: ${String.fromCharCode(tag)}`);
	}
}

export function* getEntries(node: RxObject): Generator<[string, RxNode]> {
	if (node.type !== "object") {
		throw new TypeError("Node must be an object");
	}
	const { data, refs, schema } = node
	let right = node.content;
	if (schema !== undefined) {
		let keys: Iterable<string>;
		if (typeof schema === "string") {
			if (!(schema in refs)) {
				throw new ReferenceError(`Unknown schema reference: ${schema}`);
			}
			const ref = refs[schema];
			if (typeof ref !== "object" || ref === null) {
				throw new TypeError("Schema reference must point to an object or array");
			}
			keys = Array.isArray(ref) ? ref : Object.keys(ref);
		} else if (typeof schema === "number") {
			let targetNode = get(data, refs, schema);
			if (targetNode.type === "array") {
				keys = getEach(targetNode).map((k) => {
					if (k.type === "primitive" && typeof k.value === "string") {
						return k.value;
					}
					throw new TypeError("Schema reference array must contain only string primitives");
				}) as Iterable<string>;
			} else if (targetNode.type === "object") {
				keys = getEntries(targetNode).map(([k]) => k)
			} else {
				throw new TypeError("Schema reference must point to an object or array");
			}
		} else {
			throw new TypeError("Invalid schema reference type");
		}
		const values = getEach(node);
		// Zip keys and values together, with keys coming from the schema reference and values coming from the object content.
		const keysIter = keys[Symbol.iterator]();
		for (const value of values) {
			const key = keysIter.next();
			if (key.done) {
				throw new SyntaxError("Not enough keys in schema reference for object entries");
			}
			if (typeof key.value !== "string") {
				throw new TypeError("Schema reference keys must be strings");
			}
			yield [key.value, value];
		}
		return;
	}
	while (right > node.left) {
		const key = get(data, refs, right);
		if (!(key.type === "primitive" && typeof key.value === "string")) {
			throw new SyntaxError("Expected string key in object");
		}
		right = key.left;
		const value = get(data, refs, right);
		right = value.left;
		yield [key.value, value];
	}
}

// Get values.  This can be called on objects and will return all entries as values
export function* getEach(node: RxArray | RxObject): Generator<RxNode> {
	if (node.type !== "array" && node.type !== "object") {
		throw new TypeError("Node must be an array or object");
	}
	const { data, refs } = node;
	let right = node.content;
	while (right > node.left) {
		const value = get(data, refs, right);
		right = value.left;
		yield value;
	}
}

export function decode(
	input: Uint8Array,
	options?: RexCDecodeOptions,
): unknown {
	const opts = { ...DECODE_DEFAULTS, ...options };
	if (opts.lazy) {
		return lazyDecode(input, opts.refs);
	}
	return eagerDecode(input, opts.refs);
}

export function lazyDecode(input: Uint8Array, refs: Record<string, unknown> = {}): unknown {
	throw new Error("TODO: implement lazy decode");
}

export function eagerDecode(input: Uint8Array, refs: Record<string, unknown> = {}): unknown {
	return resolve(get(input, refs));
}

export function resolve(node: RxNode): unknown {
	const { data, refs } = node
	if (node.type === "primitive") {
		return node.value;
	}
	if (node.type === "pointer") {
		const target = node.target
		if (typeof target === "string") {
			if (target in refs) {
				return refs[target];
			}
			throw new ReferenceError(`Unknown reference: ${target}`);
		} else if (typeof target === "number") {
			return resolve(get(data, refs, target));
		}
		throw new TypeError(`Invalid pointer target type: ${typeof target}`);
	}
	if (node.type === "object") {
		const obj: Record<string, unknown> = {};
		for (const [key, value] of getEntries(node)) {
			obj[key] = resolve(value);
		}
		return obj;
	}
	if (node.type === "array") {
		return Array.from(getEach(node)).map((value) => resolve(value));
	}
	throw new TypeError(`Unknown node type - ${JSON.stringify(node)}`);
}

export function parse(input: string, options?: RexCDecodeOptions): unknown {
	return decode(new TextEncoder().encode(input), options);
}

// Input is an integer string.
// returns base and number of zeroes that were trimmed
function trimZeroes(str: string): [number, number] {
	const trimmed = str.replace(/0+$/, "");
	const zeroCount = str.length - trimmed.length;
	return [parseInt(trimmed, 10), zeroCount];
}

// Given a double value, split it into a base and power of 10.
// For example, 1234.5678 would be split into 12345678 and -4.
export function splitNumber(val: number): [number, number] {
	if (Number.isInteger(val)) {
		if (Math.abs(val) < 10) {
			return [val, 0];
		}
		if (Math.abs(val) < 9.999999999999999e20) {
			return trimZeroes(val.toString());
		}
	}
	// Try decimal representation first
	const decStr = val.toPrecision(14).match(/^([-+]?\d+)(?:\.(\d+))?$/);
	if (decStr) {
		const b1 = parseInt((decStr[1] ?? "") + (decStr[2] ?? ""), 10);
		const e1 = -(decStr[2]?.length ?? 0);
		if (e1 === 0) {
			return [b1, 0];
		}
		const [b2, e2] = splitNumber(b1);
		return [b2, e1 + e2];
	}
	// Then try scientific notation
	const sciStr = val
		.toExponential(14)
		.match(/^([+-]?\d+)(?:\.(\d+))?(?:e([+-]?\d+))$/);
	if (sciStr) {
		// Count the decimal places
		const e1 = -(sciStr[2]?.length ?? 0);
		// Parse the exponent
		const e2 = parseInt(sciStr[3] ?? "0", 10);
		// Parse left of e as integer with zeroes trimmed
		const [b1, e3] = trimZeroes(sciStr[1] + (sciStr[2] ?? ""));
		return [b1, e1 + e2 + e3];
	}
	throw new Error(`Invalid number format: ${val}`);
}

// Map from object to JSON key
const KeyMap = new WeakMap<object, string>();
function makeKey(val: unknown): string {
	if (val && typeof val === "object") {
		let key = KeyMap.get(val);
		if (!key) {
			key = JSON.stringify(val);
			KeyMap.set(val, key);
		}
		return key;
	}
	return JSON.stringify(val);
}

// Compare two strings in UTF-8 order, which is the same as comparing the binary data as UTF-8
// This is important for the binary search to work in lua where strings are binary data
// Since UTF-8 preserves code point ordering, we can simply order by codepoints.
export function utf8Sort(a: string, b: string): number {
	const len = Math.min(a.length, b.length);
	for (let i = 0; i < len;) {
		const cpA = a.codePointAt(i) ?? 0;
		const cpB = b.codePointAt(i) ?? 0;
		if (cpA !== cpB) return cpA - cpB;
		// Jump by 2 for surrogate pairs.
		i += cpA > 0xffff ? 2 : 1;
	}
	return a.length - b.length;
}

function withData<T extends object>(data: Uint8Array, refs: Record<string, unknown>, fields: T):
	Readonly<T> & RxContext {
	Object.defineProperty(fields, "data", { value: data });
	Object.defineProperty(fields, "refs", { value: refs });
	return Object.freeze(fields) as Readonly<T> & RxContext;
}


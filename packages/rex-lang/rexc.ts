import { write } from "bun";

export interface RexCEncodeOptions {

	/** Encode in reverse mode (which enables streaming writers). */
	reverse?: boolean;

	/** Stream to callback instead of returning buffer. Works in both modes, but forward mode requires the receiver to reverse values. */
	onChunk?: (chunk: Uint8Array, offset: number) => void;

	/** Encode to allow random-access reads (length prefixes on containers). */
	randomAccess?: boolean;

	/** Enable path chains. */
	pathChains?: boolean;

	/** Enable pointers. */
	pointers?: boolean;

	/** Enable shared schemas. */
	schemas?: boolean;

	/**
	 * If an array or object has >= this many values, embed an index.
	 * `false` or `Infinity` to disable, `true` defaults to 10.
	 */
	indexes?: boolean | number;

	/** Extra refs after the builtins. */
	refs?: unknown[];
}

export interface RexCDecodeOptions {
	/** Indicates the input was encoded in reverse mode. */
	reverse?: boolean;

	/** Extra refs after the builtins. Must match exactly what the encoder used. */
	refs?: unknown[];

	/** When true, use Proxy to lazy decode properties on access. */
	lazy?: boolean;
}

const ENCODE_DEFAULTS = {
	reverse: false,
	randomAccess: true,
	pathChains: true,
	pointers: true,
	schemas: true,
	indexes: 10,
} as const satisfies Partial<RexCEncodeOptions>;

const DECODE_DEFAULTS = {
	reverse: false,
	lazy: true,
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
	if (num <= 0xFFFFFFFF) {
		return (num >>> 1) ^ -(num & 1);
	}
	// For larger numbers, we need to use arithmetic to avoid overflow issues.
	return (num % 2 === 0) ? (num / 2) : ((num + 1) / -2);
}

const b64Chars = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";

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

export const forwardEncoders = {
	string(tag: string, value: string) {
		if (!b64Regex.test(value)) {
			throw new TypeError(`String contains invalid characters for inline encoding: ${value}`);
		}
		return value + tag;
	},
	unsigned(tag: string, value: number) {
		if (value < 0) {
			throw new RangeError(`Value must be non-negative, got ${value}`);
		}
		return `${toB64(value)}${tag}`;
	},
	signed(tag: string, value: number) {
		return `${toB64(toZigZag(value))}${tag}`;
	}
}

export const reverseEncoders = {
	string(tag: string, value: string) {
		if (!b64Regex.test(value)) {
			throw new TypeError(`String contains invalid characters for inline encoding: ${value}`);
		}
		return tag + value;
	},
	unsigned(tag: string, value: number) {
		if (value < 0) {
			throw new RangeError(`Value must be non-negative, got ${value}`);
		}
		return `${tag}${toB64(value)}`;
	},
	signed(tag: string, value: number) {
		return `${tag}${toB64(toZigZag(value))}`;
	}
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
export function writeB64(buffer: Uint8Array, offset: number, value: number): number {
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

export function readB64(buffer: Uint8Array, offset: number, length: number): number {
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

export function stringify(value: unknown, options: RexCStringifyOptions & { onChunk: (chunk: string, offset: number) => void }): undefined;
export function stringify(value: unknown, options?: RexCStringifyOptions): string;
export function stringify(value: unknown, options?: RexCStringifyOptions): string | undefined {
	const { onChunk, ...rest } = options ?? {};
	if (onChunk) {
		encode(value, {
			...rest,
			onChunk: (chunk, offset) => onChunk(new TextDecoder().decode(chunk), offset),
		});
		return undefined;
	}
	return new TextDecoder().decode(encode(value, rest));
}

export function encode(value: unknown, options: RexCEncodeOptions & { onChunk: (chunk: Uint8Array, offset: number) => void }): undefined
export function encode(value: unknown, options?: RexCEncodeOptions): Uint8Array
export function encode(rootValue: unknown, options?: RexCEncodeOptions): Uint8Array | undefined {
	const opts = { ...ENCODE_DEFAULTS, ...options };
	const parts: Uint8Array[] = [];
	let byteLength = 0;
	const onChunk = opts.onChunk ?? (chunk => parts.push(chunk));
	const reverse = opts.reverse
	const randomAccess = opts.randomAccess
	const pointers = opts.pointers
	const schemas = opts.schemas
	const pathChains = opts.pathChains
	// Map from value identity to encoded offset, used for pointers
	const seenOffsets: Record<string, number> = {};
	// Map from schema identity to offset of either array of object with same shape
	const schemaOffsets: Record<string, number> = {};
	const seenCosts: Record<string, number> = {};
	const {
		string: writeStringPair,
		unsigned: writeUnsigned,
		signed: writeSigned
	} = reverse ? reverseEncoders : forwardEncoders;

	// Pre-scan the dataset to find reused path prefixes
	const duplicatePrefixes = new Set<string>();
	if (pathChains && pointers) {
		const seenPrefixes = new Set<string>();
		scanPrefixes(rootValue)
		function scanPrefixes(value: unknown) {
			if (typeof value === "string" && value[0] === "/") {
				let offset = 0
				if (!seenPrefixes.has(value)) {
					while (offset < value.length) {
						const nextSlash = value.indexOf("/", offset + 1);
						if (nextSlash === -1) break;
						const prefix = value.slice(0, nextSlash);
						if (seenPrefixes.has(prefix)) {
							duplicatePrefixes.add(prefix);
						} else {
							seenPrefixes.add(prefix);
						}
						offset = nextSlash;
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
	}

	writeAny(rootValue)

	// In streaming mode, there is no final buffer to return.
	if (opts.onChunk) return undefined;
	const output = new Uint8Array(byteLength);
	if (reverse) {
		// in reverse mode, we actually just flush the chunks in order.
		let offset = 0;
		for (const chunk of parts) {
			output.set(chunk, offset);
			offset += chunk.byteLength;
		}
	} else {
		// in forward mode, we need to write the chunks reversed.
		let offset = 0;
		for (let i = parts.length - 1; i >= 0; i--) {
			const chunk = parts[i] as Uint8Array;
			output.set(chunk, offset);
			offset += chunk.byteLength;
		}
	}
	return output;

	function pushBytes(bytes: Uint8Array) {
		onChunk(bytes, byteLength);
		return byteLength += bytes.byteLength;
	}

	function pushString(str: string) {
		const bytes = new TextEncoder().encode(str);
		return pushBytes(bytes);
	}

	function writeAny(value: unknown, needsSkippable = false) {
		if (!pointers) return writeAnyInner(value, needsSkippable);
		const key = JSON.stringify(value);
		const seenOffset = seenOffsets[key];
		if (seenOffset !== undefined) {
			const delta = byteLength - seenOffset;
			const seenCost = seenCosts[key] ?? 0
			const pointerCost = Math.ceil(Math.log(delta + 1) / Math.log(64)) + 1;
			if (pointerCost < seenCost) {
				return pushString(writeUnsigned("^", delta));
			}
		}

		const before = byteLength;
		const ret = writeAnyInner(value, needsSkippable);
		seenOffsets[key] = byteLength;
		seenCosts[key] = byteLength - before;
		return ret;
	}

	function writeAnyInner(value: unknown, needsSkippable: boolean) {
		switch (typeof value) {
			case "string": return writeString(value);
			case "number": return writeNumber(value);
			case "boolean": return pushString(writeStringPair("'", value ? 'tr' : 'fl'));
			case "undefined": return pushString(writeStringPair("'", 'un'));
			case "object":
				if (value === null) return pushString(writeStringPair("'", 'nl'));;
				if (Array.isArray(value)) return writeArray(value, needsSkippable);
				return writeObject(value as Record<string, unknown>, needsSkippable);
			default:
				throw new TypeError(`Unsupported value type: ${typeof value}`);
		}
	}

	function writeString(value: string) {
		if (b64Regex.test(value)) {
			return pushString(writeStringPair(':', value));
		}
		if (pathChains && value[0] === '/' && value.length > 1) {
			if (pointers) {
				if (value === "/") {
					// Special case for root path
					return pushString("/");
				}
				if (duplicatePrefixes.has(value) && value.lastIndexOf("/") === 0) {
					// TODO: allow this when the shorted prefix contains a slash
					const before = byteLength;
					writeAny(value.substring(1));
					const size = byteLength - before;
					return pushString(writeUnsigned('/', size));
				}
				// We need to write the string last-segments first, but only split when needed
				let offset = value.length;
				let head: string | undefined
				let tail: string | undefined
				while (offset > 0) {
					offset = value.lastIndexOf("/", offset - 1);
					if (offset <= 0) break;
					const prefix = value.slice(0, offset);
					if (duplicatePrefixes.has(prefix)) {
						// Grab head and tail 
						// (removing leading slashes since the pathChain format implies them between segments)
						head = prefix;
						tail = value.substring(offset + 1);
						break;
					}
				}
				if (head && tail) {
					const before = byteLength;
					writeAny(tail);
					writeAny(head);
					const size = byteLength - before;
					return pushString(writeUnsigned("/", size));
				}
			}
		}
		const utf8 = new TextEncoder().encode(value);
		pushBytes(utf8);
		return pushString(writeUnsigned(",", utf8.byteLength));
	}

	function writeNumber(value: number) {
		if (Number.isNaN(value)) {
			return pushString(writeStringPair("'", 'nan'));
		}
		if (value === Infinity) {
			return pushString(writeStringPair("'", 'inf'));
		}
		if (value === -Infinity) {
			return pushString(writeStringPair("'", 'nif'));
		}
		const [base, exp] = splitNumber(value);
		if (exp >= 0 && exp < 5 && Number.isInteger(base) && Number.isSafeInteger(base)) {
			return pushString(writeSigned("+", value));
		}
		pushString(writeSigned("+", base))
		return pushString(writeSigned("*", exp))
	}

	function writeArray(value: unknown[], needsSkippable = false) {
		pushString(reverse ? '[' : ']');
		const before = byteLength;
		for (let i = value.length - 1; i >= 0; i--) {
			writeAny(value[i], randomAccess);
		}
		const length = byteLength - before;
		const tag = reverse ? ']' : '['
		return pushString(needsSkippable ? writeUnsigned(tag, length) : tag);
	}

	function writeObject(value: Record<string, unknown>, needsSkippable = false) {
		pushString(reverse ? '{' : '}');
		const before = byteLength;
		let schemaTarget: number | undefined;
		let keys: string[] | undefined;
		let keysKey: string | undefined;
		if (schemas) {
			keys = Object.keys(value);
			keysKey = JSON.stringify(keys);
			schemaTarget = schemaOffsets[keysKey];
		}
		if (schemaTarget !== undefined) {
			const values = Object.values(value);
			for (let i = values.length - 1; i >= 0; i--) {
				writeAny(values[i], randomAccess);
			}
			pushString(writeUnsigned("^", byteLength - schemaTarget));
		} else {
			const entries = Object.entries(value);
			for (let i = entries.length - 1; i >= 0; i--) {
				const [key, val] = entries[i] as [string, unknown];
				writeAny(val, randomAccess);
				writeAny(key);
			}
		}
		const length = byteLength - before;
		const tag = reverse ? '}' : '{'
		const ret = pushString(needsSkippable ? writeUnsigned(tag, length) : tag);
		if (schemas && keysKey && schemaTarget === undefined) {
			schemaOffsets[keysKey] = byteLength
		}
		return ret
	}

}

export function decode(input: Uint8Array, options?: RexCDecodeOptions): unknown {
	const opts = { ...DECODE_DEFAULTS, ...options };
	throw new Error("TODO: implement parse");
}

export function parse(input: string, options?: RexCDecodeOptions): unknown {
	return decode(new TextEncoder().encode(input), options);
}

// Input is an integer string.
// returns base and number of zeroes that were trimmed
function trimZeroes(str: string): [number, number] {
	const trimmed = str.replace(/0+$/, "")
	const zeroCount = str.length - trimmed.length
	return [parseInt(trimmed, 10), zeroCount]
}

// Given a double value, split it into a base and power of 10.
// For example, 1234.5678 would be split into 12345678 and -4.
export function splitNumber(val: number): [number, number] {
	if (Number.isInteger(val)) {
		if (Math.abs(val) < 10) {
			return [val, 0]
		}
		if (Math.abs(val) < 9.999999999999999e20) {
			return trimZeroes(val.toString())
		}
	}
	// Try decimal representation first
	const decStr = val.toPrecision(14).match(/^([-+]?\d+)(?:\.(\d+))?$/)
	if (decStr) {
		const b1 = parseInt((decStr[1] ?? "") + (decStr[2] ?? ""), 10)
		const e1 = -(decStr[2]?.length ?? 0)
		if (e1 === 0) {
			return [b1, 0]
		}
		const [b2, e2] = splitNumber(b1)
		return [b2, e1 + e2]
	}
	// Then try scientific notation
	const sciStr = val
		.toExponential(14)
		.match(/^([+-]?\d+)(?:\.(\d+))?(?:e([+-]?\d+))$/)
	if (sciStr) {
		// Count the decimal places
		const e1 = -(sciStr[2]?.length ?? 0)
		// Parse the exponent
		const e2 = parseInt(sciStr[3] ?? "0", 10)
		// Parse left of e as integer with zeroes trimmed
		const [b1, e3] = trimZeroes(sciStr[1] + (sciStr[2] ?? ""))
		return [b1, e1 + e2 + e3]
	}
	throw new Error(`Invalid number format: ${val}`)
}

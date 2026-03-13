/////////////////////
//
// Cursor-based rexc parser with zero-allocation reads and Proxy wrapper
//
//////////////////

import { is as isB64, read as b64Read, decodeTable as b64Decode, encodeTable as b64Encode } from "./b64";
import { fromZigZag } from "./rexc";

// ── Tags ──

export type Tag =
  | "int"
  | "float"
  | "str"
  | "ref"
  | "true"
  | "false"
  | "null"
  | "undef"
  | "array"
  | "object"
  | "ptr"
  | "chain";

// ── Cursor ──

export interface Cursor {
  data: Uint8Array;
  left: number;
  right: number;
  tag: Tag;
  val: number;
  ixWidth: number;
  ixCount: number;
  schema: number;
}

export function makeCursor(data: Uint8Array): Cursor {
  return {
    data,
    left: 0,
    right: data.length,
    tag: "null",
    val: 0,
    ixWidth: 0,
    ixCount: 0,
    schema: 0,
  };
}

// Internal scratch cursors — reused across calls to avoid allocations.
// Safe because JS is single-threaded and these functions don't re-enter each other.
const _empty = new Uint8Array(0);
const _k: Cursor = makeCursor(_empty); // key/temp cursor
const _s: Cursor = makeCursor(_empty); // schema cursor

// ── Core parsing ──

// Scan left from c.right past b64 digits. Sets c.left to the tag position.
// Returns the tag byte. b64 digits are at data[c.left+1 .. c.right).
function peekTag(c: Cursor): number {
  const { data } = c;
  let offset = c.right;
  while (--offset >= 0 && isB64(data[offset]!));
  if (offset < 0) throw new SyntaxError("peekTag: no tag found");
  c.left = offset;
  return data[offset]!;
}

// Unpack index metadata into cursor: low 3 bits = width-1, rest = count
function unpackIndex(c: Cursor, data: Uint8Array, left: number, right: number): void {
  const packed = b64Read(data, left, right);
  c.ixWidth = (packed & 0b111) + 1;
  c.ixCount = packed >> 3;
}

/** Read one node ending at c.right. Fills all cursor fields. Returns the tag. */
export function read(c: Cursor): Tag {
  const { data } = c;
  let { right } = c;

  // Reset container fields
  c.ixWidth = 0;
  c.ixCount = 0;
  c.schema = 0;

  // Find the tag: peekTag sets c.left to tag position
  const tag = peekTag(c);
  let { left } = c;

  if (tag === 0x27) {
    // ' — ref or builtin
    // Name bytes are at data[left+1..right), b64 digits overlap with name
    const nameLen = right - left - 1;
    // Check builtins by length + first byte
    if (nameLen === 1) {
      const ch = data[left + 1]!;
      if (ch === 0x74) { c.tag = "true"; c.val = 0; return c.tag; }  // t
      if (ch === 0x66) { c.tag = "false"; c.val = 0; return c.tag; } // f
      if (ch === 0x6e) { c.tag = "null"; c.val = 0; return c.tag; }  // n
      if (ch === 0x75) { c.tag = "undef"; c.val = 0; return c.tag; } // u
    } else if (nameLen === 3) {
      const a = data[left + 1]!, b = data[left + 2]!, d = data[left + 3]!;
      if (a === 0x69 && b === 0x6e && d === 0x66) { c.tag = "float"; c.val = Infinity; return c.tag; }   // inf
      if (a === 0x6e && b === 0x69 && d === 0x66) { c.tag = "float"; c.val = -Infinity; return c.tag; }  // nif
      if (a === 0x6e && b === 0x61 && d === 0x6e) { c.tag = "float"; c.val = NaN; return c.tag; }        // nan
    }
    c.val = nameLen;
    return c.tag = "ref";
  }

  const b64 = b64Read(data, left + 1, right);

  switch (tag) {
    case 0x2c: // , — string (most common)
      c.left = left - b64;
      c.val = b64;
      return c.tag = "str";

    case 0x2b: // + — integer
      c.val = fromZigZag(b64);
      return c.tag = "int";

    case 0x2a: { // * — float (exponent)
      const exp = fromZigZag(b64);
      const savedRight = c.right;
      c.right = left;
      read(c);
      c.val = parseFloat(`${c.val}e${exp}`);
      c.right = savedRight;
      return c.tag = "float";
    }

    case 0x3a: { // : — object
      let content = left;
      c.left = left - b64;
      // Parse optional schema (rightmost), then optional index
      if (content > c.left) {
        _k.data = data;
        _k.right = content;
        let innerTag = peekTag(_k);
        // Schema: ' (ref) or ^ (pointer to container)
        if (innerTag === 0x27 || innerTag === 0x5e) {
          let isSchema = true;
          if (innerTag === 0x5e) {
            const target = _k.left - b64Read(data, _k.left + 1, content);
            _s.data = data;
            _s.right = target;
            const targetTag = peekTag(_s);
            isSchema = targetTag === 0x3b || targetTag === 0x3a;
          }
          if (isSchema) {
            c.schema = content;
            content = _k.left;
          }
        }
        // Index: #
        if (content > c.left) {
          _k.right = content;
          innerTag = peekTag(_k);
          if (innerTag === 0x23) {
            unpackIndex(c, data, _k.left + 1, content);
            content = _k.left - c.ixWidth * c.ixCount;
          }
        }
      }
      c.val = content;
      return c.tag = "object";
    }

    case 0x3b: { // ; — array
      let content = left;
      c.left = left - b64;
      // Check for index
      if (content > c.left) {
        _k.data = data;
        _k.right = content;
        const ixTag = peekTag(_k);
        if (ixTag === 0x23) { // #
          unpackIndex(c, data, _k.left + 1, content);
          content = _k.left - c.ixWidth * c.ixCount;
        }
      }
      c.val = content;
      return c.tag = "array";
    }

    case 0x5e: // ^ — pointer
      c.val = left - b64;
      return c.tag = "ptr";

    case 0x2e: // . — chain
      c.left = left - b64;
      c.val = left;
      return c.tag = "chain";

    default:
      throw new SyntaxError(`Unknown tag: ${String.fromCharCode(tag)}`);
  }
}

// ── String handling ──

// String body start offset. For "str": body is at [left, left+val).
// For "ref": name is at [left+1, left+1+val) (skip the ' tag byte).
function strStart(c: Cursor): number {
  return c.left + (c.tag === "ref" ? 1 : 0);
}

// Shared TextDecoder for readStr
const decoder = new TextDecoder();

/** Decode the string at cursor position to a JS string. 1 allocation. */
export function readStr(c: Cursor): string {
  const start = strStart(c);
  return decoder.decode(c.data.subarray(start, start + c.val));
}

/** Resolve a node to a string, following pointers and concatenating chains.
 *  For plain "str" nodes this is just readStr. */
export function resolveStr(c: Cursor): string {
  while (c.tag === "ptr") { c.right = c.val; read(c); }
  if (c.tag === "str") return readStr(c);
  if (c.tag === "chain") {
    // Save chain boundaries before iterating (read() overwrites c.left)
    const parts: string[] = [];
    let right = c.val;
    const left = c.left;
    while (right > left) {
      c.right = right;
      read(c);
      right = c.left;
      parts.push(resolveStr(c));
    }
    return parts.join("");
  }
  throw new TypeError(`resolveStr: expected str, ptr, or chain, got ${c.tag}`);
}

// Shared TextEncoder for strEquals/strCompare
const encoder = new TextEncoder();

/** Zero-alloc equality check: does cursor's string match target? */
export function strEquals(c: Cursor, target: string): boolean {
  const start = strStart(c);
  const { val: byteLen, data } = c;
  let ti = 0;
  for (let i = start, end = start + byteLen; i < end; i++) {
    if (ti >= target.length) return false;
    const cp = target.codePointAt(ti)!;
    if (cp < 0x80) {
      if (data[i] !== cp) return false;
      ti++;
    } else if (cp < 0x800) {
      if (end - i < 2) return false;
      if (data[i] !== (0xc0 | (cp >> 6))) return false;
      if (data[++i] !== (0x80 | (cp & 0x3f))) return false;
      ti++;
    } else if (cp < 0x10000) {
      if (end - i < 3) return false;
      if (data[i] !== (0xe0 | (cp >> 12))) return false;
      if (data[++i] !== (0x80 | ((cp >> 6) & 0x3f))) return false;
      if (data[++i] !== (0x80 | (cp & 0x3f))) return false;
      ti++;
    } else {
      if (end - i < 4) return false;
      if (data[i] !== (0xf0 | (cp >> 18))) return false;
      if (data[++i] !== (0x80 | ((cp >> 12) & 0x3f))) return false;
      if (data[++i] !== (0x80 | ((cp >> 6) & 0x3f))) return false;
      if (data[++i] !== (0x80 | (cp & 0x3f))) return false;
      ti += cp > 0xffff ? 2 : 1; // surrogate pair in JS string
    }
  }
  return ti >= target.length;
}

/** Compare cursor's string against target. Returns <0, 0, or >0. Allocates 1 Uint8Array for target encoding. */
export function strCompare(c: Cursor, target: string): number {
  const start = strStart(c);
  const { val: byteLen, data } = c;
  // Encode target to UTF-8 for byte comparison (1 allocation — unavoidable for ordering)
  const targetBytes = encoder.encode(target);
  const len = Math.min(byteLen, targetBytes.length);
  for (let i = 0; i < len; i++) {
    const diff = data[start + i]! - targetBytes[i]!;
    if (diff !== 0) return diff;
  }
  return byteLen - targetBytes.length;
}

// ── Container access ──

/** Jump to the Nth child of an indexed container. O(1). Reads the child into c. */
export function seekChild(c: Cursor, container: Cursor, index: number): void {
  if (container.ixWidth === 0) {
    throw new Error("seekChild requires an indexed container");
  }
  if (index < 0 || index >= container.ixCount) {
    throw new RangeError(`seekChild: index ${index} out of range [0, ${container.ixCount})`);
  }
  const { data } = container;
  // Layout: [content] [ix entry 0..N-1] [# packed] [tag b64size]
  // container.val = content boundary = start of index table
  // Each entry is a b64 delta relative to container.val
  // child_right = container.val - delta
  const { val: ixBase, ixWidth } = container;
  const entryLeft = ixBase + index * ixWidth;
  const delta = b64Read(data, entryLeft, entryLeft + ixWidth);
  c.data = data;
  c.right = ixBase - delta;
  read(c);
}

/** Collect child right-boundaries into caller-owned array (logical order). Returns count. */
export function collectChildren(container: Cursor, offsets: number[]): number {
  _k.data = container.data;
  let right = container.val;
  const end = container.left;
  let count = 0;
  while (right > end) {
    if (count >= offsets.length) offsets.push(right);
    else offsets[count] = right;
    count++;
    _k.right = right;
    read(_k);
    right = _k.left;
  }
  return count;
}

// Compare a key node (in _k) against target string.
// Handles str (zero-alloc), ptr→str (zero-alloc), chain (allocates).
function keyEquals(target: string): boolean {
  // Resolve pointers
  while (_k.tag === "ptr") { _k.right = _k.val; read(_k); }
  // Fast path: plain string — zero-alloc comparison
  if (_k.tag === "str") return strEquals(_k, target);
  // Slow path: chain — must allocate to concatenate
  if (_k.tag === "chain") return resolveStr(_k) === target;
  return false;
}

/** Find a key in an object. Fills c with the value node if found. */
export function findKey(c: Cursor, container: Cursor, target: string): boolean {
  if (container.tag !== "object") return false;

  const { data } = container;
  _k.data = data;

  // TODO: sorted + indexed binary search path
  // For now: linear scan through key/value pairs
  let right = container.val;
  const end = container.left;

  if (container.schema !== 0) {
    // Schema object: content has only values, keys come from schema
    _s.data = data;
    _s.right = container.schema;
    read(_s);

    if (_s.tag === "ptr") {
      _s.right = _s.val;
      read(_s);
    }

    let keyRight = _s.val;
    const keyEnd = _s.left;
    let valRight = container.val;

    if (_s.tag === "object") {
      // Schema is an object — keys are its keys.
      // Read key into _k, check match, then skip schema value using _s.
      while (keyRight > keyEnd && valRight > end) {
        _k.right = keyRight;
        read(_k);
        const keyLeft = _k.left; // save before keyEquals may follow pointers/chains
        const matched = keyEquals(target);
        // Skip schema value using _s
        _s.data = data;
        _s.right = keyLeft;
        read(_s);
        keyRight = _s.left;

        if (matched) {
          c.data = data;
          c.right = valRight;
          read(c);
          return true;
        }

        c.data = data;
        c.right = valRight;
        read(c);
        valRight = c.left;
      }
    }

    if (_s.tag === "array") {
      while (keyRight > keyEnd && valRight > end) {
        _k.right = keyRight;
        read(_k);
        keyRight = _k.left;

        if (keyEquals(target)) {
          c.data = data;
          c.right = valRight;
          read(c);
          return true;
        }

        c.data = data;
        c.right = valRight;
        read(c);
        valRight = c.left;
      }
    }

    return false;
  }

  // No schema: interleaved key/value pairs
  while (right > end) {
    _k.right = right;
    read(_k);
    const keyLeft = _k.left; // save before keyEquals may follow pointers/chains
    if (keyEquals(target)) {
      c.data = data;
      c.right = keyLeft;
      read(c);
      return true;
    }
    // Skip value
    c.data = data;
    c.right = keyLeft;
    read(c);
    right = c.left;
  }
  return false;
}

// ── Raw bytes ──

/** Zero-copy view of the raw rexc bytes for the node at cursor position. */
export function rawBytes(c: Cursor): Uint8Array {
  return c.data.subarray(c.left, c.right);
}

// ── High-level Proxy API ──

export type Refs = Record<string, Uint8Array>;

const HANDLE = Symbol("rexc.handle");

/** Open a rexc buffer and return a Proxy-wrapped root value. */
export function open(buffer: Uint8Array, refs?: Refs): unknown {
  throw new Error("TODO: implement open");
}

/** Get the raw handle from a Proxy-wrapped value (escape hatch). */
export function handle(proxy: unknown): { data: Uint8Array; right: number } | undefined {
  if (proxy && typeof proxy === "object" && HANDLE in proxy) {
    return (proxy as any)[HANDLE];
  }
  return undefined;
}

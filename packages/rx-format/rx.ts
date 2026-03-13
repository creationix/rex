/////////////////////
//
// Cursor-based rexc parser with zero-allocation reads and Proxy wrapper
//
//////////////////

import { is as isB64, read as b64Read, decodeTable as b64Decode, encodeTable as b64Encode } from "./b64";
import { fromZigZag } from "@creationix/rex/rexc";

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
const _cc: Cursor = makeCursor(_empty); // collectChildren cursor (separate from _k to avoid conflict with read())

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

// Shared TextEncoder for encoding targets
const encoder = new TextEncoder();

/** Encode a string to UTF-8 bytes for use with strEquals/strCompare. */
export function prepareKey(target: string): Uint8Array {
  return encoder.encode(target);
}

/**
 * Compare a node's string bytes against key bytes starting at offset.
 * Handles str, ptr, and chain (zero-alloc for all).
 * Returns { cmp, offset } where cmp is <0, 0, or >0 for the first difference,
 * NaN if the node is not a string type, and offset is how far into the key bytes.
 */
function nodeCompare(c: Cursor, key: Uint8Array, offset: number): { cmp: number; offset: number } {
  while (c.tag === "ptr") { c.right = c.val; read(c); }

  if (c.tag === "str" || c.tag === "ref") {
    const start = strStart(c);
    const byteLen = c.val;
    const { data } = c;
    const len = Math.min(byteLen, key.length - offset);
    for (let i = 0; i < len; i++) {
      const diff = data[start + i]! - key[offset + i]!;
      if (diff !== 0) return { cmp: diff, offset: offset + i };
    }
    if (byteLen > key.length - offset) return { cmp: 1, offset: key.length };
    return { cmp: 0, offset: offset + byteLen };
  }

  if (c.tag === "chain") {
    let right = c.val;
    const left = c.left;
    while (right > left) {
      c.right = right;
      read(c);
      right = c.left;
      const result = nodeCompare(c, key, offset);
      if (result.cmp !== 0) return result;
      offset = result.offset;
    }
    return { cmp: 0, offset };
  }

  return { cmp: NaN, offset };
}

/** Compare cursor's string against target. Returns <0, 0, >0, or NaN if not a string node. */
export function strCompare(c: Cursor, target: Uint8Array): number {
  const { cmp, offset } = nodeCompare(c, target, 0);
  if (cmp !== 0) return cmp;
  return offset < target.length ? -1 : 0;
}

/** Zero-alloc equality check: does cursor's string match target? */
export function strEquals(c: Cursor, target: Uint8Array): boolean {
  return strCompare(c, target) === 0;
}

/** Zero-alloc prefix check: does cursor's string start with prefix? */
export function strHasPrefix(c: Cursor, prefix: Uint8Array): boolean {
  if (prefix.length === 0) return true;
  const { offset } = nodeCompare(c, prefix, 0);
  return offset === prefix.length;
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
  // Uses _cc instead of _k because read() internally uses _k for object
  // schema/index detection — calling read(_k) on an object node would self-conflict.
  _cc.data = container.data;
  let right = container.val;
  const end = container.left;
  let count = 0;
  while (right > end) {
    if (count >= offsets.length) offsets.push(right);
    else offsets[count] = right;
    count++;
    _cc.right = right;
    read(_cc);
    right = _cc.left;
  }
  return count;
}

// Compare a key node (in _k) against target. Zero-alloc for str, ptr, and chain.
function keyEquals(target: Uint8Array): boolean {
  return strEquals(_k, target);
}

/** Find a key in an object. Fills c with the value node if found. */
export function findKey(c: Cursor, container: Cursor, target: string | Uint8Array): boolean {
  if (container.tag !== "object") return false;
  if (typeof target === "string") target = prepareKey(target);

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

/**
 * Find all keys matching a prefix in an object.
 * On indexed objects: O(log n) binary search + O(m) iteration over matches.
 * On non-indexed objects: O(n) linear scan.
 * Calls visitor(keyCursor, valueCursor) for each match — use resolveStr(key)
 * only if you need the string. Stops if visitor returns false.
 */
export function findByPrefix(
  c: Cursor,
  container: Cursor,
  prefix: string | Uint8Array,
  visitor: (key: Cursor, value: Cursor) => boolean | void,
): void {
  if (container.tag !== "object") return;
  if (typeof prefix === "string") prefix = prepareKey(prefix);

  const { data } = container;

  // TODO: schema-based objects
  if (container.schema !== 0) return;

  if (container.ixWidth > 0 && container.ixCount > 0) {
    // Binary search: index entries are sorted and point to keys
    let lo = 0, hi = container.ixCount;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      seekChild(c, container, mid);
      const cmp = strCompare(c, prefix);
      if (cmp < 0) lo = mid + 1;
      else hi = mid;
    }
    // lo is the first key >= prefix. Iterate while prefix matches.
    for (let i = lo; i < container.ixCount; i++) {
      seekChild(c, container, i);
      const keyRight = c.right;
      const keyLeft = c.left;
      if (!strHasPrefix(c, prefix)) break;
      // Re-read key into _cc (safe from read() internal _k usage)
      _cc.data = data; _cc.right = keyRight; read(_cc);
      // Read value (immediately after key)
      c.data = data; c.right = keyLeft; read(c);
      if (visitor(_cc, c) === false) return;
    }
    return;
  }

  // Non-indexed: linear scan
  _k.data = data;
  let right = container.val;
  const end = container.left;
  while (right > end) {
    _k.right = right;
    read(_k);
    const keyLeft = _k.left;
    const keyRight = right;
    if (strHasPrefix(_k, prefix)) {
      // Re-read key into _cc (safe from read() internal _k usage)
      _cc.data = data; _cc.right = keyRight; read(_cc);
      c.data = data; c.right = keyLeft; read(c);
      if (visitor(_cc, c) === false) return;
    } else {
      c.data = data; c.right = keyLeft; read(c);
    }
    right = c.left;
  }
}

// ── Raw bytes ──

/** Zero-copy view of the raw rexc bytes for the node at cursor position. */
export function rawBytes(c: Cursor): Uint8Array {
  return c.data.subarray(c.left, c.right);
}

// ── High-level Proxy API ──

export type Refs = Record<string, Uint8Array>;

const HANDLE = Symbol("rexc.handle");

type NodeInfo = {
  data: Uint8Array;
  right: number;
  tag: Tag;
  val: number;
  left: number;
  ixWidth: number;
  ixCount: number;
  schema: number;
  _count?: number;
  _offsets?: number[];
  _keys?: string[];
  _keyMap?: Map<string, number>; // key → value right-offset, built by ensureKeyMap
};

/** Open a rexc buffer and return a Proxy-wrapped root value. */
export function open(buffer: Uint8Array, refs?: Refs): unknown {
  const nodeMap = new WeakMap<object, NodeInfo>();
  const proxyCache = new Map<number, unknown>(); // right-offset → memoized value
  const refCaches = refs ? new Map<Uint8Array, Map<number, unknown>>() : undefined;
  const scratch = makeCursor(buffer);

  function snap(c: Cursor): NodeInfo {
    return {
      data: c.data, right: c.right, tag: c.tag, val: c.val,
      left: c.left, ixWidth: c.ixWidth, ixCount: c.ixCount, schema: c.schema,
    };
  }

  function getCache(data: Uint8Array): Map<number, unknown> {
    if (data === buffer) return proxyCache;
    let cache = refCaches!.get(data);
    if (!cache) { cache = new Map(); refCaches!.set(data, cache); }
    return cache;
  }

  function wrap(c: Cursor): unknown {
    while (c.tag === "ptr") { c.right = c.val; read(c); }
    if (c.tag === "ref") {
      if (!refs) return undefined;
      const refBuf = refs[readStr(c)];
      if (!refBuf) return undefined;
      c.data = refBuf; c.right = refBuf.length; read(c);
      return wrap(c);
    }
    // Check cache for containers (primitives are cheap to recreate)
    const cache = getCache(c.data);
    const cached = cache.get(c.right);
    if (cached !== undefined) return cached;
    switch (c.tag) {
      case "int": case "float": return c.val;
      case "str": return readStr(c);
      case "chain": return resolveStr(c);
      case "true": return true;
      case "false": return false;
      case "null": return null;
      case "undef": return undefined;
    }
    const info = snap(c);
    const target: object = c.tag === "array" ? [] : Object.create(null);
    nodeMap.set(target, info);
    const proxy = new Proxy(target, handler);
    cache.set(c.right, proxy);
    return proxy;
  }

  function childCount(info: NodeInfo): number {
    if (info._count !== undefined) return info._count;
    if (info.ixCount > 0) return info._count = info.ixCount;
    if (info.tag === "array") {
      ensureOffsets(info);
      return info._count!;
    }
    // Object without index — scan children
    let right = info.val, n = 0;
    while (right > info.left) {
      scratch.data = info.data; scratch.right = right;
      read(scratch); right = scratch.left; n++;
    }
    return info._count = info.schema !== 0 ? n : n / 2;
  }

  function ensureOffsets(info: NodeInfo): number[] {
    if (!info._offsets) {
      info._offsets = [];
      info._count = collectChildren(info as unknown as Cursor, info._offsets);
    }
    return info._offsets;
  }

  function getChild(info: NodeInfo, index: number): unknown {
    if (index < 0 || index >= childCount(info)) return undefined;
    if (info.ixWidth > 0) {
      seekChild(scratch, info as unknown as Cursor, index);
      return wrap(scratch);
    }
    const offsets = ensureOffsets(info);
    scratch.data = info.data;
    scratch.right = offsets[index]!;
    read(scratch);
    return wrap(scratch);
  }

  function getValue(info: NodeInfo, key: string): unknown {
    // Use cached key map if available (built by enumKeys/ownKeys)
    if (info._keyMap) {
      const valRight = info._keyMap.get(key);
      if (valRight === undefined) return undefined;
      scratch.data = info.data;
      scratch.right = valRight;
      read(scratch);
      return wrap(scratch);
    }
    scratch.data = info.data;
    if (findKey(scratch, info as unknown as Cursor, key)) return wrap(scratch);
    return undefined;
  }

  function ensureKeyMap(info: NodeInfo): { keys: string[]; map: Map<string, number> } {
    if (info._keyMap) {
      return { keys: info._keys!, map: info._keyMap };
    }
    const keys: string[] = [];
    const map = new Map<string, number>();
    const kc = makeCursor(info.data);
    if (info.schema !== 0) {
      const sc = makeCursor(info.data);
      sc.right = info.schema; read(sc);
      while (sc.tag === "ptr") { sc.right = sc.val; read(sc); }
      let valRight = info.val;
      if (sc.tag === "object") {
        let keyRight = sc.val;
        const keyEnd = sc.left;
        while (keyRight > keyEnd) {
          kc.right = keyRight; read(kc);
          const nextRight = kc.left;
          const name = resolveStr(kc);
          keys.push(name);
          map.set(name, valRight);
          // advance value cursor
          scratch.data = info.data; scratch.right = valRight; read(scratch);
          valRight = scratch.left;
          // skip schema value
          sc.right = nextRight; read(sc);
          keyRight = sc.left;
        }
      } else if (sc.tag === "array") {
        let keyRight = sc.val;
        const keyEnd = sc.left;
        while (keyRight > keyEnd) {
          kc.right = keyRight; read(kc);
          const name = resolveStr(kc);
          keys.push(name);
          map.set(name, valRight);
          // advance value cursor
          scratch.data = info.data; scratch.right = valRight; read(scratch);
          valRight = scratch.left;
          keyRight = kc.left;
        }
      }
    } else {
      let right = info.val;
      while (right > info.left) {
        kc.right = right; read(kc);
        const keyLeft = kc.left;
        const name = resolveStr(kc);
        keys.push(name);
        map.set(name, keyLeft);
        // skip value
        kc.right = keyLeft; read(kc);
        right = kc.left;
      }
    }
    info._keys = keys;
    info._keyMap = map;
    return { keys, map };
  }

  const handler: ProxyHandler<object> = {
    get(target, prop) {
      const info = nodeMap.get(target)!;
      if (prop === HANDLE) return { data: info.data, right: info.right };

      if (prop === Symbol.iterator) {
        if (info.tag === "array") {
          return function*() {
            const n = childCount(info);
            for (let i = 0; i < n; i++) yield getChild(info, i);
          };
        }
        if (info.tag === "object") {
          return function*() {
            const ks = ensureKeyMap(info).keys;
            for (const k of ks) yield [k, getValue(info, k)] as [string, unknown];
          };
        }
        return undefined;
      }

      if (typeof prop === "symbol") return undefined;
      if (prop === "length") return childCount(info);

      if (info.tag === "array") {
        const idx = Number(prop);
        if (Number.isInteger(idx) && idx >= 0) return getChild(info, idx);
        // Delegate Array.prototype methods to a materialized snapshot
        const method = (Array.prototype as any)[prop];
        if (typeof method === "function") {
          return function(...args: unknown[]) {
            const n = childCount(info);
            const arr: unknown[] = new Array(n);
            for (let i = 0; i < n; i++) arr[i] = getChild(info, i);
            return method.apply(arr, args);
          };
        }
        return undefined;
      }

      if (info.tag === "object") return getValue(info, prop);
      return undefined;
    },

    has(target, prop) {
      const info = nodeMap.get(target)!;
      if (prop === HANDLE) return true;
      if (typeof prop === "symbol") return false;
      if (prop === "length") return true;
      if (info.tag === "array") {
        const idx = Number(prop);
        return Number.isInteger(idx) && idx >= 0 && idx < childCount(info);
      }
      if (info.tag === "object") {
        if (info._keyMap) return info._keyMap.has(prop);
        scratch.data = info.data;
        return findKey(scratch, info as unknown as Cursor, prop);
      }
      return false;
    },

    ownKeys(target) {
      const info = nodeMap.get(target)!;
      if (info.tag === "array") {
        const n = childCount(info);
        const ks: string[] = [];
        for (let i = 0; i < n; i++) ks.push(String(i));
        ks.push("length");
        return ks;
      }
      return ensureKeyMap(info).keys;
    },

    getOwnPropertyDescriptor(target, prop) {
      const info = nodeMap.get(target)!;
      if (info.tag === "array") {
        if (prop === "length") {
          return { configurable: true, enumerable: false, value: childCount(info), writable: false };
        }
        const idx = Number(prop);
        if (typeof prop === "string" && Number.isInteger(idx) && idx >= 0 && idx < childCount(info)) {
          return { configurable: true, enumerable: true, value: getChild(info, idx) };
        }
        return undefined;
      }
      if (info.tag === "object" && typeof prop === "string") {
        if (info._keyMap) {
          if (info._keyMap.has(prop)) {
            return { configurable: true, enumerable: true, value: getValue(info, prop) };
          }
        } else {
          scratch.data = info.data;
          if (findKey(scratch, info as unknown as Cursor, prop)) {
            return { configurable: true, enumerable: true, value: wrap(scratch) };
          }
        }
      }
      return undefined;
    },

    set() { throw new TypeError("rexc data is read-only"); },
    deleteProperty() { throw new TypeError("rexc data is read-only"); },
  };

  // Read and wrap root
  scratch.right = buffer.length;
  read(scratch);
  return wrap(scratch);
}

/** Get the raw handle from a Proxy-wrapped value (escape hatch). */
export function handle(proxy: unknown): { data: Uint8Array; right: number } | undefined {
  if (proxy && typeof proxy === "object" && HANDLE in proxy) {
    return (proxy as any)[HANDLE];
  }
  return undefined;
}

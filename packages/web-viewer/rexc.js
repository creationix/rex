// ../rex-lang/rexc.ts
var BUILTIN_REFS = {
  n: null,
  t: true,
  f: false,
  u: undefined,
  nan: NaN,
  inf: Infinity,
  nif: -Infinity
};
var ENCODE_DEFAULTS = {
  pretty: false,
  bareStrings: true,
  randomAccess: true,
  pointers: true,
  schemas: true,
  pathChains: true,
  indexThreshold: 10,
  reverse: false,
  refs: {}
};
var DECODE_DEFAULTS = {
  reverse: false,
  lazy: true,
  refs: {}
};
function toZigZag(num) {
  if (num >= -2147483648 && num <= 2147483647) {
    return num << 1 ^ num >> 31;
  }
  return num < 0 ? num * -2 - 1 : num * 2;
}
function fromZigZag(num) {
  if (num <= 4294967295) {
    return num >>> 1 ^ -(num & 1);
  }
  return num % 2 === 0 ? num / 2 : (num + 1) / -2;
}
var b64Chars = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";
var b64Lookup = new Uint8Array(128).fill(255);
var b64Codes = new Uint8Array(64);
for (let i = 0;i < 64; i++) {
  const code = b64Chars.charCodeAt(i);
  b64Lookup[code] = i;
  b64Codes[i] = code;
}
function toB64Signed(num) {
  return toB64(toZigZag(num));
}
function fromB64Signed(str) {
  return fromZigZag(fromB64(str));
}
var b64Regex = /^[0-9a-zA-Z\-_]*$/;
var forwardEncoders = {
  string(tag, value) {
    if (!b64Regex.test(value)) {
      throw new TypeError(`String contains invalid characters for inline encoding: ${value}`);
    }
    return value + tag;
  },
  unsigned(tag, value) {
    if (value < 0) {
      throw new RangeError(`Value must be non-negative, got ${value}`);
    }
    return `${toB64(value)}${tag}`;
  },
  signed(tag, value) {
    return `${toB64(toZigZag(value))}${tag}`;
  }
};
var reverseEncoders = {
  string(tag, value) {
    if (!b64Regex.test(value)) {
      throw new TypeError(`String contains invalid characters for inline encoding: ${value}`);
    }
    return tag + value;
  },
  unsigned(tag, value) {
    if (value < 0) {
      throw new RangeError(`Value must be non-negative, got ${value}`);
    }
    return `${tag}${toB64(value)}`;
  },
  signed(tag, value) {
    return `${tag}${toB64(toZigZag(value))}`;
  }
};
function toB64(num) {
  let result = "";
  while (num > 0) {
    result = b64Chars[num % 64] + result;
    num = Math.floor(num / 64);
  }
  return result;
}
function fromB64(str) {
  let result = 0;
  for (let i = 0;i < str.length; i++) {
    const value = b64Lookup[str.charCodeAt(i)];
    if (value === 255) {
      throw new Error(`Invalid base64 character: ${str[i]}`);
    }
    result = result * 64 + value;
  }
  return result;
}
var b64Scratch = new Uint8Array(10);
function writeB64(buffer, offset, value) {
  if (value === 0)
    return offset;
  let len = 0;
  while (value > 0) {
    b64Scratch[len++] = b64Codes[value % 64];
    value = Math.floor(value / 64);
  }
  for (let i = len - 1;i >= 0; i--) {
    buffer[offset++] = b64Scratch[i];
  }
  return offset;
}
function readB64(buffer, offset, length) {
  let result = 0;
  for (let i = 0;i < length; i++) {
    const code = buffer[offset + i];
    const value = b64Lookup[code];
    if (value === 255) {
      throw new Error(`Invalid base64 character: ${String.fromCharCode(code)}`);
    }
    result = result * 64 + value;
  }
  return result;
}
function stringify(value, options) {
  const { onChunk, ...rest } = options ?? {};
  if (onChunk) {
    encode(value, {
      ...rest,
      onChunk: (chunk, offset) => onChunk(new TextDecoder().decode(chunk), offset)
    });
    return;
  }
  return new TextDecoder().decode(encode(value, rest));
}
function encode(rootValue, options) {
  const opts = { ...ENCODE_DEFAULTS, ...options };
  const parts = [];
  let byteLength = 0;
  const onChunk = opts.onChunk ?? ((chunk) => parts.push(chunk));
  const reverse = opts.reverse;
  const randomAccess = opts.randomAccess;
  const pointers = opts.pointers;
  const schemas = opts.schemas;
  const pathChains = opts.pathChains;
  const bareStrings = opts.bareStrings;
  const refs = Object.fromEntries(Object.entries({ ...opts.refs }).map(([key, val]) => [makeKey(val), key]));
  const pretty = opts.pretty;
  let indentLevel = 0;
  const seenOffsets = {};
  const schemaOffsets = {};
  const seenCosts = {};
  const {
    string: writeStringPair,
    unsigned: writeUnsigned,
    signed: writeSigned
  } = reverse ? reverseEncoders : forwardEncoders;
  const duplicatePrefixes = new Set;
  if (pathChains && pointers) {
    let scanPrefixes = function(value) {
      if (typeof value === "string" && value[0] === "/") {
        let offset = 0;
        if (!seenPrefixes.has(value)) {
          while (offset < value.length) {
            const nextSlash = value.indexOf("/", offset + 1);
            if (nextSlash === -1)
              break;
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
    };
    const seenPrefixes = new Set;
    scanPrefixes(rootValue);
  }
  writeAny(rootValue);
  if (opts.onChunk)
    return;
  const output = new Uint8Array(byteLength);
  if (reverse) {
    let offset = 0;
    for (const chunk of parts) {
      output.set(chunk, offset);
      offset += chunk.byteLength;
    }
  } else {
    let offset = 0;
    for (let i = parts.length - 1;i >= 0; i--) {
      const chunk = parts[i];
      output.set(chunk, offset);
      offset += chunk.byteLength;
    }
  }
  return output;
  function pushBytes(bytes) {
    onChunk(bytes, byteLength);
    return byteLength += bytes.byteLength;
  }
  function pushString(str) {
    const bytes = new TextEncoder().encode(str);
    return pushBytes(bytes);
  }
  function indent() {
    pushString(`
` + "  ".repeat(indentLevel));
  }
  function writeAny(value, needsSkippable = false) {
    if (!pointers)
      return writeAnyInner(value, needsSkippable);
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
    const ret = writeAnyInner(value, needsSkippable);
    seenOffsets[key] = byteLength;
    seenCosts[key] = byteLength - before;
    return ret;
  }
  function writeAnyInner(value, needsSkippable) {
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
        if (value === null)
          return pushString(writeStringPair("'", "n"));
        ;
        if (Array.isArray(value))
          return writeArray(value, needsSkippable);
        return writeObject(value, needsSkippable);
      default:
        throw new TypeError(`Unsupported value type: ${typeof value}`);
    }
  }
  function writeString(value) {
    if (bareStrings && b64Regex.test(value)) {
      return pushString(writeStringPair(".", value));
    }
    if (pathChains && value[0] === "/" && value.length > 1) {
      if (pointers) {
        if (value === "/") {
          return pushString("/");
        }
        if (duplicatePrefixes.has(value) && value.lastIndexOf("/") === 0) {
          const before = byteLength;
          writeAny(value.substring(1));
          const size = byteLength - before;
          return pushString(writeUnsigned("/", size));
        }
        let offset = value.length;
        let head;
        let tail;
        while (offset > 0) {
          offset = value.lastIndexOf("/", offset - 1);
          if (offset <= 0)
            break;
          const prefix = value.slice(0, offset);
          if (duplicatePrefixes.has(prefix)) {
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
  function writeNumber(value) {
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
    if (exp >= 0 && exp < 5 && Number.isInteger(base) && Number.isSafeInteger(base)) {
      return pushString(writeSigned("+", value));
    }
    pushString(writeSigned("+", base));
    return pushString(writeSigned("*", exp));
  }
  function writeArray(value, needsSkippable = false) {
    if (value.length === 0) {
      return pushString(";");
    }
    if (!needsSkippable) {
      pushString(reverse ? "[" : "]");
      if (pretty) {
        indent();
      }
    }
    indentLevel++;
    const before = byteLength;
    for (let f = value.length - 1, i = f;i >= 0; i--) {
      if (pretty && reverse) {
        if (i === f) {
          pushString("  ");
        } else {
          indent();
        }
      }
      writeAny(value[i], randomAccess);
      if (pretty && !reverse) {
        indent();
      }
    }
    const length = byteLength - before;
    indentLevel--;
    if (pretty && reverse) {
      indent();
    }
    return pushString(needsSkippable ? writeUnsigned(";", length) : reverse ? "]" : "[");
  }
  function writeObject(value, needsSkippable = false) {
    const keys = Object.keys(value);
    if (keys.length === 0) {
      return pushString(":");
    }
    if (!needsSkippable) {
      pushString(reverse ? "{" : "}");
      if (pretty) {
        indent();
      }
    }
    indentLevel++;
    const before = byteLength;
    let keysKey;
    let schemaTarget;
    let schemaRef;
    if (schemas) {
      keysKey = makeKey(keys);
      schemaRef = refs[keysKey];
      schemaTarget = schemaOffsets[keysKey] ?? seenOffsets[keysKey];
    }
    const useSchema = schemaRef !== undefined || schemaTarget !== undefined;
    if (useSchema) {
      const values = Object.values(value);
      for (let f = values.length - 1, i = f;i >= 0; i--) {
        if (pretty && reverse) {
          if (i === f) {
            pushString("  ");
          } else {
            indent();
          }
        }
        writeAny(values[i], randomAccess);
        if (pretty && !reverse) {
          indent();
        }
      }
      if (pretty && reverse) {
        indentLevel--;
        indent();
        indentLevel++;
      }
      if (schemaRef !== undefined) {
        pushString(writeStringPair("'", schemaRef));
      } else if (schemaTarget !== undefined) {
        pushString(writeUnsigned("^", byteLength - schemaTarget));
      } else {
        writeAny(keys, randomAccess);
      }
      if (pretty) {
        pushString(" ");
      }
    } else {
      const entries = Object.entries(value);
      for (let f = entries.length - 1, i = f;i >= 0; i--) {
        const [key, val] = entries[i];
        if (pretty && reverse) {
          if (i === f) {
            pushString("  ");
          } else {
            indent();
          }
        }
        writeAny(val, randomAccess);
        if (pretty)
          pushString(" ");
        writeAny(key);
        if (pretty && !reverse) {
          indent();
        }
      }
    }
    const length = byteLength - before;
    indentLevel--;
    if (pretty && reverse && !useSchema) {
      indent();
    }
    const ret = pushString(needsSkippable ? writeUnsigned(":", length) : reverse ? "}" : "{");
    if (schemas && keysKey && !useSchema) {
      schemaOffsets[keysKey] = byteLength;
    }
    return ret;
  }
}
function decode(input, options) {
  const opts = { ...DECODE_DEFAULTS, ...options };
  throw new Error("TODO: implement parse");
}
function parse(input, options) {
  return decode(new TextEncoder().encode(input), options);
}
function trimZeroes(str) {
  const trimmed = str.replace(/0+$/, "");
  const zeroCount = str.length - trimmed.length;
  return [parseInt(trimmed, 10), zeroCount];
}
function splitNumber(val) {
  if (Number.isInteger(val)) {
    if (Math.abs(val) < 10) {
      return [val, 0];
    }
    if (Math.abs(val) < 999999999999999900000) {
      return trimZeroes(val.toString());
    }
  }
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
  const sciStr = val.toExponential(14).match(/^([+-]?\d+)(?:\.(\d+))?(?:e([+-]?\d+))$/);
  if (sciStr) {
    const e1 = -(sciStr[2]?.length ?? 0);
    const e2 = parseInt(sciStr[3] ?? "0", 10);
    const [b1, e3] = trimZeroes(sciStr[1] + (sciStr[2] ?? ""));
    return [b1, e1 + e2 + e3];
  }
  throw new Error(`Invalid number format: ${val}`);
}
var KeyMap = new WeakMap;
function makeKey(val) {
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
export {
  writeB64,
  toZigZag,
  toB64Signed,
  toB64,
  stringify,
  splitNumber,
  reverseEncoders,
  readB64,
  parse,
  fromZigZag,
  fromB64Signed,
  fromB64,
  forwardEncoders,
  encode,
  decode,
  BUILTIN_REFS
};

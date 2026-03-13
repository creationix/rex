// Custom base64 numeric system
// Numbers are written big-endian with the most significant digit on the left
// There is no padding, not even for zero, which is an empty string

export const regex = /^[0-9a-zA-Z\-_]*$/;

export const chars =
  "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";

// char-code -> digit-value (0xff = invalid)
export const decodeTable = new Uint8Array(256).fill(0xff);

// digit-value -> char-code
export const encodeTable = new Uint8Array(64);

// Build out the tables for encoding and decoding 
// based on chars as source of truth
for (let i = 0; i < 64; i++) {
  const code = chars.charCodeAt(i);
  decodeTable[code] = i;
  encodeTable[i] = code;
}

// Return true if byte is 0-9, a-z, A-Z, '-' or '_'
export function is(byte: number): boolean {
  return decodeTable[byte] !== 0xff;
}

// Encode a number as b64 string
export function stringify(num: number): string {
  if (!Number.isSafeInteger(num) || num < 0) {
    throw new Error(`Cannot stringify ${num} as base64`);
  }
  let result = "";
  while (num > 0) {
    result = chars[num % 64] + result;
    num = Math.floor(num / 64);
  }
  return result;
}

// Decode a b64 string to a number, throws if invalid character is found
export function parse(str: string): number {
  let result = 0;
  for (let i = 0; i < str.length; i++) {
    const digit = decodeTable[str.charCodeAt(i)]!;
    if (digit === 0xff) {
      throw new Error(`Invalid base64 character: ${str[i]}`);
    }
    result = result * 64 + digit;
  }
  return result;
}

// right is after last base64 digit
// left is before first base64 digit
// Digits are big-endian so the left-most digit is the most significant
export function read(
  data: Uint8Array,
  left: number,
  right: number,
): number {
  let result = 0;
  for (let i = left; i < right; i++) {
    const digit = decodeTable[data[i]!]!
    if (digit === 0xff) {
      throw new Error(`Invalid base64 character code: ${data[i]}`);
    }
    result = result * 64 + digit;
  }
  return result;
}

// Return the number of b64 digits needed to encode num
export function sizeof(num: number): number {
  if (!Number.isSafeInteger(num) || num < 0) {
    throw new Error(`Cannot calculate size of ${num} as base64`);
  }
  return Math.ceil(Math.log(num + 1) / Math.log(64));
}

export function write(
  data: Uint8Array,
  left: number,
  right: number,
  num: number,
) {
  let offset = right - 1;
  while (offset >= left) {
    data[offset--] = encodeTable[num % 64]!;
    num = Math.floor(num / 64);
  }
  if (num > 0) {
    throw new Error(`Cannot write ${num} as base64`);
  }
}

// Encode a signed integer as an unsigned zigzag value
export function toZigZag(num: number): number {
  // Bitwise path for int32 range; >>> 0 converts signed result to uint32
  if (num >= -0x80000000 && num <= 0x7fffffff) {
    return ((num << 1) ^ (num >> 31)) >>> 0;
  }
  return num < 0 ? num * -2 - 1 : num * 2;
}

// Decode an unsigned zigzag value back to a signed integer
export function fromZigZag(num: number): number {
  // Bitwise path for uint32 range
  if (num <= 0xffffffff) {
    return (num >>> 1) ^ -(num & 1);
  }
  return num % 2 === 0 ? num / 2 : (num + 1) / -2;
}
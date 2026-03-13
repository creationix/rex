import { describe, expect, test } from "bun:test";
import { parse, stringify, is, read, write, sizeof } from "./b64";

describe('b64 stringify', () => {
  test('encoding b64 digits in correct order', () => {
    expect(stringify(0)).toBe('');
    expect(stringify(1)).toBe('1');
    expect(stringify(9)).toBe('9');
    expect(stringify(10)).toBe('a');
    expect(stringify(35)).toBe('z');
    expect(stringify(36)).toBe('A');
    expect(stringify(61)).toBe('Z');
    expect(stringify(62)).toBe('-');
    expect(stringify(63)).toBe('_');
    expect(stringify(64)).toBe('10');
  });
  test('encoding b64 as powers of 16)', () => {
    expect(stringify(0x1)).toBe('1');
    expect(stringify(0x10)).toBe('g');
    expect(stringify(0x100)).toBe('40');
    expect(stringify(0x1000)).toBe('100');
    expect(stringify(0x10000)).toBe('g00');
    expect(stringify(0x100000)).toBe('4000');
    expect(stringify(0x1000000)).toBe('10000');
    expect(stringify(0x10000000)).toBe('g0000');
    expect(stringify(0x100000000)).toBe('400000');
    expect(stringify(0x1000000000)).toBe('1000000');
    expect(stringify(0x10000000000)).toBe('g000000');
    expect(stringify(0x100000000000)).toBe('40000000');
    expect(stringify(0x1000000000000)).toBe('100000000');
    expect(stringify(0x10000000000000)).toBe('g00000000');
  });
  test('encoding b64 near 12, 32 and 53 bit precision limits)', () => {
    expect(stringify(2 ** 16 - 5)).toBe('f_X');
    expect(stringify(2 ** 16 - 4)).toBe('f_Y');
    expect(stringify(2 ** 16 - 3)).toBe('f_Z');
    expect(stringify(2 ** 16 - 2)).toBe('f_-');
    expect(stringify(2 ** 16 - 1)).toBe('f__');
    expect(stringify(2 ** 16)).toBe('g00');
    expect(stringify(2 ** 16 + 1)).toBe('g01');
    expect(stringify(2 ** 16 + 2)).toBe('g02');
    expect(stringify(2 ** 16 + 3)).toBe('g03');
    expect(stringify(2 ** 16 + 4)).toBe('g04');
    expect(stringify(2 ** 32 - 5)).toBe('3____X');
    expect(stringify(2 ** 32 - 4)).toBe('3____Y');
    expect(stringify(2 ** 32 - 3)).toBe('3____Z');
    expect(stringify(2 ** 32 - 2)).toBe('3____-');
    expect(stringify(2 ** 32 - 1)).toBe('3_____');
    expect(stringify(2 ** 32)).toBe('400000');
    expect(stringify(2 ** 32 + 1)).toBe('400001');
    expect(stringify(2 ** 32 + 2)).toBe('400002');
    expect(stringify(2 ** 32 + 3)).toBe('400003');
    expect(stringify(2 ** 32 + 4)).toBe('400004');
    expect(stringify(2 ** 53 - 1)).toBe('v________');
    expect(stringify(2 ** 53 - 2)).toBe('v_______-');
    expect(stringify(2 ** 53 - 3)).toBe('v_______Z');
    expect(stringify(2 ** 53 - 4)).toBe('v_______Y');
    expect(stringify(2 ** 53 - 5)).toBe('v_______X');
  });
  test('fails on invalid inputs', () => {
    expect(() => stringify(-1)).toThrow();
    expect(() => stringify(1.5)).toThrow();
    expect(() => stringify(NaN)).toThrow();
    expect(() => stringify(Infinity)).toThrow();
  });
});

describe('b64 parse', () => {
  test('decoding b64 digits in correct order', () => {
    expect(parse('')).toBe(0);
    expect(parse('1')).toBe(1);
    expect(parse('9')).toBe(9);
    expect(parse('a')).toBe(10);
    expect(parse('z')).toBe(35);
    expect(parse('A')).toBe(36);
    expect(parse('Z')).toBe(61);
    expect(parse('-')).toBe(62);
    expect(parse('_')).toBe(63);
    expect(parse('10')).toBe(64);
  })
  test('decoding b64 as powers of 16)', () => {
    expect(parse('1')).toBe(0x1);
    expect(parse('g')).toBe(0x10);
    expect(parse('40')).toBe(0x100);
    expect(parse('100')).toBe(0x1000);
    expect(parse('g00')).toBe(0x10000);
    expect(parse('4000')).toBe(0x100000);
    expect(parse('10000')).toBe(0x1000000);
    expect(parse('g0000')).toBe(0x10000000);
    expect(parse('400000')).toBe(0x100000000);
    expect(parse('1000000')).toBe(0x1000000000);
    expect(parse('g000000')).toBe(0x10000000000);
    expect(parse('40000000')).toBe(0x100000000000);
    expect(parse('100000000')).toBe(0x1000000000000);
    expect(parse('g00000000')).toBe(0x10000000000000);
  });
  test('decoding b64 near 12, 32 and 53 bit precision limits)', () => {
    expect(parse('f_X')).toBe(2 ** 16 - 5);
    expect(parse('f_Y')).toBe(2 ** 16 - 4);
    expect(parse('f_Z')).toBe(2 ** 16 - 3);
    expect(parse('f_-')).toBe(2 ** 16 - 2);
    expect(parse('f__')).toBe(2 ** 16 - 1);
    expect(parse('g00')).toBe(2 ** 16);
    expect(parse('g01')).toBe(2 ** 16 + 1);
    expect(parse('g02')).toBe(2 ** 16 + 2);
    expect(parse('g03')).toBe(2 ** 16 + 3);
    expect(parse('g04')).toBe(2 ** 16 + 4);
    expect(parse('3____X')).toBe(2 ** 32 - 5);
    expect(parse('3____Y')).toBe(2 ** 32 - 4);
    expect(parse('3____Z')).toBe(2 ** 32 - 3);
    expect(parse('3____-')).toBe(2 ** 32 - 2);
    expect(parse('3_____')).toBe(2 ** 32 - 1);
    expect(parse('400000')).toBe(2 ** 32);
    expect(parse('400001')).toBe(2 ** 32 + 1);
    expect(parse('400002')).toBe(2 ** 32 + 2);
    expect(parse('400003')).toBe(2 ** 32 + 3);
    expect(parse('400004')).toBe(2 ** 32 + 4);
    expect(parse('w00000000')).toBe(2 ** 53);
    expect(parse('v________')).toBe(2 ** 53 - 1);
    expect(parse('v_______-')).toBe(2 ** 53 - 2);
    expect(parse('v_______Z')).toBe(2 ** 53 - 3);
    expect(parse('v_______Y')).toBe(2 ** 53 - 4);
    expect(parse('v_______X')).toBe(2 ** 53 - 5);
  });
});

describe('b64 parse/stringify', () => {
  test('random fuzzing', () => {
    for (let i = 0; i < 100000; i++) {
      const n = Math.floor(Math.random() * (Number.MAX_SAFE_INTEGER + 2));
      expect(parse(stringify(n))).toBe(n);
    }
  });
});

describe('b64 is', () => {
  test('valid characters', () => {
    for (let i = 0; i < 256; i++) {
      const char = String.fromCharCode(i);
      if (
        (i >= 48 && i <= 57) || // 0-9
        (i >= 65 && i <= 90) || // A-Z
        (i >= 97 && i <= 122) || // a-z
        char === '-' ||
        char === '_'
      ) {
        expect(is(i)).toBe(true);
      } else {
        expect(is(i)).toBe(false);
      }
    }
  });
});

describe('b64 sizeof', () => {
  test('size of b64 encoding', () => {
    expect(() => sizeof(-1)).toThrow();
    expect(sizeof(0)).toBe(0);
    expect(sizeof(1)).toBe(1);
    expect(sizeof(63)).toBe(1);
    expect(sizeof(64)).toBe(2);
    expect(sizeof(4095)).toBe(2);
    expect(sizeof(4096)).toBe(3);
    expect(sizeof(262143)).toBe(3);
    expect(sizeof(262144)).toBe(4);
    expect(sizeof(2 ** 53 - 1)).toBe(9);
    expect(() => sizeof(2 ** 53)).toThrow();
  });
});

describe('b64 read', () => {
  test('decoding b64 digits in correct order', () => {
    const data = new Uint8Array([45, 95, 48, 49]); // '-_01'
    expect(read(data, 0, 1)).toBe(62);
    expect(read(data, 1, 2)).toBe(63);
    expect(read(data, 2, 3)).toBe(0);
    expect(read(data, 3, 4)).toBe(1);
    expect(read(data, 2, 4)).toBe(0 * 64 + 1);
    expect(read(data, 0, 2)).toBe(62 * 64 + 63);
    expect(read(data, 0, 3)).toBe(62 * 64 * 64 + 63 * 64 + 0);
    expect(read(data, 0, 4)).toBe(62 * 64 * 64 * 64 + 63 * 64 * 64 + 0 * 64 + 1);
    expect(read(data, 1, 4)).toBe(63 * 64 * 64 + 0 * 64 + 1);
  });

  test('fails on invalid characters', () => {
    const data = new Uint8Array([45, 95, 48, 49, 64]); // '-_01@'
    expect(() => read(data, 0, 5)).toThrow();
    expect(() => read(data, 4, 5)).toThrow();
    expect(() => read(data, 0, 4)).not.toThrow();
  });
});

describe('b64 write', () => {
  test('writing b64 digits to data', () => {
    const data = new Uint8Array(10);
    write(data, 0, 10, 0);
    expect(data.slice(0, 10)).toEqual(new Uint8Array([48, 48, 48, 48, 48, 48, 48, 48, 48, 48]));
    write(data, 0, 2, 62 * 64 + 63); // '-_'
    expect(data.slice(0, 2)).toEqual(new Uint8Array([45, 95]));
    write(data, 2, 5, 62 * 64 * 64 + 63 * 64 + 1); // '-_01'
    expect(data.slice(2, 5)).toEqual(new Uint8Array([45, 95, 49]));
    write(data, 0, 10, Number.MAX_SAFE_INTEGER); // '_v________'
    expect(data.slice(0, 10)).toEqual(new Uint8Array([48, 118, 95, 95, 95, 95, 95, 95, 95, 95]));
    write(data, 0, 10, 2 ** 53); // '0w00000000'
    expect(data.slice(0, 10)).toEqual(new Uint8Array([48, 119, 48, 48, 48, 48, 48, 48, 48, 48]));
  });
  test('fails on write overflow', () => {
    const data = new Uint8Array(5);
    expect(() => write(data, 0, 5, 2 ** 40)).toThrow();
  });
});

describe('b64 sizeof+write+read', () => {
  test('random fuzzing', () => {
    for (let i = 0; i < 100000; i++) {
      const n = Math.floor(Math.random() * (Number.MAX_SAFE_INTEGER + 2));
      const size = sizeof(n);
      const data = new Uint8Array(size);
      write(data, 0, size, n);
      expect(read(data, 0, size)).toBe(n);
    }
  });
});


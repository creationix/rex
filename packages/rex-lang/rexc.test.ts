import { describe, expect, test } from "bun:test";
import { stringify, parse, encode, decode, toB64, fromB64, readB64, writeB64, toZigZag, fromZigZag } from "./rexc.ts";

function toHex(buf: Uint8Array): string {
	return Array.from(buf, (b) => b.toString(16).padStart(2, "0")).join("");
}

function fromHex(hex: string): Uint8Array {
	const bytes = new Uint8Array(hex.length / 2);
	for (let i = 0; i < hex.length; i += 2) {
		bytes[i / 2] = parseInt(hex.slice(i, i + 2), 16);
	}
	return bytes;
}

function expectHex(buf: Uint8Array, hex: string) {
	expect(toHex(buf)).toBe(hex);
}

describe("toZigZag", () => {
	test("encodes 0 as 0", () => {
		expect(toZigZag(0)).toBe(0);
	});

	test("encodes small positive values", () => {
		expect(toZigZag(1)).toBe(2);
		expect(toZigZag(2)).toBe(4);
		expect(toZigZag(10)).toBe(20);
		expect(toZigZag(42)).toBe(84);
		expect(toZigZag(100)).toBe(200);
	});

	test("encodes small negative values", () => {
		expect(toZigZag(-1)).toBe(1);
		expect(toZigZag(-2)).toBe(3);
		expect(toZigZag(-10)).toBe(19);
		expect(toZigZag(-42)).toBe(83);
		expect(toZigZag(-100)).toBe(199);
	});

	test("interleaves positive and negative", () => {
		// 0, -1, 1, -2, 2, -3, 3, ...
		expect(toZigZag(0)).toBe(0);
		expect(toZigZag(-1)).toBe(1);
		expect(toZigZag(1)).toBe(2);
		expect(toZigZag(-2)).toBe(3);
		expect(toZigZag(2)).toBe(4);
		expect(toZigZag(-3)).toBe(5);
		expect(toZigZag(3)).toBe(6);
	});

	test("handles values beyond 32-bit range", () => {
		// These use the arithmetic path (no bitwise overflow)
		expect(toZigZag(0x80000000)).toBe(0x100000000);
		expect(toZigZag(-0x80000001)).toBe(0x100000001);
		expect(toZigZag(Number.MAX_SAFE_INTEGER)).toBe(Number.MAX_SAFE_INTEGER * 2);
	});
});

describe("fromZigZag", () => {
	test("decodes 0 as 0", () => {
		expect(fromZigZag(0)).toBe(0);
	});

	test("decodes even values to positive", () => {
		expect(fromZigZag(2)).toBe(1);
		expect(fromZigZag(4)).toBe(2);
		expect(fromZigZag(20)).toBe(10);
		expect(fromZigZag(84)).toBe(42);
		expect(fromZigZag(200)).toBe(100);
	});

	test("decodes odd values to negative", () => {
		expect(fromZigZag(1)).toBe(-1);
		expect(fromZigZag(3)).toBe(-2);
		expect(fromZigZag(19)).toBe(-10);
		expect(fromZigZag(83)).toBe(-42);
		expect(fromZigZag(199)).toBe(-100);
	});

	test("handles values beyond 32-bit range", () => {
		expect(fromZigZag(0x100000000)).toBe(0x80000000);
		expect(fromZigZag(0x100000001)).toBe(-0x80000001);
	});
});

describe("zigzag round-trip", () => {
	test("round-trips small values", () => {
		for (let i = -100; i <= 100; i++) {
			expect(fromZigZag(toZigZag(i))).toBe(i);
		}
	});

	test("round-trips 32-bit boundary values", () => {
		const values = [0x7fffffff, -0x80000000, 0x7ffffffe, -0x7fffffff];
		for (const n of values) {
			expect(fromZigZag(toZigZag(n))).toBe(n);
		}
	});

	test("round-trips beyond 32-bit range", () => {
		// Note: -MAX_SAFE_INTEGER overflows float precision in zigzag encoding
		const values = [0x80000000, -0x80000001, 0x100000000, -0x100000000, Number.MAX_SAFE_INTEGER];
		for (const n of values) {
			expect(fromZigZag(toZigZag(n))).toBe(n);
		}
	});

	test("small magnitudes produce small encodings", () => {
		// Key property: zigzag keeps small values small regardless of sign
		expect(toZigZag(1)).toBeLessThan(toZigZag(100));
		expect(toZigZag(-1)).toBeLessThan(toZigZag(100));
		expect(toZigZag(-1)).toBeLessThan(toZigZag(-100));
	});
});

describe("toB64", () => {
	test("encodes 0 as empty string", () => {
		expect(toB64(0)).toBe("");
	});

	test("encodes single-digit values", () => {
		expect(toB64(1)).toBe("1");
		expect(toB64(9)).toBe("9");
		expect(toB64(10)).toBe("a");
		expect(toB64(35)).toBe("z");
		expect(toB64(36)).toBe("A");
		expect(toB64(61)).toBe("Z");
		expect(toB64(62)).toBe("-");
		expect(toB64(63)).toBe("_");
	});

	test("encodes two-digit values", () => {
		expect(toB64(64)).toBe("10");
		expect(toB64(65)).toBe("11");
		expect(toB64(127)).toBe("1_");
		expect(toB64(128)).toBe("20");
		expect(toB64(64 * 64 - 1)).toBe("__");
	});

	test("encodes three-digit values", () => {
		expect(toB64(64 * 64)).toBe("100");
		expect(toB64(64 * 64 * 64 - 1)).toBe("___");
	});

	test("uses canonical encoding (no leading zeros)", () => {
		// 1 should be "1" not "01" or "001"
		expect(toB64(1)).toBe("1");
		expect(toB64(64)).toBe("10");
	});
});

describe("fromB64", () => {
	test("decodes empty string as 0", () => {
		expect(fromB64("")).toBe(0);
	});

	test("decodes single-digit values", () => {
		expect(fromB64("0")).toBe(0);
		expect(fromB64("1")).toBe(1);
		expect(fromB64("9")).toBe(9);
		expect(fromB64("a")).toBe(10);
		expect(fromB64("z")).toBe(35);
		expect(fromB64("A")).toBe(36);
		expect(fromB64("Z")).toBe(61);
		expect(fromB64("-")).toBe(62);
		expect(fromB64("_")).toBe(63);
	});

	test("decodes multi-digit values", () => {
		expect(fromB64("10")).toBe(64);
		expect(fromB64("1_")).toBe(127);
		expect(fromB64("__")).toBe(64 * 64 - 1);
		expect(fromB64("100")).toBe(64 * 64);
		expect(fromB64("___")).toBe(64 * 64 * 64 - 1);
	});

	test("throws on invalid characters", () => {
		expect(() => fromB64("!")).toThrow("Invalid base64 character");
		expect(() => fromB64(" ")).toThrow("Invalid base64 character");
		expect(() => fromB64("abc~")).toThrow("Invalid base64 character");
	});
});

describe("b64 round-trip", () => {
	test("round-trips small values", () => {
		for (let i = 0; i <= 63; i++) {
			expect(fromB64(toB64(i))).toBe(i);
		}
	});

	test("round-trips boundary values", () => {
		const boundaries = [64, 127, 128, 255, 256, 4095, 4096, 64 * 64 - 1, 64 * 64, 64 * 64 * 64 - 1, 64 * 64 * 64];
		for (const n of boundaries) {
			expect(fromB64(toB64(n))).toBe(n);
		}
	});

	test("round-trips large values", () => {
		const large = [100_000, 1_000_000, 16_777_216, 268_435_456, Number.MAX_SAFE_INTEGER];
		for (const n of large) {
			expect(fromB64(toB64(n))).toBe(n);
		}
	});
});

describe("writeB64", () => {
	test("writes nothing for 0", () => {
		const buf = new Uint8Array(10);
		const end = writeB64(buf, 0, 0);
		expect(end).toBe(0);
		expectHex(buf.subarray(0, end), "");
	});

	test("writes single-digit values", () => {
		const buf = new Uint8Array(10);
		// 1 → "1" → 0x31
		expectHex(buf.subarray(0, writeB64(buf, 0, 1)), "31");
		// 10 → "a" → 0x61
		expectHex(buf.subarray(0, writeB64(buf, 0, 10)), "61");
		// 63 → "_" → 0x5f
		expectHex(buf.subarray(0, writeB64(buf, 0, 63)), "5f");
	});

	test("writes multi-digit values", () => {
		const buf = new Uint8Array(10);
		// 84 → "1k" → 0x31 0x6b
		expectHex(buf.subarray(0, writeB64(buf, 0, 84)), "316b");
		// 64 → "10" → 0x31 0x30
		expectHex(buf.subarray(0, writeB64(buf, 0, 64)), "3130");
		// 127 → "1_" → 0x31 0x5f
		expectHex(buf.subarray(0, writeB64(buf, 0, 127)), "315f");
		// 128 → "20" → 0x32 0x30
		expectHex(buf.subarray(0, writeB64(buf, 0, 128)), "3230");
		// 4095 → "__" → 0x5f 0x5f
		expectHex(buf.subarray(0, writeB64(buf, 0, 4095)), "5f5f");
	});

	test("writes three-digit values", () => {
		const buf = new Uint8Array(10);
		// 4096 → "100" → 0x31 0x30 0x30
		expectHex(buf.subarray(0, writeB64(buf, 0, 4096)), "313030");
		// 262143 → "___" → 0x5f 0x5f 0x5f
		expectHex(buf.subarray(0, writeB64(buf, 0, 262143)), "5f5f5f");
	});

	test("writes at non-zero offset without touching earlier bytes", () => {
		const buf = new Uint8Array(10);
		buf[0] = 0xff;
		buf[1] = 0xee;
		// write 84 → "1k" at offset 2
		const end = writeB64(buf, 2, 84);
		expect(end).toBe(4);
		expectHex(buf.subarray(0, end), "ffee316b");
	});

	test("round-trips with readB64", () => {
		const values = [0, 1, 63, 64, 4095, 4096, 100_000, 1_000_000, Number.MAX_SAFE_INTEGER];
		for (const n of values) {
			const buf = new Uint8Array(16);
			const end = writeB64(buf, 0, n);
			expect(readB64(buf, 0, end)).toBe(n);
		}
	});

	test("round-trips with readB64 at non-zero offset", () => {
		const values = [42, 64 * 64, 100_000];
		for (const n of values) {
			const buf = new Uint8Array(20);
			const offset = 5;
			const end = writeB64(buf, offset, n);
			expect(readB64(buf, offset, end - offset)).toBe(n);
		}
	});
});

describe("readB64", () => {
	test("reads single digit from hex", () => {
		// 0x31 = '1' → digit 1
		expect(readB64(fromHex("31"), 0, 1)).toBe(1);
		// 0x61 = 'a' → digit 10
		expect(readB64(fromHex("61"), 0, 1)).toBe(10);
		// 0x5f = '_' → digit 63
		expect(readB64(fromHex("5f"), 0, 1)).toBe(63);
	});

	test("reads multi-digit from hex", () => {
		// "1k" → 0x316b → 84
		expect(readB64(fromHex("316b"), 0, 2)).toBe(84);
		// "10" → 0x3130 → 64
		expect(readB64(fromHex("3130"), 0, 2)).toBe(64);
		// "__" → 0x5f5f → 4095
		expect(readB64(fromHex("5f5f"), 0, 2)).toBe(4095);
		// "___" → 0x5f5f5f → 262143
		expect(readB64(fromHex("5f5f5f"), 0, 3)).toBe(262143);
	});

	test("reads at non-zero offset from hex", () => {
		// prefix ff ee, then "1k" (0x316b) at offset 2
		expect(readB64(fromHex("ffee316b"), 2, 2)).toBe(84);
	});

	test("reads zero-length as 0", () => {
		expect(readB64(fromHex("316b"), 0, 0)).toBe(0);
	});

	test("throws on invalid byte", () => {
		// 0x20 = space, not a valid b64 digit
		expect(() => readB64(fromHex("6120"), 0, 2)).toThrow("Invalid base64 character");
	});

	test("agrees with fromB64 for all single-digit values", () => {
		const digits = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";
		const buf = new TextEncoder().encode(digits);
		for (let i = 0; i < 64; i++) {
			expect(readB64(buf, i, 1)).toBe(i);
		}
	});

	test("round-trips with toB64 through buffer", () => {
		const values = [0, 1, 63, 64, 4095, 4096, 100_000, 1_000_000];
		for (const n of values) {
			const str = toB64(n);
			const buf = new TextEncoder().encode(str);
			expect(readB64(buf, 0, buf.length)).toBe(n);
		}
	});
});

describe("rexc stringify", () => {
	describe("primitives", () => {
		test("encodes integers with zigzag + base64", () => {
			expect(stringify(0)).toBe("+");
			expect(stringify(1)).toBe("2+");
			expect(stringify(-1)).toBe("1+");
			expect(stringify(42)).toBe("1k+");
			expect(stringify(-42)).toBe("1j+");
		});

		test("encodes decimals", () => {
			expect(stringify(3.14)).toBe("3*9Q+");
			expect(stringify(0.5)).toBe("1*a+");
			expect(stringify(1000000)).toBe("c*2+");
		});

		test("encodes bare strings", () => {
			expect(stringify("")).toBe(".");
			expect(stringify("hello")).toBe("hello.");
			expect(stringify("x-action")).toBe("x-action.");
		});

		test("encodes length-prefixed strings for non-bare characters", () => {
			expect(stringify("hello world")).toBe("b,hello world");
			expect(stringify("foo bar")).toBe("7,foo bar");
		});

		test("encodes booleans, null, undefined", () => {
			expect(stringify(true)).toBe("t'");
			expect(stringify(false)).toBe("f'");
			expect(stringify(null)).toBe("n'");
			expect(stringify(undefined)).toBe("u'");
		});

		test("encodes special numbers", () => {
			expect(stringify(NaN)).toBe("nan'");
			expect(stringify(Infinity)).toBe("inf'");
			expect(stringify(-Infinity)).toBe("nif'");
		});
	});

	describe("arrays", () => {
		test("encodes simple arrays", () => {
			expect(stringify([1, 2, 3])).toBe("6;2+4+6+");
		});

		test("encodes arrays as values with length prefix", () => {
			const encoded = stringify([[1, 2, 3]], {});
			expect(encoded).toBe("8;6;2+4+6+")
		});

		test("encodes empty array", () => {
			expect(stringify([])).toBe(";");
		});

		test("encodes nested arrays", () => {
			const encoded = stringify([[1], [2]]);
			expect(encoded).toBe("8;2;2+2;4+");
		});

		test('encodes arrays with different formats', () => {
			const data = [[1, 2], [3, 4]]
			expect(stringify(data, { reverse: false })).toBe("c;4;2+4+4;6+8+")
			expect(stringify(data, { reverse: true })).toBe("+8+6;4+4+2;4;c")
			expect(stringify(data, { reverse: false, indexes: 0 })).toBe("o;g#0a8;g#022+4+8;g#026+8+")
			expect(stringify(data, { reverse: true, indexes: 0 })).toBe("+8+602#g;8+4+202#g;80a#g;o")
		})

	});

	describe("objects", () => {
		test("encodes simple objects", () => {
			expect(stringify({ color: "red", size: 42 }))
				.toBe("i:color.red.size.1k+");
		});

		test("encodes empty object", () => {
			expect(stringify({})).toBe(":");
		});

		test("encodes objects with length prefix", () => {
			const encoded = stringify([{ a: 1 }]);
			// Should have a length prefix before {
			expect(encoded).toBe("6;4:a.2+")
		});

		test("encodes objects with different formats", () => {
			const data = { a: { b: 1, c: 1 }, d: { e: 3, f: 4 } }
			expect(stringify(data, { reverse: false })).toBe("o:a.8:b.2+c.2+d.8:e.6+f.8+")
			expect(stringify(data, { reverse: true })).toBe("+8.f+6.e:8.d+2.c+2.b:8.a:o")
			expect(stringify(data, { reverse: false, indexes: 0 })).toBe("A:g#0ga.c:g#04b.2+c.2+d.c:g#04e.6+f.8+")
			expect(stringify(data, { reverse: true, indexes: 0 })).toBe("+8.f+6.e04#g:c.d+2.c+2.b04#g:c.a0g#g:A")
		});

		test("object keys are sorted when indexes enabled", () => {
			const obj = { c: 3, a: 1, b: 2 };
			const encoded = stringify(obj, { indexes: 2 });
			expect(encoded).toBe("h:o#480c.6+a.2+b.4+");
		});
	});

	describe("indexes", () => {
		test('embed index into small array', () => {
			const arr = [1, 2, 3]
			const encoded = stringify(arr, { indexes: 2 });
			expect(encoded).toBe("b;o#0242+4+6+");
		});
		test("embeds index for medium arrays", () => {
			const arr = Array.from({ length: 12 }, (_, i) => i);
			const encoded = stringify(arr, { indexes: 10 });
			expect(encoded).toBe("C;1w#013579bdfhjl+2+4+6+8+a+c+e+g+i+k+m+");
		});
		test("embeds index for large arrays", () => {
			const arr = Array.from({ length: 40 }, (_, i) => i);
			const encoded = stringify(arr, { indexes: 30 });
			expect(encoded).toBe("2G;51#0001030507090b0d0f0h0j0l0n0p0r0t0v0x0z0B0D0F0H0J0L0N0P0R0T0V0X0Z0_1215181b1e1h1k+2+4+6+8+a+c+e+g+i+k+m+o+q+s+u+w+y+A+C+E+G+I+K+M+O+Q+S+U+W+Y+-+10+12+14+16+18+1a+1c+1e+");
		});

		test("skips index for small arrays", () => {
			const encoded = stringify([1, 2, 3], { indexes: 10 });
			expect(encoded).not.toContain("#");
		});

		test("disables index when indexes is false", () => {
			const arr = Array.from({ length: 20 }, (_, i) => i);
			const encoded = stringify(arr, { indexes: false });
			expect(encoded).not.toContain("#");
		});

		test('indices for maps', () => {
			const obj = { a: 1, b: 2, c: 3 };
			const encoded = stringify(obj, { indexes: 2 });
			expect(encoded).toBe("h:o#048a.2+b.4+c.6+");
		})

		test('map indexes sort keys', () => {
			const obj = { c: 3, a: 1, b: 2 };
			const encoded = stringify(obj, { indexes: 2 });
			expect(encoded).toBe("h:o#480c.6+a.2+b.4+");
		})

		test('schema objects can have indices on values', () => {
			const data = [{ name: "alice", age: 1 }, { name: "bob", age: 2 }];
			expect(stringify(data, { indexes: 1, schemas: true }))
				.toBe("F;g#0ge:c^g#06alice.2+j:g#90name.bob.age.4+");
			expect(stringify(data, { indexes: 1, schemas: true, reverse: true }))
				.toBe("+4.age.bob.name90#g:j+2.alice06#g^c:e0g#g;F");
		});
	})

	describe("pointers", () => {
		test("deduplicates repeated strings", () => {
			const encoded = stringify(["hello", "hello"]);
			expect(encoded).toBe("7;^hello.")
		});

		test("deduplicates repeated objects", () => {
			const obj = { x: 1 };
			expect(stringify([obj, obj])).toBe("7;^4:x.2+");
		});

		test("does not deduplicate when pointers disabled", () => {
			const encoded = stringify(["hello", "hello"], { pointers: false });
			expect(encoded).toBe("c;hello.hello.");
		});
	});

	describe("refs", () => {
		test("encodes value matching a ref as ref shorthand", () => {
			expect(stringify("hello", { refs: { H: "hello" }, pointers: true, randomAccess: false }))
				.toBe("H'");
		});

		test("encodes number matching a ref", () => {
			expect(stringify(42, { refs: { X: 42 }, pointers: true, randomAccess: false }))
				.toBe("X'");
		});

		test("encodes refs inside arrays", () => {
			expect(stringify(["hello", "world"], { refs: { H: "hello" }, pointers: true }))
				.toBe("8;H'world.");
		});

		test("encodes multiple refs", () => {
			expect(stringify(["hello", 42], { refs: { H: "hello", X: 42 }, pointers: true }))
				.toBe("4;H'X'");
		});

		test("encodes schema ref for repeated object shapes", () => {
			const data = [{ a: 1, b: 2 }, { a: 3, b: 4 }];
			expect(stringify(data, { refs: { S: ["a", "b"] }, pointers: true, schemas: true }))
				.toBe("g;6:S'2+4+6:S'6+8+");
		});

		test("encodes refs in reverse mode", () => {
			expect(stringify("hello", { refs: { H: "hello" }, pointers: true, reverse: true }))
				.toBe("'H");
		});

		test("use refs even when pointers are disabled", () => {
			expect(stringify("hello", { refs: { H: "hello" }, pointers: false }))
				.toBe("H'");
		});
	});

	describe("shared schemas", () => {
		test("deduplicates repeated object shapes", () => {
			const data = [
				{ name: "alice", age: 1 },
				{ name: "bob", age: 2 },
				{ name: "charlie", age: 3 },
			];
			const withSchemas = stringify(data, { schemas: true });
			const withoutSchemas = stringify(data, { schemas: false });
			expect(withSchemas).toBe("H;a:i^alice.2+8:6^bob.4+j:name.charlie.age.6+")
			expect(withoutSchemas).toBe("L;c:o^alice.t^2+a:a^bob.h^4+j:name.charlie.age.6+")
		});

		test("does not use schemas for single objects", () => {
			const data = [{ name: "alice" }];
			const encoded = stringify(data, { schemas: true });
			expect(encoded).toBe("d;b:name.alice.")
		});
	});

	describe("path chains", () => {
		// Non-repeated prefixes should not use pathChains optimization
		expect(stringify("/", { pathChains: false })).toBe("1,/");
		expect(stringify("/about", { pathChains: false })).toBe("6,/about");
		expect(stringify("/", { pathChains: true })).toBe("1,/");
		expect(stringify("/about", { pathChains: true })).toBe("6,/about");
		const paths = [
			"/foo/bar/baz",
			"/foo/bar/qux",
			"/foo/quux",
		]
		// `/foo` is pointed to twice via `/foo/bar` and `/foo/quux`.
		// `/foo/bar/` is pointed to twice via `/foo/bar/baz` and `/foo/bar/qux`.
		// Therefore we should have prefixes for both of them
		// This inner `/foo` is encoded as `4/foo:`
		// Then the outer `/foo/bar` could point to it or inline if possible
		//
		// In this case we write `/foo/quux` first so `/foo` is a standalone target
		// `/foo/quux` then becomes `b/4/foo:quux:`
		// And now we have pointer targets for both `/foo` and `/foo/quux`
		//
		// Next we encode `/foo/bar/qux` which contains `/foo/bar` that we want to make a target, 
		// and that recursively depends on / points to `/foo` from the previous entry
		// so we write `??/??^bar:` for `/foo/bar`, but will calculate `??` later
		// The entire chain is then `??/??/??^bar:qux:`
		// So when combined with the previous line, we can calculate all pointers and lengths
		// this gives us `c/6/a^bar:qux:b/4/foo:quux:`
		//
		// Now we finally encode `/foo/bar/baz` which can point to `/foo/bar` and then append `baz:`
		// This is `??/??^baz:`
		// now combining to the other we can calculate the `??` slots
		// And we finally get `6/6^baz:c/6/a^bar:qux:b/4/foo:quux:` for the 3 strings
		//
		// The array wrapping is a root object and doesn't need to be skippable so it's just wrapping in `[]`

		expect(stringify(paths, { pathChains: true, pointers: true, schemas: false, reverse: false }))
			.toBe("z;6/6^baz.c/6/a^bar.qux.b/4/foo.quux.")
		// Reverse mode is the same thing in reverse
		expect(stringify(paths, { pathChains: true, pointers: true, schemas: false, reverse: true }))
			.toBe(".quux.foo/4/b.qux.bar^a/6/c.baz^6/6;z")

		// The current implementaion breaks out `/foo` as a "duplicated" prefix.
		// But technically we could stop at `/foo/bar` as the root prefix, 
		// but that requires changing the duplicate prefix detector to be more complex and cancel nested prefixes.
		// Also this form writes cleaner encodings since most path segments are b64 friendly.
		// The inner `/foo/bar` is currently `a/4/foo:bar:` when it could be `9/7,foo/bar`.
		// The "optimized" form is one less character and one less concat layer, but more ugly.
		const prefixedPaths = [
			"/foo/bar/baz",
			"/foo/bar/qux",
		]
		expect(stringify(prefixedPaths, { pathChains: true, pointers: true, schemas: false, reverse: false }))
			.toBe("q;6/6^baz.g/a/4/foo.bar.qux.")
		// Reverse mode is the same thing in reverse
		expect(stringify(prefixedPaths, { pathChains: true, pointers: true, schemas: false, reverse: true }))
			.toBe(".qux.bar.foo/4/a/g.baz^6/6;q")
	})

	describe("website manifest", () => {
		const doc = {
			"/": { name: "Home", method: "GET" },
			"/about": { name: "About", method: "GET" },
			"/contact": { name: "Contact", method: "POST" },
			"/blog": { name: "Blog", method: "GET" },
			"/blog/post": { name: "Blog Post", method: "GET" },
			"/blog/post/comment": { name: "Comment", method: "POST" },
			"/api/data": { name: "API Data", method: "GET" },
			"/api/update": { name: "API Update", method: "POST" },
			"/admin": { name: "Admin", method: "GET" },
			"/admin/settings": { name: "Admin Settings", method: "POST" },
			"/admin/users": { name: "Admin Users", method: "GET" },
			"/admin/users/add": { name: "Add User", method: "POST" },
			"/admin/users/remove": { name: "Remove User", method: "POST" },
			"/admin/logs": { name: "Admin Logs", method: "GET" },
			"/admin/logs/clear": { name: "Clear Logs", method: "POST" },
			"/admin/logs/export": { name: "Export Logs", method: "GET" },
			"/admin/logs/export/json": { name: "Export Logs as JSON", method: "GET" },
			"/admin/logs/export/csv": { name: "Export Logs as CSV", method: "GET" },
		};
		test("byte counts are accurate with different options", () => {
			expect(stringify(doc, { pathChains: true, pointers: true, schemas: true, reverse: false, indexes: false }))
				.toBe("8o:1,/b:7G^Home.84^6,/aboutc:7l^About.7K^8,/contacte:6Z^Contact.5j^H^b:6H^Blog.75^7/q^post.h:6l^9,Blog Post6F^l/5/blog.c,post/commente:5H^Comment.41^7/p^data.g:5i^8,API Data5D^d/4/api.update.i:4N^a,API Update33^47^c:4q^Admin.4P^c/3Q^settings.m:3-^e,Admin Settings2c^N^j:3A^b,Admin Users3S^6/o^add.g:37^8,Add User1r^i/9/2r^users.remove.i:2x^b,Remove UserP^1R^i:2a^a,Admin Logs2t^9/1s^clear.k:1H^a,Clear LogsPOST.Y^j:1j^b,Export Logs1B^7/z^json.q:S^j,Export Logs as JSON10^s/m/d/6/admin.logs.export.csv.A:name.i,Export Logs as CSVmethod.GET.");
			expect(stringify(doc, { pathChains: true, pointers: true, schemas: true, reverse: true, indexes: false }))
				.toBe(".GET.methodExport Logs as CSV,i.name:A.csv.export.logs.admin/6/d/m/s^10Export Logs as JSON,j^S:q.json^z/7^1BExport Logs,b^1j:j^Y.POSTClear Logs,a^1H:k.clear^1s/9^2tAdmin Logs,a^2a:i^1R^PRemove User,b^2x:i.remove.users^2r/9/i^1rAdd User,8^37:g.add^o/6^3SAdmin Users,b^3A:j^N^2cAdmin Settings,e^3-:m.settings^3Q/c^4P.Admin^4q:c^47^33API Update,a^4N:i.update.api/4/d^5DAPI Data,8^5i:g.data^p/7^41.Comment^5H:epost/comment,c.blog/5/l^6FBlog Post,9^6l:h.post^q/7^75.Blog^6H:b^H^5j.Contact^6Z:e/contact,8^7K.About^7l:c/about,6^84.Home^7G:b/,1:8o");
			expect(stringify(doc, { pathChains: true, pointers: true, schemas: false, reverse: false, indexes: false }))
				.toBe("9b:1,/e:8v^Home.8M^8Q^6,/aboutf:87^About.8n^8r^8,/contacth:7I^Contact.7W^5T^N^e:7n^Blog.7E^7I^7/t^post.k:6-^9,Blog Post79^7d^l/5/blog.c,post/commenth:6h^Comment.6v^4s^7/s^data.j:5R^8,API Data61^65^d/4/api.update.l:5h^a,API Update5r^3o^4y^f:4T^Admin.57^5b^c/4c^settings.p:4o^e,Admin Settings4u^2r^T^m:3X^b,Admin Users44^48^6/r^add.j:3r^8,Add User3D^1A^i/9/2G^users.remove.l:2O^b,Remove User2X^V^21^l:2o^a,Admin Logs2y^2C^9/1B^clear.n:1S^a,Clear Logs20^POST.11^m:1q^b,Export Logs1z^1D^7/B^json.s:W^j,Export Logs as JSONY^10^s/m/d/6/admin.logs.export.csv.A:name.i,Export Logs as CSVmethod.GET.");
			expect(stringify(doc, { pathChains: true, pointers: true, schemas: false, reverse: true, indexes: false }))
				.toBe(".GET.methodExport Logs as CSV,i.name:A.csv.export.logs.admin/6/d/m/s^10^YExport Logs as JSON,j^W:s.json^B/7^1D^1zExport Logs,b^1q:m^11.POST^20Clear Logs,a^1S:n.clear^1B/9^2C^2yAdmin Logs,a^2o:l^21^V^2XRemove User,b^2O:l.remove.users^2G/9/i^1A^3DAdd User,8^3r:j.add^r/6^48^44Admin Users,b^3X:m^T^2r^4uAdmin Settings,e^4o:p.settings^4c/c^5b^57.Admin^4T:f^4y^3o^5rAPI Update,a^5h:l.update.api/4/d^65^61API Data,8^5R:j.data^s/7^4s^6v.Comment^6h:hpost/comment,c.blog/5/l^7d^79Blog Post,9^6-:k.post^t/7^7I^7E.Blog^7n:e^N^5T^7W.Contact^7I:h/contact,8^8r^8n.About^87:f/about,6^8Q^8M.Home^8v:e/,1:9b");
			expect(stringify(doc, { pathChains: true, pointers: true, schemas: true, bareStrings: false, reverse: false, indexes: false }))
				.toBe("8N:1,/c:80^4,Home8r^6,/aboutd:7G^5,About84^8,/contactf:7h^7,Contact5w^J^c:6-^4,Blog7p^8/r^4,posth:6C^9,Blog Post6Y^m/6/4,blogc,post/commentf:5X^7,Comment4a^8/q^4,datag:5w^8,API Data5T^f/5/3,api6,updatei:4Z^a,API Update39^4f^d:4C^5,Admin50^d/3X^8,settingsm:48^e,Admin Settings2g^O^j:3K^b,Admin Users42^7/p^3,addg:3g^8,Add User1u^k/a/2w^5,users6,removei:2E^b,Remove UserQ^1U^i:2h^a,Admin Logs2C^a/1v^5,clearl:1N^a,Clear Logs4,POSTZ^j:1o^b,Export Logs1I^8/A^4,jsonq:W^j,Export Logs as JSON16^w/p/f/7/5,admin4,logs6,export3,csvD:4,namei,Export Logs as CSV6,method3,GET");

		})
	})

	describe("emoji party", () => {
		const doc = {
			"/emoji/🔥": { name: "fire", group: "travel-places" },
			"/emoji/💧": { name: "water", group: "travel-places" },
			"/emoji/🌱": { name: "seedling", group: "animals-nature" },
			"/emoji/🐍": { name: "snake", group: "animals-nature" },
			"/emoji/🎸": { name: "guitar", group: "objects" },
			"/emoji/⚽": { name: "soccer ball", group: "activities" },
			"/emoji/❤️": { name: "red heart", group: "smileys-emotion" },
			"/emoji/🏴‍☠️": { name: "pirate flag", group: "flags" },
		};
		test("byte counts are accurate with different options", () => {
			expect(stringify(doc, { pathChains: false, pointers: true, schemas: true, reverse: false }))
				.toBe("4N:b,/emoji/🔥a:3_^fire.o^b,/emoji/💧n:3C^water.travel-places.b,/emoji/🌱e:30^seedling.o^b,/emoji/🐍o:2z^snake.animals-nature.b,/emoji/🎸i:1Y^guitar.objects.a,/emoji/⚽r:1s^b,soccer ballactivities.d,/emoji/❤️t:N^9,red heartsmileys-emotion.k,/emoji/🏴‍☠️u:name.b,pirate flaggroup.flags.");
			expect(stringify(doc, { pathChains: false, pointers: true, schemas: true, reverse: true }))
				.toBe(".flags.grouppirate flag,b.name:u/emoji/🏴‍☠️,k.smileys-emotionred heart,9^N:t/emoji/❤️,d.activitiessoccer ball,b^1s:r/emoji/⚽,a.objects.guitar^1Y:i/emoji/🎸,b.animals-nature.snake^2z:o/emoji/🐍,b^o.seedling^30:e/emoji/🌱,b.travel-places.water^3C:n/emoji/💧,b^o.fire^3_:a/emoji/🔥,b:4N");
			expect(stringify(doc, { pathChains: true, pointers: true, schemas: false, reverse: false }))
				.toBe("4V:9/3Z^4,🔥d:4b^fire.4l^p^9/3z^4,💧q:3N^water.3W^travel-places.9/2Y^4,🌱h:3a^seedling.3g^p^9/2u^4,🐍r:2I^snake.2R^animals-nature.9/1S^4,🎸l:24^guitar.2c^objects.8/1k^3,⚽u:1z^b,soccer ball1B^activities.a/H^6,❤️v:U^9,red heartZ^smileys-emotion.n/6/emoji.d,🏴‍☠️u:name.b,pirate flaggroup.flags.");
			expect(stringify(doc, { pathChains: true, pointers: true, schemas: false, reverse: true }))
				.toBe(".flags.grouppirate flag,b.name:u🏴‍☠️,d.emoji/6/n.smileys-emotion^Zred heart,9^U:v❤️,6^H/a.activities^1Bsoccer ball,b^1z:u⚽,3^1k/8.objects^2c.guitar^24:l🎸,4^1S/9.animals-nature^2R.snake^2I:r🐍,4^2u/9^p^3g.seedling^3a:h🌱,4^2Y/9.travel-places^3W.water^3N:q💧,4^3z/9^p^4l.fire^4b:d🔥,4^3Z/9:4V");
			expect(stringify(doc, { pathChains: true, pointers: true, schemas: true, reverse: false }))
				.toBe("4B:9/3F^4,🔥a:3R^fire.m^9/3i^4,💧n:3u^water.travel-places.9/2K^4,🌱e:2W^seedling.m^9/2j^4,🐍o:2v^snake.animals-nature.9/1K^4,🎸i:1W^guitar.objects.8/1f^3,⚽r:1s^b,soccer ballactivities.a/F^6,❤️t:Q^9,red heartsmileys-emotion.n/6/emoji.d,🏴‍☠️u:name.b,pirate flaggroup.flags.");
			expect(stringify(doc, { pathChains: true, pointers: true, schemas: true, reverse: true }))
				.toBe(".flags.grouppirate flag,b.name:u🏴‍☠️,d.emoji/6/n.smileys-emotionred heart,9^Q:t❤️,6^F/a.activitiessoccer ball,b^1s:r⚽,3^1f/8.objects.guitar^1W:i🎸,4^1K/9.animals-nature.snake^2v:o🐍,4^2j/9^m.seedling^2W:e🌱,4^2K/9.travel-places.water^3u:n💧,4^3i/9^m.fire^3R:a🔥,4^3F/9:4B");
		});
	})

	describe("encode colored fruits", () => {
		const doc = [
			{ color: "red", fruits: ["apple", "strawberry"] },
			{ color: "green", fruits: ["apple"] },
			{ color: "yellow", fruits: ["apple", "banana"] },
			{ color: "orange", fruits: ["orange"] },
		]
		test("with correct options applied", () => {
			expect(stringify(doc, { pointers: false, schemas: false, reverse: false }))
				.toBe("27;A:color.red.fruits.h;apple.strawberry.r:color.green.fruits.6;apple.z:color.yellow.fruits.d;apple.banana.t:color.orange.fruits.7;orange.")
			expect(stringify(doc, { pointers: false, schemas: false, reverse: true }))
				.toBe(".orange;7.fruits.orange.color:t.banana.apple;d.fruits.yellow.color:z.apple;6.fruits.green.color:r.strawberry.apple;h.fruits.red.color:A;27")
			expect(stringify(doc, { pointers: true, schemas: false, reverse: false }))
				.toBe("1x;p:14^red.15^d;G^strawberry.e:G^green.G^2;f^q:q^yellow.p^d;apple.banana.o:color.9^fruits.7;orange.")
			expect(stringify(doc, { pointers: true, schemas: false, reverse: true }))
				.toBe(".orange;7.fruits^9.color:o.banana.apple;d^p.yellow^q:q^f;2^G.green^G:e.strawberry^G;d^15.red^14:p;1x")
			expect(stringify(doc, { pointers: false, schemas: true, reverse: false }))
				.toBe("1D;q:13^red.h;apple.strawberry.g:E^green.6;apple.o:m^yellow.d;apple.banana.t:color.orange.fruits.7;orange.")
			expect(stringify(doc, { pointers: false, schemas: true, reverse: true }))
				.toBe(".orange;7.fruits.orange.color:t.banana.apple;d.yellow^m:o.apple;6.green^E:g.strawberry.apple;h.red^13:q;1D")
			expect(stringify(doc, { pointers: true, schemas: true, reverse: false }))
				.toBe("1p;l:X^red.d;C^strawberry.c:A^green.2;d^o:m^yellow.d;apple.banana.o:color.9^fruits.7;orange.")
			expect(stringify(doc, { pointers: true, schemas: true, reverse: true }))
				.toBe(".orange;7.fruits^9.color:o.banana.apple;d.yellow^m:o^d;2.green^A:c.strawberry^C;d.red^X:l;1p")
		});
	})
})

describe.skip("rexc parse", () => {
	describe("primitives", () => {
		test("parses integers", () => {
			expect(parse("+")).toBe(0);
			expect(parse("2+")).toBe(1);
			expect(parse("1+")).toBe(-1);
			expect(parse("1k+")).toBe(42);
			expect(parse("1j+")).toBe(-42);
		});

		test("parses decimals", () => {
			expect(parse("3*9Q+")).toBe(3.14);
			expect(parse("1*2+")).toBe(0.5);
		});

		test("parses bare strings", () => {
			expect(parse(":")).toBe("");
			expect(parse("hello:")).toBe("hello");
			expect(parse("x-action:")).toBe("x-action");
		});

		test("parses length-prefixed strings", () => {
			expect(parse("b,hello world")).toBe("hello world");
			expect(parse("7,foo bar")).toBe("foo bar");
		});

		test("parses booleans, null, undefined", () => {
			expect(parse("tr'")).toBe(true);
			expect(parse("fl'")).toBe(false);
			expect(parse("nl'")).toBe(null);
			expect(parse("un'")).toBe(undefined);
		});

		test("parses special numbers", () => {
			expect(parse("nan'")).toBeNaN();
			expect(parse("inf'")).toBe(Infinity);
			expect(parse("nif'")).toBe(-Infinity);
		});
	});

	describe("arrays", () => {
		test("parses simple arrays", () => {
			expect(parse("[2+4+6+]")).toEqual([1, 2, 3]);
		});

		test("parses arrays with length prefix", () => {
			expect(parse("6[2+4+6+]")).toEqual([1, 2, 3]);
		});

		test("parses empty array", () => {
			expect(parse("[]")).toEqual([]);
		});
	});

	describe("objects", () => {
		test("parses simple objects", () => {
			expect(parse("{color:red:size:1k+}")).toEqual({ color: "red", size: 42 });
		});

		test("parses empty object", () => {
			expect(parse("{}")).toEqual({});
		});
	});

	describe("pointers", () => {
		test("resolves pointer references", () => {
			const result = decode("[hello:^]") as string[];
			expect(result).toEqual(["hello", "hello"]);
		});
	});

	describe("lazy mode", () => {
		test("returns proxy when lazy is true", () => {
			const result = decode("{a:2+b:4+}", { lazy: true });
			expect((result as Record<string, number>).a).toBe(1);
			expect((result as Record<string, number>).b).toBe(2);
		});

		test("returns plain object when lazy is false", () => {
			const result = decode("{a:2+b:4+}", { lazy: false });
			expect(result).toEqual({ a: 1, b: 2 });
		});
	});
});

describe.skip("rexc round-trip", () => {
	const roundTrip = (value: unknown, options?: Parameters<typeof encode>[1]) => {
		const encoded = stringify(value, options);
		return decode(encoded);
	};

	test("round-trips primitives", () => {
		expect(roundTrip(0)).toBe(0);
		expect(roundTrip(1)).toBe(1);
		expect(roundTrip(-1)).toBe(-1);
		expect(roundTrip(42)).toBe(42);
		expect(roundTrip(3.14)).toBe(3.14);
		expect(roundTrip("hello")).toBe("hello");
		expect(roundTrip("hello world")).toBe("hello world");
		expect(roundTrip("")).toBe("");
		expect(roundTrip(true)).toBe(true);
		expect(roundTrip(false)).toBe(false);
		expect(roundTrip(null)).toBe(null);
		expect(roundTrip(undefined)).toBe(undefined);
	});

	test("round-trips arrays", () => {
		expect(roundTrip([])).toEqual([]);
		expect(roundTrip([1, 2, 3])).toEqual([1, 2, 3]);
		expect(roundTrip(["a", "b", "c"])).toEqual(["a", "b", "c"]);
		expect(roundTrip([[1, 2], [3, 4]])).toEqual([[1, 2], [3, 4]]);
	});

	test("round-trips objects", () => {
		expect(roundTrip({})).toEqual({});
		expect(roundTrip({ a: 1, b: 2 })).toEqual({ a: 1, b: 2 });
		expect(roundTrip({ name: "rex", nested: { ok: true } })).toEqual({
			name: "rex",
			nested: { ok: true },
		});
	});

	test("round-trips complex nested structures", () => {
		const value = {
			routes: [
				{ path: "/api/users", handler: "getUsers", methods: ["GET"] },
				{ path: "/api/users", handler: "createUser", methods: ["POST"] },
			],
			metadata: { version: 1, generated: true },
		};
		expect(roundTrip(value)).toEqual(value);
	});

	test("round-trips with all features enabled", () => {
		const value = {
			paths: ["/docs/api/v2/users", "/docs/api/v2/teams", "/docs/api/v2/billing"],
			config: { retries: 3, timeout: 30 },
		};
		expect(
			roundTrip(value, {
				pointers: true,
				schemas: true,
				pathChains: true,
			}),
		).toEqual(value);
	});

	test("round-trips with all features disabled", () => {
		const value = { items: [1, "two", null, true, { nested: [3.14] }] };
		expect(
			roundTrip(value, {
				pointers: false,
				schemas: false,
				pathChains: false,
				indexes: false,
			}),
		).toEqual(value);
	});

	test("round-trips with duplicated values", () => {
		const shared = { type: "page", status: 200 };
		const value = [shared, shared, shared];
		expect(roundTrip(value)).toEqual(value);
	});

	test("round-trips large indexed arrays", () => {
		const arr = Array.from({ length: 100 }, (_, i) => i);
		expect(roundTrip(arr, { indexes: 10 })).toEqual(arr);
	});

	test("round-trips large indexed objects", () => {
		const obj: Record<string, number> = {};
		for (let i = 0; i < 50; i++) obj[`key${i}`] = i;
		expect(roundTrip(obj, { indexes: 10 })).toEqual(obj);
	});
});


describe("rexc streaming", () => {
	test("onChunk receives chunks in reverse mode", () => {
		const chunks: { offset: number; data: string }[] = [];
		stringify({ a: 1 }, {
			reverse: true,
			onChunk: (chunk, offset) => chunks.push({ offset, data: chunk }),
		});
		expect(chunks).toEqual([
			{
				offset: 0,
				data: "+2",
			}, {
				offset: 2,
				data: ".a",
			}, {
				offset: 4,
				data: ":4",
			}
		])
	});

	test("onChunk offsets are increasing in reverse mode", () => {
		const offsets: number[] = [];
		encode([1, 2, 3, "hello", { a: true }], {
			reverse: true,
			onChunk: (_, offset) => offsets.push(offset),
		});
		for (let i = 1; i < offsets.length; i++) {
			expect(offsets[i]).toBeGreaterThanOrEqual(offsets[i - 1]!);
		}
	});

	test("reassembled chunks match non-streaming output", () => {
		const value = { items: [1, "two", true], name: "test" };
		const direct = stringify(value, { reverse: true });
		const chunks: string[] = [];
		stringify(value, {
			reverse: true,
			onChunk: (chunk) => chunks.push(chunk),
		});
		const result = chunks.join("")
		expect(result).toBe(direct);
	});

});

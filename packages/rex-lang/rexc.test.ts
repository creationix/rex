import { describe, expect, test } from "bun:test";
import {
	fromB64,
	fromZigZag,
	parse,
	readB64,
	stringify,
	toB64,
	toZigZag,
	writeB64,
	get,
	encode,
	getEntries,
	getEach,
	makeContext,
	type RxArray,
	type RxObject,
} from "./rexc.ts";

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
		const values = [
			0x80000000,
			-0x80000001,
			0x100000000,
			-0x100000000,
			Number.MAX_SAFE_INTEGER,
		];
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
		const boundaries = [
			64,
			127,
			128,
			255,
			256,
			4095,
			4096,
			64 * 64 - 1,
			64 * 64,
			64 * 64 * 64 - 1,
			64 * 64 * 64,
		];
		for (const n of boundaries) {
			expect(fromB64(toB64(n))).toBe(n);
		}
	});

	test("round-trips large values", () => {
		const large = [
			100_000,
			1_000_000,
			16_777_216,
			268_435_456,
			Number.MAX_SAFE_INTEGER,
		];
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
		const values = [
			0,
			1,
			63,
			64,
			4095,
			4096,
			100_000,
			1_000_000,
			Number.MAX_SAFE_INTEGER,
		];
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
		expect(() => readB64(fromHex("6120"), 0, 2)).toThrow(
			"Invalid base64 character",
		);
	});

	test("agrees with fromB64 for all single-digit values", () => {
		const digits =
			"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";
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
			expect(stringify(1)).toBe("+2");
			expect(stringify(-1)).toBe("+1");
			expect(stringify(42)).toBe("+1k");
			expect(stringify(-42)).toBe("+1j");
		});

		test("encodes decimals", () => {
			expect(stringify(3.14)).toBe("+9Q*3");
			expect(stringify(0.5)).toBe("+a*1");
			expect(stringify(1000000)).toBe("+2*c");
		});

		test("encodes length-prefixed strings for non-bare characters", () => {
			expect(stringify("hello world")).toBe("hello world,b");
			expect(stringify("foo bar")).toBe("foo bar,7");
		});

		test("encodes booleans, null, undefined", () => {
			expect(stringify(true)).toBe("'t");
			expect(stringify(false)).toBe("'f");
			expect(stringify(null)).toBe("'n");
			expect(stringify(undefined)).toBe("'u");
		});

		test("encodes special numbers", () => {
			expect(stringify(NaN)).toBe("'nan");
			expect(stringify(Infinity)).toBe("'inf");
			expect(stringify(-Infinity)).toBe("'nif");
		});
	});

	describe("arrays", () => {
		test("encodes simple arrays", () => {
			expect(stringify([1, 2, 3])).toBe("+6+4+2;6");
		});

		test("encodes arrays as values with length prefix", () => {
			const encoded = stringify([[1, 2, 3]], {});
			expect(encoded).toBe("+6+4+2;6;8");
		});

		test("encodes empty array", () => {
			expect(stringify([])).toBe(";");
		});

		test("encodes nested arrays", () => {
			const encoded = stringify([[1], [2]]);
			expect(encoded).toBe("+4;2+2;2;8");
		});

		test("encodes arrays with different formats", () => {
			const data = [
				[1, 2],
				[3, 4],
			];
			expect(stringify(data)).toBe("+8+6;4+4+2;4;c");
			expect(stringify(data, { indexes: 0 })).toBe(
				"+8+602#g;8+4+202#g;80a#g;o",
			);
		});
	});

	describe("objects", () => {
		test("encodes simple objects", () => {
			expect(stringify({ color: "red", size: 42 })).toBe(
				"+1ksize,4red,3color,5:l",
			);
		});

		test("encodes empty object", () => {
			expect(stringify({})).toBe(":");
		});

		test("encodes objects with length prefix", () => {
			const encoded = stringify([{ a: 1 }]);
			// Should have a length prefix before {
			expect(encoded).toBe("+2a,1:5;7");
		});

		test("encodes objects with different formats", () => {
			const data = { a: { b: 1, c: 1 }, d: { e: 3, f: 4 } };
			expect(stringify(data)).toBe("+8f,1+6e,1:ad,1+2c,1+2b,1:aa,1:u");
			expect(stringify(data, { indexes: 0 })).toBe(
				"+8f,1+6e,105#g:ed,1+2c,1+2b,105#g:ea,10j#g:G",
			);
		});

		test("object keys are sorted when indexes enabled", () => {
			const obj = { c: 3, a: 1, b: 2 };
			const encoded = stringify(obj, { indexes: 2 });
			expect(encoded).toBe("+4b,1+2a,1+6c,15a0#o:k");
		});
	});

	describe("indexes", () => {
		test("embed index into small array", () => {
			const arr = [1, 2, 3];
			const encoded = stringify(arr, { indexes: 2 });
			expect(encoded).toBe("+6+4+2024#o;b");
		});
		test("embeds index for medium arrays", () => {
			const arr = Array.from({ length: 12 }, (_, i) => i);
			const encoded = stringify(arr, { indexes: 10 });
			expect(encoded).toBe("+m+k+i+g+e+c+a+8+6+4+2+013579bdfhjl#1w;C");
		});
		test("embeds index for large arrays", () => {
			const arr = Array.from({ length: 40 }, (_, i) => i);
			const encoded = stringify(arr, { indexes: 30 });
			expect(encoded).toBe(
				"+1e+1c+1a+18+16+14+12+10+-+Y+W+U+S+Q+O+M+K+I+G+E+C+A+y+w+u+s+q+o+m+k+i+g+e+c+a+8+6+4+2+0001030507090b0d0f0h0j0l0n0p0r0t0v0x0z0B0D0F0H0J0L0N0P0R0T0V0X0Z0_1215181b1e1h1k#51;2G",
			);
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

		test("indices for maps", () => {
			const obj = { a: 1, b: 2, c: 3 };
			const encoded = stringify(obj, { indexes: 2 });
			expect(encoded).toBe("+6c,1+4b,1+2a,105a#o:k");
		});

		test("map indexes sort keys", () => {
			const obj = { c: 3, a: 1, b: 2 };
			const encoded = stringify(obj, { indexes: 2 });
			expect(encoded).toBe("+4b,1+2a,1+6c,15a0#o:k");
		});

		test("schema objects can have indices on values", () => {
			const data = [
				{ name: "alice", age: 1 },
				{ name: "bob", age: 2 },
			];
			expect(stringify(data, { indexes: 1 })).toBe(
				"+4age,3bob,3name,4b0#g:m+2alice,507#g^d:f0h#g;J",
			);
			expect(stringify(data, { indexes: 1 })).toBe(
				"+4age,3bob,3name,4b0#g:m+2alice,507#g^d:f0h#g;J",
			);
		});
	});

	describe("pointers", () => {
		test("deduplicates repeated strings", () => {
			const encoded = stringify(["hello", "hello"]);
			expect(encoded).toBe("hello,5^;8");
		});

		test("deduplicates repeated objects", () => {
			const obj = { x: 1 };
			expect(stringify([obj, obj])).toBe("+2x,1:5^;8");
		});
	});

	describe("refs", () => {
		test("encodes value matching a ref as ref shorthand", () => {
			expect(
				stringify("hello", {
					refs: { H: "hello" }
				}),
			).toBe("'H");
		});

		test("encodes number matching a ref", () => {
			expect(stringify(42, { refs: { X: 42 } })).toBe("'X");
		});

		test("encodes refs inside arrays", () => {
			expect(stringify(["hello", "world"], { refs: { H: "hello" } })).toBe(
				"world,5'H;9",
			);
		});

		test("encodes multiple refs", () => {
			expect(
				stringify(["hello", 42], {
					refs: { H: "hello", X: 42 },
				}),
			).toBe("'X'H;4");
		});

		test("encodes schema ref for repeated object shapes", () => {
			const data = [
				{ a: 1, b: 2 },
				{ a: 3, b: 4 },
			];
			expect(
				stringify(data, {
					refs: { S: ["a", "b"] },
				}),
			).toBe("+8+6'S:6+4+2'S:6;g");
		});

		test("encodes refs in reverse mode", () => {
			expect(
				stringify("hello", {
					refs: { H: "hello" },
				}),
			).toBe("'H");
		});

		test("use refs even when pointers are disabled", () => {
			expect(
				stringify("hello", { refs: { H: "hello" } }),
			).toBe("'H");
		});
	});

	describe("shared schemas", () => {
		test("deduplicates repeated object shapes", () => {
			const data = [
				{ name: "alice", age: 1 },
				{ name: "bob", age: 2 },
				{ name: "charlie", age: 3 },
			];
			expect(stringify(data)).toBe(
				"+6age,3charlie,7name,4:m+4bob,3^7:9+2alice,5^k:b;M",
			);
		});

		test("does not use schemas for single objects", () => {
			const data = [{ name: "alice" }];
			const encoded = stringify(data);
			expect(encoded).toBe("alice,5name,4:d;f");
		});
	});

	describe("path chains", () => {
		test("encodes path chains with shared prefixes", () => {
			// Non-repeated prefixes should not use pathChains optimization
			expect(stringify("/")).toBe("/,1");
			expect(stringify("/about")).toBe("/about,6");
			const paths = ["/foo/bar/baz", "/foo/bar/qux", "/foo/quux"];
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

			expect(stringify(paths)).toBe(
				"/quux,5/foo,4.d/qux,4/bar,4^e.8.g/baz,4^8.8;H",
			);

			// The current implementaion breaks out `/foo` as a "duplicated" prefix.
			// But technically we could stop at `/foo/bar` as the root prefix,
			// but that requires changing the duplicate prefix detector to be more complex and cancel nested prefixes.
			// Also this form writes cleaner encodings since most path segments are b64 friendly.
			// The inner `/foo/bar` is currently `a/4/foo:bar:` when it could be `9/7,foo/bar`.
			// The "optimized" form is one less character and one less concat layer, but more ugly.
			const prefixedPaths = ["/foo/bar/baz", "/foo/bar/qux"];
			expect(stringify(prefixedPaths)).toBe("/qux,4/bar,4/foo,4.c.k/baz,4^8.8;w");
		});
	});

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
			expect(stringify(doc)).toBe(
				"GET,3method,6Export Logs as CSV,iname,4:D/csv,4/export,7/logs,5/admin,6.f.q.y^18Export Logs as JSON,j^Y:q/json,5^B.9^1LExport Logs,b^1r:j^-POST,4Clear Logs,a^1Q:l/clear,6^1x.b^2GAdmin Logs,a^2l:i^1W^RRemove User,b^2I:i/remove,7/users,6^2A.b.m^1xAdd User,8^3m:g/add,4^q.8^49Admin Users,b^3R:j^P^2kAdmin Settings,e^4f:m/settings,9^41.e^58Admin,5^4K:d^4l^3eAPI Update,a^55:i/update,7/api,4.f^5_API Data,8^5E:g/data,5^r.9^4gComment,7^64:f/post/comment,d/blog,5.m^75Blog Post,9^6L:h/post,5^s.9^7zBlog,4^78:c^K^5DContact,7^7r:f/contact,8^8eAbout,5^7Q:d/about,6^8BHome,4^8a:c/,1:8X",
			);
			expect(stringify(doc)).toBe(
				"GET,3method,6Export Logs as CSV,iname,4:D/csv,4/export,7/logs,5/admin,6.f.q.y^18Export Logs as JSON,j^Y:q/json,5^B.9^1LExport Logs,b^1r:j^-POST,4Clear Logs,a^1Q:l/clear,6^1x.b^2GAdmin Logs,a^2l:i^1W^RRemove User,b^2I:i/remove,7/users,6^2A.b.m^1xAdd User,8^3m:g/add,4^q.8^49Admin Users,b^3R:j^P^2kAdmin Settings,e^4f:m/settings,9^41.e^58Admin,5^4K:d^4l^3eAPI Update,a^55:i/update,7/api,4.f^5_API Data,8^5E:g/data,5^r.9^4gComment,7^64:f/post/comment,d/blog,5.m^75Blog Post,9^6L:h/post,5^s.9^7zBlog,4^78:c^K^5DContact,7^7r:f/contact,8^8eAbout,5^7Q:d/about,6^8BHome,4^8a:c/,1:8X",
			);
			expect(
				stringify(doc, {
					indexes: false,
				}),
			).toBe(
				"GET,3method,6Export Logs as CSV,iname,4:D/csv,4/export,7/logs,5/admin,6.f.q.y^18Export Logs as JSON,j^Y:q/json,5^B.9^1LExport Logs,b^1r:j^-POST,4Clear Logs,a^1Q:l/clear,6^1x.b^2GAdmin Logs,a^2l:i^1W^RRemove User,b^2I:i/remove,7/users,6^2A.b.m^1xAdd User,8^3m:g/add,4^q.8^49Admin Users,b^3R:j^P^2kAdmin Settings,e^4f:m/settings,9^41.e^58Admin,5^4K:d^4l^3eAPI Update,a^55:i/update,7/api,4.f^5_API Data,8^5E:g/data,5^r.9^4gComment,7^64:f/post/comment,d/blog,5.m^75Blog Post,9^6L:h/post,5^s.9^7zBlog,4^78:c^K^5DContact,7^7r:f/contact,8^8eAbout,5^7Q:d/about,6^8BHome,4^8a:c/,1:8X",
			);
		});
	});

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
			expect(stringify(doc)).toBe(
				"flags,5group,5pirate flag,bname,4:x/🏴‍☠️,e/emoji,6.osmileys-emotion,fred heart,9^S:u/❤️,7^H.bactivities,asoccer ball,b^1w:s/⚽,4^1j.9objects,7guitar,6^21:k/🎸,5^1R.aanimals-nature,esnake,5^2F:q/🐍,5^2t.a^oseedling,8^36:f/🌱,5^2W.atravel-places,dwater,5^3J:p/💧,5^3x.a^ofire,4^46:b/🔥,5^3W.a:4W",
			);
			expect(stringify(doc, { chainSplit: false })).toBe(
				"flags,5group,5pirate flag,bname,4:x/emoji/🏴‍☠️,ksmileys-emotion,fred heart,9^O:u/emoji/❤️,dactivities,asoccer ball,b^1u:s/emoji/⚽,aobjects,7guitar,6^20:k/emoji/🎸,banimals-nature,esnake,5^2F:q/emoji/🐍,b^pseedling,8^37:f/emoji/🌱,btravel-places,dwater,5^3L:p/emoji/💧,b^pfire,4^49:b/emoji/🔥,b:4-",
			);
		});
	});

	describe("encode colored fruits", () => {
		const doc = [
			{ color: "red", fruits: ["apple", "strawberry"] },
			{ color: "green", fruits: ["apple"] },
			{ color: "yellow", fruits: ["apple", "banana"] },
			{ color: "orange", fruits: ["orange"] },
		];
		test("with correct options applied", () => {
			expect(stringify(doc)).toBe(
				"orange,6;8fruits,6^acolor,5:rbanana,6apple,5;fyellow,6^p:r^e;2green,5^E:dstrawberry,a^F;ered,3^11:o;1z",
			);
			expect(stringify(doc, {})).toBe(
				"orange,6;8fruits,6^acolor,5:rbanana,6apple,5;fyellow,6^p:r^e;2green,5^E:dstrawberry,a^F;ered,3^11:o;1z",
			);
			expect(stringify(doc)).toBe(
				"orange,6;8fruits,6^acolor,5:rbanana,6apple,5;fyellow,6^p:r^e;2green,5^E:dstrawberry,a^F;ered,3^11:o;1z",
			);
		});
	});
});

describe("rexc parse", () => {
	describe("primitives", () => {
		test("parses integers", () => {
			expect(parse("+")).toBe(0);
			expect(parse("+2")).toBe(1);
			expect(parse("+1")).toBe(-1);
			expect(parse("+1k")).toBe(42);
			expect(parse("+1j")).toBe(-42);
		});

		test("parses decimals", () => {
			expect(parse("+9Q*3")).toBe(3.14);
			expect(parse("+a*1")).toBe(0.5);
		});

		test("parses strings", () => {
			expect(parse(",")).toBe("");
			expect(parse("hello world,b")).toBe("hello world");
			expect(parse("foo bar,7")).toBe("foo bar");
		});

		test("parses booleans, null, undefined", () => {
			expect(parse("'t")).toBe(true);
			expect(parse("'f")).toBe(false);
			expect(parse("'n")).toBe(null);
			expect(parse("'u")).toBe(undefined);
		});

		test("parses special numbers", () => {
			expect(parse("'nan")).toBeNaN();
			expect(parse("'inf")).toBe(Infinity);
			expect(parse("'nif")).toBe(-Infinity);
		});
	});

	describe("arrays", () => {
		test("parses simple arrays", () => {
			expect(parse("+6+4+2;6")).toEqual([1, 2, 3]);
		});

		test("parses empty array", () => {
			expect(parse(";")).toEqual([]);
		});
	});

	describe("objects", () => {
		test("parses simple objects", () => {
			expect(parse("+1ksize,4red,3color,5:l")).toEqual({ color: "red", size: 42 });
		});

		test("parses empty object", () => {
			expect(parse(":")).toEqual({});
		});
	});

	test("resolves pointer references", () => {
		expect(parse("hello,5^;8")).toEqual(["hello", "hello"]);
	});

	describe("refs", () => {
		test("resolves ref references", () => {
			expect(parse("'H", { refs: { H: "hello" } })).toBe("hello");
		});
	});

	describe("lazy mode", () => {
		test("decodes properly with and without lazy mode", () => {
			expect(parse("+4b,1+2a,1:a", { lazy: false })).toEqual({ a: 1, b: 2 });
			expect(parse("+4b,1+2a,1:a", { lazy: true })).toEqual({ a: 1, b: 2 });
		});
	});
});

describe("rexc round-trip", () => {
	const roundTrip = (
		value: unknown,
		opts?: {
			refs?: Record<string, unknown>;
			indexes?: number | false;
			chainSplit?: string | false;
			lazy?: boolean;
		},
	) => {
		const encoded = stringify(value, opts);
		return parse(encoded, opts);
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
		expect(
			roundTrip([
				[1, 2],
				[3, 4],
			]),
		).toEqual([
			[1, 2],
			[3, 4],
		]);
	});

	test("round-trips objects", () => {
		expect(roundTrip({})).toEqual({});
		expect(roundTrip({ a: 1, b: 2 })).toEqual({ a: 1, b: 2 });
		expect(
			roundTrip({ name: "rex", nested: { ok: true } }))
			.toEqual({ name: "rex", nested: { ok: true } });
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

	test("round-trips with path chains", () => {
		const value = {
			paths: [
				"/docs/api/v2/users",
				"/docs/api/v2/teams",
				"/docs/api/v2/billing",
			],
			config: { retries: 3, timeout: 30 },
		};
		expect(roundTrip(value)).toEqual(value);
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
		stringify(
			{ a: 1 },
			{
				onChunk: (data, offset) => chunks.push({ offset, data }),
			},
		);
		expect(chunks).toEqual([
			{
				offset: 0,
				data: "+2",
			},
			{
				offset: 2,
				data: "a",
			},
			{
				offset: 3,
				data: ",1",
			},
			{
				offset: 5,
				data: ":5",
			},
		]);
	});

	test("onChunk offsets are increasing in reverse mode", () => {
		const offsets: number[] = [];
		stringify([1, 2, 3, "hello", { a: true }], {
			onChunk: (_, offset) => offsets.push(offset),
		});
		for (let i = 1; i < offsets.length; i++) {
			expect(offsets[i]).toBeGreaterThanOrEqual(offsets[i - 1]!);
		}
	});

	test("reassembled chunks match non-streaming output", () => {
		const value = { items: [1, "two", true], name: "test" };
		const direct = stringify(value);
		const chunks: string[] = [];
		stringify(value, {
			onChunk: (chunk) => chunks.push(chunk),
		});
		const result = chunks.join("");
		expect(result).toBe(direct);
	});
});

describe("rexc reading", () => {
	test("get returns correct metadata for primitives", () => {
		expect(get(encode(0))).toEqual({
			// `+`
			type: "primitive",
			left: 0,
			right: 1,
			value: 0,
		});
		expect(get(encode(42))).toEqual({
			// `+1k`
			type: "primitive",
			left: 0,
			right: 3,
			value: 42,
		});
		expect(get(encode(-42))).toEqual({
			// `+1j`
			type: "primitive",
			left: 0,
			right: 3,
			value: -42,
		});
		expect(get(encode(3.14))).toEqual({
			// `+9Q*3`
			type: "primitive",
			left: 0,
			right: 5,
			value: 3.14,
		});
		expect(get(encode(""))).toEqual({
			// `,`
			type: "primitive",
			left: 0,
			right: 1,
			value: "",
		});
		expect(get(encode("hello"))).toEqual({
			// `hello,5`
			type: "primitive",
			left: 0,
			right: 7,
			value: "hello",
		});
		expect(get(encode("content-type"))).toEqual({
			// `content-type,c`
			type: "primitive",
			left: 0,
			right: 14,
			value: "content-type",
		});
		expect(get(encode("🏴‍☠️"))).toEqual({
			// `🏴‍☠️,d`
			type: "primitive",
			left: 0,
			right: 15,
			value: "🏴‍☠️",
		});
		expect(get(encode("🚀"))).toEqual({
			// `🚀,4`
			type: "primitive",
			left: 0,
			right: 6,
			value: "🚀",
		});
		expect(get(encode(true))).toEqual({
			// `'t`
			type: "primitive",
			left: 0,
			right: 2,
			value: true,
		});
		expect(get(encode(false))).toEqual({
			// `'f`
			type: "primitive",
			left: 0,
			right: 2,
			value: false,
		});
		expect(get(encode(null))).toEqual({
			// `'n`
			type: "primitive",
			left: 0,
			right: 2,
			value: null,
		});
		expect(get(encode(undefined))).toEqual({
			// `'u`
			type: "primitive",
			left: 0,
			right: 2,
			value: undefined,
		});
		expect(get(encode(NaN))).toEqual({
			// `'nan`
			type: "primitive",
			left: 0,
			right: 4,
			value: NaN,
		});
		expect(get(encode(Infinity))).toEqual({
			// `'inf`
			type: "primitive",
			left: 0,
			right: 4,
			value: Infinity,
		});
		expect(get(encode(-Infinity))).toEqual({
			// `'nif`
			type: "primitive",
			left: 0,
			right: 4,
			value: -Infinity,
		});
	});

	test("get returns correct metadata for arrays and objects", () => {
		expect(get(encode([1, 2, 3]))).toEqual({
			// `+6+4+2;6`
			type: "array",
			left: 0,
			right: 8,
			content: 6,
		});
		expect(get(encode({ a: 1, b: 2 }))).toEqual({
			// `+4b,1+2a,1:a`
			type: "object",
			left: 0,
			right: 12,
			content: 10,
		});
		expect(get(encode([1, 2, 3], { indexes: 0 }))).toEqual({
			// `+6+4+2024#o;b`
			type: "array",
			left: 0,
			right: 13,
			content: 6,
			index: {
				width: 1,
				count: 3,
			},
		});
		expect(get(encode({ a: 1, b: 2 }, { indexes: 1 }))).toEqual({
			// `+4b,1+2a,105#g:e`
			type: "object",
			left: 0,
			right: 16,
			content: 10,
			index: {
				width: 1,
				count: 2,
			},
		});
		expect(get(encode({ a: 1, b: 2 }, { refs: { K: ["a", "b"] } }))).toEqual({
			// `+4+2'K:6`
			type: "object",
			left: 0,
			right: 8,
			content: 4,
			schema: "K",
		});
		expect(
			get(
				encode([
					{ a: 1, b: 2 },
					{ a: 3, b: 4 },
				]),
			),
		).toEqual({
			// `+8b,1+6a,1:a+4+2^4:6;k`
			type: "array",
			left: 0,
			right: 22,
			content: 20,
		});
		expect(
			get(
				encode([
					{ a: 1, b: 2 },
					{ a: 3, b: 4 },
				]),
				20,
			),
		).toEqual({
			// `+8b,1+6a,1:a+4+2^4:6;k`
			type: "object",
			left: 12,
			right: 20,
			content: 16,
			schema: 12,
		});
	});

	test("getEntries and getValues return correct metadata for arrays and objects", () => {
		let context = makeContext(encode({ a: 1, b: 2 }));
		expect([...getEntries(context, get(context.data) as RxObject)]).toEqual([
			// `+4b,1+2a,1:a`
			["a", {
				type: "primitive",
				left: 5,
				right: 7,
				value: 1,
			}],
			["b", {
				type: "primitive",
				left: 0,
				right: 2,
				value: 2,
			}],
		]);

		context = makeContext(encode([1, 2, 3]));
		expect([...getEach(context, get(context.data) as RxArray)]).toEqual([
			// `+6+4+2;6`
			{
				type: "primitive",
				left: 4,
				right: 6,
				value: 1,
			},
			{
				type: "primitive",
				left: 2,
				right: 4,
				value: 2,
			},
			{
				type: "primitive",
				left: 0,
				right: 2,
				value: 3,
			},
		])
	});
});

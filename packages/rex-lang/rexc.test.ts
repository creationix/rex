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
			expect(stringify("")).toBe(":");
			expect(stringify("hello")).toBe("hello:");
			expect(stringify("x-action")).toBe("x-action:");
		});

		test("encodes length-prefixed strings for non-bare characters", () => {
			expect(stringify("hello world")).toBe("b,hello world");
			expect(stringify("foo bar")).toBe("7,foo bar");
		});

		test("encodes booleans, null, undefined", () => {
			expect(stringify(true)).toBe("tr'");
			expect(stringify(false)).toBe("fl'");
			expect(stringify(null)).toBe("nl'");
			expect(stringify(undefined)).toBe("un'");
		});

		test("encodes special numbers", () => {
			expect(stringify(NaN)).toBe("nan'");
			expect(stringify(Infinity)).toBe("inf'");
			expect(stringify(-Infinity)).toBe("nif'");
		});
	});

	describe("arrays", () => {
		test("encodes simple arrays", () => {
			expect(stringify([1, 2, 3])).toBe("[2+4+6+]");
		});

		test("encodes arrays as values with length prefix when randomAccess is true", () => {
			const encoded = stringify([[1, 2, 3]], { randomAccess: true });
			expect(encoded).toBe("[6[2+4+6+]]")
			const encoded2 = stringify([[1, 2, 3]], { randomAccess: false });
			expect(encoded2).toBe("[[2+4+6+]]");
		});

		test("encodes empty array", () => {
			expect(stringify([])).toBe("[]");
		});

		test("encodes nested arrays", () => {
			const encoded = stringify([[1], [2]], { randomAccess: false });
			expect(encoded).toBe("[[2+][4+]]");
		});
	});

	describe("objects", () => {
		test("encodes simple objects", () => {
			expect(stringify({ color: "red", size: 42 }, { randomAccess: false })).toBe(
				"{color:red:size:1k+}",
			);
		});

		test("encodes empty object", () => {
			expect(stringify({})).toBe("{}");
		});

		test("encodes objects with length prefix when randomAccess is true", () => {
			const encoded = stringify([{ a: 1 }], { randomAccess: true });
			// Should have a length prefix before {
			expect(encoded).toBe("[4{a:2+}]")
			const encoded2 = stringify([{ a: 1 }], { randomAccess: false });
			expect(encoded2).toBe("[{a:2+}]");
		});
	});

	describe.skip("indexes", () => {
		test("embeds index for large arrays", () => {
			const arr = Array.from({ length: 12 }, (_, i) => i);
			const encoded = stringify(arr, { indexes: 10 });
			expect(encoded).toContain("#");
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
	});

	describe("pointers", () => {
		test("deduplicates repeated strings", () => {
			const encoded = stringify(["hello", "hello"], { randomAccess: false });
			expect(encoded).toBe("[^hello:]")
		});

		test("deduplicates repeated objects", () => {
			const obj = { x: 1 };
			const encoded = stringify([obj, obj], { randomAccess: false });
			expect(encoded).toBe("[^{x:2+}]");
			const encoded2 = stringify([obj, obj], { randomAccess: true });
			expect(encoded2).toBe("[^4{x:2+}]");
		});

		test("does not deduplicate when pointers disabled", () => {
			const encoded = stringify(["hello", "hello"], { pointers: false, randomAccess: false });
			expect(encoded).toBe("[hello:hello:]");
		});
	});

	describe("shared schemas", () => {
		test("deduplicates repeated object shapes", () => {
			const data = [
				{ name: "alice", age: 1 },
				{ name: "bob", age: 2 },
				{ name: "charlie", age: 3 },
			];
			const withSchemas = stringify(data, { schemas: true, randomAccess: false });
			const withoutSchemas = stringify(data, { schemas: false, randomAccess: false });
			expect(withSchemas).toBe("[{j^alice:2+}{7^bob:4+}{name:charlie:age:6+}]")
			expect(withoutSchemas).toBe("[{o^alice:t^2+}{a^bob:h^4+}{name:charlie:age:6+}]")
		});

		test("does not use schemas for single objects", () => {
			const data = [{ name: "alice" }];
			const encoded = stringify(data, { schemas: true, randomAccess: false });
			expect(encoded).toBe("[{name:alice:}]")
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
			.toBe("[6/6^baz:c/6/a^bar:qux:b/4/foo:quux:]")
		// Reverse mode is the same thing in reverse
		expect(stringify(paths, { pathChains: true, pointers: true, schemas: false, reverse: true }))
			.toBe("[:quux:foo/4/b:qux:bar^a/6/c:baz^6/6]")

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
			.toBe("[6/6^baz:g/a/4/foo:bar:qux:]")
		// Reverse mode is the same thing in reverse
		expect(stringify(prefixedPaths, { pathChains: true, pointers: true, schemas: false, reverse: true }))
			.toBe("[:qux:bar:foo/4/a/g:baz^6/6]")
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
			expect(stringify(doc, { pathChains: true, randomAccess: false, pointers: true, schemas: true, reverse: false }))
				.toBe("{1,/{7H^Home:84^}6,/about{7m^About:7K^}8,/contact{6-^Contact:5j^}H^{6I^Blog:75^}7/q^post:{6m^9,Blog Post6F^}l/5/blog:c,post/comment{5I^Comment:41^}7/p^data:{5j^8,API Data5D^}d/4/api:update:{4O^a,API Update33^}47^{4r^Admin:4P^}c/3Q^settings:{3_^e,Admin Settings2c^}N^{3B^b,Admin Users3S^}6/o^add:{38^8,Add User1r^}i/9/2r^users:remove:{2y^b,Remove UserP^}1R^{2b^a,Admin Logs2t^}9/1s^clear:{1I^a,Clear LogsPOST:}Y^{1k^b,Export Logs1B^}7/z^json:{T^j,Export Logs as JSON10^}s/m/d/6/admin:logs:export:csv:{name:i,Export Logs as CSVmethod:GET:}}");
			expect(stringify(doc, { pathChains: true, randomAccess: false, pointers: true, schemas: true, reverse: true }))
				.toBe("{{:GET:methodExport Logs as CSV,i:name}:csv:export:logs:admin/6/d/m/s{^10Export Logs as JSON,j^T}:json^z/7{^1BExport Logs,b^1k}^Y{:POSTClear Logs,a^1I}:clear^1s/9{^2tAdmin Logs,a^2b}^1R{^PRemove User,b^2y}:remove:users^2r/9/i{^1rAdd User,8^38}:add^o/6{^3SAdmin Users,b^3B}^N{^2cAdmin Settings,e^3_}:settings^3Q/c{^4P:Admin^4r}^47{^33API Update,a^4O}:update:api/4/d{^5DAPI Data,8^5j}:data^p/7{^41:Comment^5I}post/comment,c:blog/5/l{^6FBlog Post,9^6m}:post^q/7{^75:Blog^6I}^H{^5j:Contact^6-}/contact,8{^7K:About^7m}/about,6{^84:Home^7H}/,1}");
			expect(stringify(doc, { pathChains: true, randomAccess: true, pointers: true, schemas: true, reverse: false }))
				.toBe("{1,/b{7X^Home:8l^}6,/aboutc{7B^About:7-^}8,/contacte{7c^Contact:5v^}J^b{6V^Blog:7j^}7/r^post:h{6y^9,Blog Post6S^}l/5/blog:c,post/commente{5T^Comment:4a^}7/q^data:g{5t^8,API Data5O^}d/4/api:update:i{4X^a,API Update3a^}4g^c{4z^Admin:4Y^}c/3Y^settings:m{46^e,Admin Settings2h^}P^j{3H^b,Admin Users3Z^}6/p^add:g{3d^8,Add User1u^}i/9/2w^users:remove:i{2C^b,Remove UserR^}1V^i{2e^a,Admin Logs2x^}9/1v^clear:k{1K^a,Clear LogsPOST:}-^j{1l^b,Export Logs1D^}7/A^json:q{T^j,Export Logs as JSON11^}s/m/d/6/admin:logs:export:csv:A{name:i,Export Logs as CSVmethod:GET:}}");
			expect(stringify(doc, { pathChains: true, randomAccess: true, pointers: true, schemas: true, reverse: true }))
				.toBe("{{:GET:methodExport Logs as CSV,i:name}A:csv:export:logs:admin/6/d/m/s{^11Export Logs as JSON,j^T}q:json^A/7{^1DExport Logs,b^1l}j^-{:POSTClear Logs,a^1K}k:clear^1v/9{^2xAdmin Logs,a^2e}i^1V{^RRemove User,b^2C}i:remove:users^2w/9/i{^1uAdd User,8^3d}g:add^p/6{^3ZAdmin Users,b^3H}j^P{^2hAdmin Settings,e^46}m:settings^3Y/c{^4Y:Admin^4z}c^4g{^3aAPI Update,a^4X}i:update:api/4/d{^5OAPI Data,8^5t}g:data^q/7{^4a:Comment^5T}epost/comment,c:blog/5/l{^6SBlog Post,9^6y}h:post^r/7{^7j:Blog^6V}b^J{^5v:Contact^7c}e/contact,8{^7-:About^7B}c/about,6{^8l:Home^7X}b/,1}");
			expect(stringify(doc, { pathChains: true, randomAccess: true, pointers: true, schemas: false, reverse: false }))
				.toBe("{1,/e{8M^Home:91^95^}6,/aboutf{8n^About:8D^8H^}8,/contacth{7X^Contact:89^63^}P^e{7B^Blog:7S^7W^}7/u^post:k{7b^9,Blog Post7m^7q^}l/5/blog:c,post/commenth{6t^Comment:6H^4B^}7/t^data:j{60^8,API Data6c^6g^}d/4/api:update:l{5r^a,API Update5B^3v^}4H^f{50^Admin:5g^5k^}c/4k^settings:p{4w^e,Admin Settings4C^2w^}V^m{42^b,Admin Users4b^4f^}6/s^add:j{3x^8,Add User3J^1D^}i/9/2L^users:remove:l{2T^b,Remove User30^X^}25^l{2s^a,Admin Logs2C^2G^}9/1E^clear:n{1V^a,Clear Logs23^POST:}13^m{1s^b,Export Logs1B^1F^}7/C^json:s{X^j,Export Logs as JSONZ^11^}s/m/d/6/admin:logs:export:csv:A{name:i,Export Logs as CSVmethod:GET:}}");
			expect(stringify(doc, { pathChains: true, randomAccess: true, pointers: true, schemas: false, reverse: true }))
				.toBe("{{:GET:methodExport Logs as CSV,i:name}A:csv:export:logs:admin/6/d/m/s{^11^ZExport Logs as JSON,j^X}s:json^C/7{^1F^1BExport Logs,b^1s}m^13{:POST^23Clear Logs,a^1V}n:clear^1E/9{^2G^2CAdmin Logs,a^2s}l^25{^X^30Remove User,b^2T}l:remove:users^2L/9/i{^1D^3JAdd User,8^3x}j:add^s/6{^4f^4bAdmin Users,b^42}m^V{^2w^4CAdmin Settings,e^4w}p:settings^4k/c{^5k^5g:Admin^50}f^4H{^3v^5BAPI Update,a^5r}l:update:api/4/d{^6g^6cAPI Data,8^60}j:data^t/7{^4B^6H:Comment^6t}hpost/comment,c:blog/5/l{^7q^7mBlog Post,9^7b}k:post^u/7{^7W^7S:Blog^7B}e^P{^63^89:Contact^7X}h/contact,8{^8H^8D:About^8n}f/about,6{^95^91:Home^8M}e/,1}");
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
			expect(stringify(doc, { pathChains: false, randomAccess: false, pointers: false, schemas: false, reverse: false }))
				.toBe("{b,/emoji/🔥{name:fire:group:travel-places:}b,/emoji/💧{name:water:group:travel-places:}b,/emoji/🌱{name:seedling:group:animals-nature:}b,/emoji/🐍{name:snake:group:animals-nature:}b,/emoji/🎸{name:guitar:group:objects:}a,/emoji/⚽{name:b,soccer ballgroup:activities:}d,/emoji/❤️{name:9,red heartgroup:smileys-emotion:}k,/emoji/🏴‍☠️{name:b,pirate flaggroup:flags:}}");
			expect(stringify(doc, { pathChains: false, randomAccess: true, pointers: true, schemas: true, reverse: false }))
				.toBe("{b,/emoji/🔥a{46^fire:p^}b,/emoji/💧n{3I^water:travel-places:}b,/emoji/🌱e{35^seedling:p^}b,/emoji/🐍o{2D^snake:animals-nature:}b,/emoji/🎸i{1_^guitar:objects:}a,/emoji/⚽r{1u^b,soccer ballactivities:}d,/emoji/❤️t{O^9,red heartsmileys-emotion:}k,/emoji/🏴‍☠️u{name:b,pirate flaggroup:flags:}}");
			expect(stringify(doc, { pathChains: false, randomAccess: true, pointers: true, schemas: true, reverse: true }))
				.toBe("{{:flags:grouppirate flag,b:name}u/emoji/🏴‍☠️,k{:smileys-emotionred heart,9^O}t/emoji/❤️,d{:activitiessoccer ball,b^1u}r/emoji/⚽,a{:objects:guitar^1_}i/emoji/🎸,b{:animals-nature:snake^2D}o/emoji/🐍,b{^p:seedling^35}e/emoji/🌱,b{:travel-places:water^3I}n/emoji/💧,b{^p:fire^46}a/emoji/🔥,b}");
			expect(stringify(doc, { pathChains: true, randomAccess: true, pointers: true, schemas: false, reverse: false }))
				.toBe("{9/44^4,🔥d{4i^fire:4s^q^}9/3F^4,💧q{3T^water:40^travel-places:}9/31^4,🌱h{3f^seedling:3l^q^}9/2y^4,🐍r{2M^snake:2V^animals-nature:}9/1V^4,🎸l{27^guitar:2f^objects:}8/1m^3,⚽u{1B^b,soccer ball1D^activities:}a/I^6,❤️v{V^9,red heart-^smileys-emotion:}n/6/emoji:d,🏴‍☠️u{name:b,pirate flaggroup:flags:}}");
			expect(stringify(doc, { pathChains: true, randomAccess: true, pointers: true, schemas: false, reverse: true }))
				.toBe("{{:flags:grouppirate flag,b:name}u🏴‍☠️,d:emoji/6/n{:smileys-emotion^-red heart,9^V}v❤️,6^I/a{:activities^1Dsoccer ball,b^1B}u⚽,3^1m/8{:objects^2f:guitar^27}l🎸,4^1V/9{:animals-nature^2V:snake^2M}r🐍,4^2y/9{^q^3l:seedling^3f}h🌱,4^31/9{:travel-places^40:water^3T}q💧,4^3F/9{^q^4s:fire^4i}d🔥,4^44/9}");
			expect(stringify(doc, { pathChains: true, randomAccess: true, pointers: true, schemas: true, reverse: false }))
				.toBe("{9/3M^4,🔥a{3Y^fire:n^}9/3o^4,💧n{3A^water:travel-places:}9/2P^4,🌱e{2_^seedling:n^}9/2n^4,🐍o{2z^snake:animals-nature:}9/1N^4,🎸i{1Z^guitar:objects:}8/1h^3,⚽r{1u^b,soccer ballactivities:}a/G^6,❤️t{R^9,red heartsmileys-emotion:}n/6/emoji:d,🏴‍☠️u{name:b,pirate flaggroup:flags:}}");
			expect(stringify(doc, { pathChains: true, randomAccess: true, pointers: true, schemas: true, reverse: true }))
				.toBe("{{:flags:grouppirate flag,b:name}u🏴‍☠️,d:emoji/6/n{:smileys-emotionred heart,9^R}t❤️,6^G/a{:activitiessoccer ball,b^1u}r⚽,3^1h/8{:objects:guitar^1Z}i🎸,4^1N/9{:animals-nature:snake^2z}o🐍,4^2n/9{^n:seedling^2_}e🌱,4^2P/9{:travel-places:water^3A}n💧,4^3o/9{^n:fire^3Y}a🔥,4^3M/9}");
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
			expect(stringify(doc, { randomAccess: false, pointers: false, schemas: false, reverse: false }))
				.toBe("[{color:red:fruits:[apple:strawberry:]}{color:green:fruits:[apple:]}{color:yellow:fruits:[apple:banana:]}{color:orange:fruits:[orange:]}]")
			expect(stringify(doc, { randomAccess: false, pointers: false, schemas: false, reverse: true }))
				.toBe("[{[:orange]:fruits:orange:color}{[:banana:apple]:fruits:yellow:color}{[:apple]:fruits:green:color}{[:strawberry:apple]:fruits:red:color}]")
			expect(stringify(doc, { randomAccess: true, pointers: false, schemas: false, reverse: false }))
				.toBe("[B{color:red:fruits:h[apple:strawberry:]}s{color:green:fruits:6[apple:]}A{color:yellow:fruits:d[apple:banana:]}u{color:orange:fruits:7[orange:]}]")
			expect(stringify(doc, { randomAccess: true, pointers: false, schemas: false, reverse: true }))
				.toBe("[{[:orange]7:fruits:orange:color}u{[:banana:apple]d:fruits:yellow:color}A{[:apple]6:fruits:green:color}s{[:strawberry:apple]h:fruits:red:color}B]")
			expect(stringify(doc, { randomAccess: false, pointers: true, schemas: false, reverse: false }))
				.toBe("[{14^red:15^[G^strawberry:]}{G^green:G^[f^]}{q^yellow:p^[apple:banana:]}{color:8^fruits:[orange:]}]")
			expect(stringify(doc, { randomAccess: false, pointers: true, schemas: false, reverse: true }))
				.toBe("[{[:orange]:fruits^8:color}{[:banana:apple]^p:yellow^q}{[^f]^G:green^G}{[:strawberry^G]^15:red^14}]")
			expect(stringify(doc, { randomAccess: true, pointers: true, schemas: false, reverse: false }))
				.toBe("[q{1a^red:1b^d[K^strawberry:]}f{K^green:K^2[h^]}r{s^yellow:r^d[apple:banana:]}p{color:9^fruits:7[orange:]}]")
			expect(stringify(doc, { randomAccess: true, pointers: true, schemas: false, reverse: true }))
				.toBe("[{[:orange]7:fruits^9:color}p{[:banana:apple]d^r:yellow^s}r{[^h]2^K:green^K}f{[:strawberry^K]d^1b:red^1a}q]")
			expect(stringify(doc, { randomAccess: false, pointers: false, schemas: true, reverse: false }))
				.toBe("[{14^red:[apple:strawberry:]}{F^green:[apple:]}{n^yellow:[apple:banana:]}{color:orange:fruits:[orange:]}]")
			expect(stringify(doc, { randomAccess: false, pointers: false, schemas: true, reverse: true }))
				.toBe("[{[:orange]:fruits:orange:color}{[:banana:apple]:yellow^n}{[:apple]:green^F}{[:strawberry:apple]:red^14}]")
			expect(stringify(doc, { randomAccess: true, pointers: false, schemas: true, reverse: false }))
				.toBe("[r{19^red:h[apple:strawberry:]}h{I^green:6[apple:]}p{o^yellow:d[apple:banana:]}u{color:orange:fruits:7[orange:]}]")
			expect(stringify(doc, { randomAccess: true, pointers: false, schemas: true, reverse: true }))
				.toBe("[{[:orange]7:fruits:orange:color}u{[:banana:apple]d:yellow^o}p{[:apple]6:green^I}h{[:strawberry:apple]h:red^19}r]")
			expect(stringify(doc, { randomAccess: false, pointers: true, schemas: true, reverse: false }))
				.toBe("[{Y^red:[C^strawberry:]}{B^green:[d^]}{n^yellow:[apple:banana:]}{color:8^fruits:[orange:]}]")
			expect(stringify(doc, { randomAccess: false, pointers: true, schemas: true, reverse: true }))
				.toBe("[{[:orange]:fruits^8:color}{[:banana:apple]:yellow^n}{[^d]:green^B}{[:strawberry^C]:red^Y}]")
			expect(stringify(doc, { randomAccess: true, pointers: true, schemas: true, reverse: false }))
				.toBe("[n{11^red:d[G^strawberry:]}d{E^green:2[f^]}p{o^yellow:d[apple:banana:]}p{color:9^fruits:7[orange:]}]")
			expect(stringify(doc, { randomAccess: true, pointers: true, schemas: true, reverse: true }))
				.toBe("[{[:orange]7:fruits^9:color}p{[:banana:apple]d:yellow^o}p{[^f]2:green^E}d{[:strawberry^G]d:red^11}n]")
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
				randomAccess: true,
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
				randomAccess: false,
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
				data: "{",
			}, {
				offset: 1,
				data: "+2",
			}, {
				offset: 3,
				data: ":a",
			}, {
				offset: 5,
				data: "}",
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

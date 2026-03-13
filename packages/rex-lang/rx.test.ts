import { describe, expect, test } from "bun:test";
import { encode } from "./rexc";
import {
	makeCursor,
	read,
	readStr,
	resolveStr,
	strEquals,
	strCompare,
	findKey,
	seekChild,
	collectChildren,
	rawBytes,
} from "./rx";

function cur(value: unknown, opts?: Parameters<typeof encode>[1]) {
	const data = encode(value, opts);
	const c = makeCursor(data);
	read(c);
	return c;
}

describe("read() primitives", () => {
	test("integers", () => {
		let c = cur(0);
		expect(c.tag).toBe("int");
		expect(c.val).toBe(0);

		c = cur(42);
		expect(c.tag).toBe("int");
		expect(c.val).toBe(42);

		c = cur(-42);
		expect(c.tag).toBe("int");
		expect(c.val).toBe(-42);
	});

	test("floats", () => {
		let c = cur(3.14);
		expect(c.tag).toBe("float");
		expect(c.val).toBe(3.14);

		c = cur(0.5);
		expect(c.tag).toBe("float");
		expect(c.val).toBe(0.5);
	});

	test("special floats", () => {
		let c = cur(Infinity);
		expect(c.tag).toBe("float");
		expect(c.val).toBe(Infinity);

		c = cur(-Infinity);
		expect(c.tag).toBe("float");
		expect(c.val).toBe(-Infinity);

		c = cur(NaN);
		expect(c.tag).toBe("float");
		expect(c.val).toBeNaN();
	});

	test("strings", () => {
		let c = cur("");
		expect(c.tag).toBe("str");
		expect(c.val).toBe(0);
		expect(readStr(c)).toBe("");

		c = cur("hello");
		expect(c.tag).toBe("str");
		expect(c.val).toBe(5);
		expect(readStr(c)).toBe("hello");

		c = cur("hello world");
		expect(c.tag).toBe("str");
		expect(readStr(c)).toBe("hello world");
	});

	test("unicode strings", () => {
		const c = cur("🚀");
		expect(c.tag).toBe("str");
		expect(readStr(c)).toBe("🚀");
	});

	test("booleans, null, undefined", () => {
		expect(cur(true).tag).toBe("true");
		expect(cur(false).tag).toBe("false");
		expect(cur(null).tag).toBe("null");
		expect(cur(undefined).tag).toBe("undef");
	});
});

describe("read() containers", () => {
	test("empty array", () => {
		const c = cur([]);
		expect(c.tag).toBe("array");
		expect(c.val).toBe(c.left); // no content
	});

	test("simple array", () => {
		const c = cur([1, 2, 3]);
		expect(c.tag).toBe("array");
		// Iterate children
		const vals: number[] = [];
		let right = c.val;
		const tmp = makeCursor(c.data);
		while (right > c.left) {
			tmp.right = right;
			read(tmp);
			expect(tmp.tag).toBe("int");
			vals.push(tmp.val);
			right = tmp.left;
		}
		expect(vals).toEqual([1, 2, 3]);
	});

	test("simple object", () => {
		const c = cur({ color: "red", size: 42 });
		expect(c.tag).toBe("object");
		// Iterate key/value pairs
		const k = makeCursor(c.data);
		const v = makeCursor(c.data);
		const entries: [string, unknown][] = [];
		let right = c.val;
		while (right > c.left) {
			k.right = right;
			read(k);
			v.right = k.left;
			read(v);
			entries.push([readStr(k), v.tag === "str" ? readStr(v) : v.val]);
			right = v.left;
		}
		expect(entries).toContainEqual(["color", "red"]);
		expect(entries).toContainEqual(["size", 42]);
	});

	test("empty object", () => {
		const c = cur({});
		expect(c.tag).toBe("object");
		expect(c.val).toBe(c.left);
	});
});

describe("read() indexed containers", () => {
	test("indexed array has ixWidth and ixCount", () => {
		const c = cur([1, 2, 3], { indexes: 0 });
		expect(c.tag).toBe("array");
		expect(c.ixWidth).toBeGreaterThan(0);
		expect(c.ixCount).toBe(3);
	});

	test("indexed object has ixWidth and ixCount", () => {
		const c = cur({ a: 1, b: 2, c: 3 }, { indexes: 0 });
		expect(c.tag).toBe("object");
		expect(c.ixWidth).toBeGreaterThan(0);
		expect(c.ixCount).toBe(3);
	});
});

describe("read() pointers", () => {
	test("pointer to string", () => {
		// hello,5^;8
		// Encoding writes last element first: "hello" at [0,7), then "^" pointer at [7,8)
		// Natural read order (right-to-left) sees pointer first, then string
		const c = cur(["hello", "hello"]);
		expect(c.tag).toBe("array");

		const tmp = makeCursor(c.data);
		// First child in read order: the pointer
		tmp.right = c.val;
		read(tmp);
		expect(tmp.tag).toBe("ptr");
		const secondChildRight = tmp.left; // save before resolving

		// Resolve pointer — should give us the string
		tmp.right = tmp.val;
		read(tmp);
		expect(tmp.tag).toBe("str");
		expect(readStr(tmp)).toBe("hello");

		// Second child in read order: the actual string
		tmp.right = secondChildRight;
		read(tmp);
		expect(tmp.tag).toBe("str");
		expect(readStr(tmp)).toBe("hello");
	});
});

describe("read() chains", () => {
	test("chain node has correct boundaries", () => {
		const c = cur(["/foo/bar/baz", "/foo/bar/qux", "/foo/quux"]);
		expect(c.tag).toBe("array");
		// Just verify we can iterate without crashing
		const tmp = makeCursor(c.data);
		let right = c.val;
		let count = 0;
		while (right > c.left) {
			tmp.right = right;
			read(tmp);
			right = tmp.left;
			count++;
		}
		expect(count).toBe(3);
	});
});

describe("strEquals", () => {
	test("matches ASCII strings", () => {
		const c = cur("hello");
		expect(strEquals(c, "hello")).toBe(true);
		expect(strEquals(c, "world")).toBe(false);
		expect(strEquals(c, "hell")).toBe(false);
		expect(strEquals(c, "helloo")).toBe(false);
	});

	test("matches unicode strings", () => {
		const c = cur("🚀");
		expect(strEquals(c, "🚀")).toBe(true);
		expect(strEquals(c, "🔥")).toBe(false);
	});

	test("matches empty string", () => {
		const c = cur("");
		expect(strEquals(c, "")).toBe(true);
		expect(strEquals(c, "a")).toBe(false);
	});
});

describe("strCompare", () => {
	test("ordering", () => {
		const a = cur("apple");
		const b = cur("banana");
		expect(strCompare(a, "apple")).toBe(0);
		expect(strCompare(a, "banana")).toBeLessThan(0);
		expect(strCompare(b, "apple")).toBeGreaterThan(0);
	});
});

describe("seekChild", () => {
	test("random access indexed array", () => {
		const arr = [10, 20, 30, 40, 50];
		const c = cur(arr, { indexes: 0 });
		expect(c.ixCount).toBe(5);
		const child = makeCursor(c.data);
		for (let i = 0; i < arr.length; i++) {
			seekChild(child, c, i);
			expect(child.tag).toBe("int");
			expect(child.val).toBe(arr[i]);
		}
	});
});

describe("collectChildren", () => {
	test("collects child boundaries", () => {
		const c = cur([1, 2, 3]);
		const offsets: number[] = [];
		const count = collectChildren(c, offsets);
		expect(count).toBe(3);
		// Verify we can read each child
		const tmp = makeCursor(c.data);
		const vals: number[] = [];
		for (let i = 0; i < count; i++) {
			tmp.right = offsets[i]!;
			read(tmp);
			vals.push(tmp.val);
		}
		expect(vals).toEqual([1, 2, 3]);
	});
});

describe("findKey", () => {
	test("finds existing key", () => {
		const c = cur({ color: "red", size: 42 });
		const v = makeCursor(c.data);
		expect(findKey(v, c, "color")).toBe(true);
		expect(v.tag).toBe("str");
		expect(readStr(v)).toBe("red");

		expect(findKey(v, c, "size")).toBe(true);
		expect(v.tag).toBe("int");
		expect(v.val).toBe(42);
	});

	test("returns false for missing key", () => {
		const c = cur({ a: 1 });
		const v = makeCursor(c.data);
		expect(findKey(v, c, "z")).toBe(false);
	});

	test("finds key that is a chain (path with shared prefix)", () => {
		// Keys like "/foo/bar" and "/foo/baz" share prefix "/foo" → chain encoding
		const obj = { "/foo/bar": 1, "/foo/baz": 2 };
		const c = cur(obj);
		const v = makeCursor(c.data);
		expect(findKey(v, c, "/foo/bar")).toBe(true);
		expect(v.tag).toBe("int");
		expect(v.val).toBe(1);

		expect(findKey(v, c, "/foo/baz")).toBe(true);
		expect(v.tag).toBe("int");
		expect(v.val).toBe(2);

		expect(findKey(v, c, "/foo/qux")).toBe(false);
	});
});

describe("rawBytes", () => {
	test("extracts node bytes", () => {
		const c = cur(42);
		const bytes = rawBytes(c);
		expect(new TextDecoder().decode(bytes)).toBe("+1k");
	});
});

describe("resolveStr", () => {
	test("plain string", () => {
		const c = cur("hello");
		expect(resolveStr(c)).toBe("hello");
	});

	test("pointer to string", () => {
		// Create data with a pointer: ["hello", "hello"] → second is a ptr
		const c = cur(["hello", "hello"]);
		const tmp = makeCursor(c.data);
		// First child in read order is the pointer
		tmp.right = c.val;
		read(tmp);
		expect(tmp.tag).toBe("ptr");
		expect(resolveStr(tmp)).toBe("hello");
	});

	test("chain string", () => {
		// Paths with shared prefixes produce chains
		const arr = ["/foo/bar/baz", "/foo/bar/qux"];
		const c = cur(arr);
		const tmp = makeCursor(c.data);
		// Iterate children and resolve each
		const results: string[] = [];
		let right = c.val;
		while (right > c.left) {
			tmp.right = right;
			read(tmp);
			results.push(resolveStr(tmp));
			right = tmp.left;
		}
		expect(results).toEqual(["/foo/bar/baz", "/foo/bar/qux"]);
	});

	test("throws on non-string node", () => {
		const c = cur(42);
		expect(() => resolveStr(c)).toThrow();
	});
});

describe("read() floats extended", () => {
	test("negative exponent (small decimal)", () => {
		const c = cur(0.001);
		expect(c.tag).toBe("float");
		expect(c.val).toBe(0.001);
	});

	test("large float", () => {
		const c = cur(1.23e15);
		expect(c.tag).toBe("float");
		expect(c.val).toBe(1.23e15);
	});

	test("small float", () => {
		const c = cur(1.5e-10);
		expect(c.tag).toBe("float");
		expect(c.val).toBe(1.5e-10);
	});

	test("negative float", () => {
		const c = cur(-3.14);
		expect(c.tag).toBe("float");
		expect(c.val).toBe(-3.14);
	});

	test("negative float with exponent", () => {
		const c = cur(-2.5e8);
		expect(c.tag).toBe("float");
		expect(c.val).toBe(-2.5e8);
	});
});

describe("read() large integers", () => {
	test("large positive without trailing zeroes", () => {
		const c = cur(123457);
		expect(c.tag).toBe("int");
		expect(c.val).toBe(123457);
	});

	test("large negative without trailing zeroes", () => {
		const c = cur(-999997);
		expect(c.tag).toBe("int");
		expect(c.val).toBe(-999997);
	});

	test("trailing zeroes encode as float with exponent", () => {
		// 1000000 = 1e6, encoder uses exponent form
		const c = cur(1000000);
		expect(c.tag).toBe("float");
		expect(c.val).toBe(1000000);
	});
});

describe("nested containers", () => {
	test("nested arrays", () => {
		const c = cur([[1, 2], [3, 4]]);
		expect(c.tag).toBe("array");
		const tmp = makeCursor(c.data);
		const inner = makeCursor(c.data);
		const results: number[][] = [];
		let right = c.val;
		while (right > c.left) {
			tmp.right = right;
			read(tmp);
			expect(tmp.tag).toBe("array");
			const vals: number[] = [];
			let innerRight = tmp.val;
			while (innerRight > tmp.left) {
				inner.right = innerRight;
				read(inner);
				vals.push(inner.val);
				innerRight = inner.left;
			}
			results.push(vals);
			right = tmp.left;
		}
		expect(results).toEqual([[1, 2], [3, 4]]);
	});

	test("nested objects", () => {
		const c = cur({ a: { b: 1 } });
		expect(c.tag).toBe("object");
		const v = makeCursor(c.data);
		expect(findKey(v, c, "a")).toBe(true);
		expect(v.tag).toBe("object");
		const inner = makeCursor(v.data);
		expect(findKey(inner, v, "b")).toBe(true);
		expect(inner.tag).toBe("int");
		expect(inner.val).toBe(1);
	});

	test("object containing array", () => {
		const c = cur({ items: [10, 20, 30] });
		const v = makeCursor(c.data);
		expect(findKey(v, c, "items")).toBe(true);
		expect(v.tag).toBe("array");
		const child = makeCursor(v.data);
		const vals: number[] = [];
		let right = v.val;
		while (right > v.left) {
			child.right = right;
			read(child);
			vals.push(child.val);
			right = child.left;
		}
		expect(vals).toEqual([10, 20, 30]);
	});

	test("array of objects", () => {
		const c = cur([{ x: 1 }, { x: 2 }]);
		expect(c.tag).toBe("array");
		const tmp = makeCursor(c.data);
		const v = makeCursor(c.data);
		const results: number[] = [];
		let right = c.val;
		while (right > c.left) {
			tmp.right = right;
			read(tmp);
			expect(tmp.tag).toBe("object");
			expect(findKey(v, tmp, "x")).toBe(true);
			results.push(v.val);
			right = tmp.left;
		}
		expect(results).toEqual([1, 2]);
	});
});

describe("seekChild on indexed objects", () => {
	test("random access indexed object entries", () => {
		const obj = { a: 10, b: 20, c: 30 };
		const c = cur(obj, { indexes: 0 });
		expect(c.tag).toBe("object");
		expect(c.ixCount).toBe(3);
		// Each entry is a key/value pair — seekChild gives the key node
		const child = makeCursor(c.data);
		const keys: string[] = [];
		for (let i = 0; i < c.ixCount; i++) {
			seekChild(child, c, i);
			// In indexed objects, each index entry points to a key
			keys.push(readStr(child));
		}
		expect(keys.length).toBe(3);
		// Indexed objects are sorted by UTF-8 key order
		expect(keys).toEqual(["a", "b", "c"]);
	});
});

describe("collectChildren on objects", () => {
	test("collects key/value boundaries", () => {
		const c = cur({ x: 1, y: 2 });
		const offsets: number[] = [];
		const count = collectChildren(c, offsets);
		// Objects without schema: children are interleaved key, value, key, value
		expect(count).toBe(4);
		const tmp = makeCursor(c.data);
		const tags: string[] = [];
		for (let i = 0; i < count; i++) {
			tmp.right = offsets[i]!;
			read(tmp);
			tags.push(tmp.tag);
		}
		// Alternating: str (key), int (value), str (key), int (value)
		expect(tags.filter(t => t === "str").length).toBe(2);
		expect(tags.filter(t => t === "int").length).toBe(2);
	});
});

describe("findKey with schema objects", () => {
	test("finds key in schema object (repeated shape)", () => {
		// Three objects with same keys. The encoder writes last-to-first,
		// so carol (index 2) is encoded first with inline keys.
		// alice and bob get schema pointers referencing carol's key layout.
		// Read order = logical order: alice, bob, carol.
		const data = [
			{ name: "alice", age: 30 },
			{ name: "bob", age: 25 },
			{ name: "carol", age: 20 },
		];
		const c = cur(data);
		expect(c.tag).toBe("array");
		const tmp = makeCursor(c.data);
		const v = makeCursor(c.data);

		// alice (first in read order) has a schema — last encoded, references carol's keys
		tmp.right = c.val;
		read(tmp);
		expect(tmp.tag).toBe("object");
		expect(tmp.schema).not.toBe(0);

		// findKey should work on schema objects
		expect(findKey(v, tmp, "name")).toBe(true);
		expect(v.tag).toBe("str");
		expect(readStr(v)).toBe("alice");

		expect(findKey(v, tmp, "age")).toBe(true);
		expect(v.tag).toBe("int");
		expect(v.val).toBe(30);

		expect(findKey(v, tmp, "missing")).toBe(false);

		// bob (second in read order) also has a schema
		tmp.right = tmp.left;
		read(tmp);
		expect(tmp.tag).toBe("object");
		expect(tmp.schema).not.toBe(0);

		expect(findKey(v, tmp, "name")).toBe(true);
		expect(readStr(v)).toBe("bob");

		expect(findKey(v, tmp, "age")).toBe(true);
		expect(v.val).toBe(25);

		// carol (third in read order) has inline keys, no schema
		tmp.right = tmp.left;
		read(tmp);
		expect(tmp.tag).toBe("object");
		expect(tmp.schema).toBe(0);

		expect(findKey(v, tmp, "name")).toBe(true);
		expect(readStr(v)).toBe("carol");

		expect(findKey(v, tmp, "age")).toBe(true);
		expect(v.val).toBe(20);
	});
});

describe("findKey with pointer keys", () => {
	test("finds key that is a pointer (deduplicated key string)", () => {
		// When the same key string appears in multiple objects, the encoder
		// deduplicates it with a pointer. Use enough objects to trigger this.
		const data = [
			{ name: "alice" },
			{ name: "bob" },
			{ name: "carol" },
		];
		const c = cur(data);
		const tmp = makeCursor(c.data);
		const v = makeCursor(c.data);

		// Iterate all objects and findKey "name" in each
		let right = c.val;
		const names: string[] = [];
		while (right > c.left) {
			tmp.right = right;
			read(tmp);
			expect(tmp.tag).toBe("object");
			expect(findKey(v, tmp, "name")).toBe(true);
			expect(v.tag).toBe("str");
			names.push(readStr(v));
			right = tmp.left;
		}
		expect(names).toEqual(["alice", "bob", "carol"]);
	});
});

describe("strEquals with multi-byte UTF-8", () => {
	test("2-byte UTF-8 (accented characters)", () => {
		const c = cur("café");
		expect(strEquals(c, "café")).toBe(true);
		expect(strEquals(c, "cafe")).toBe(false);
		expect(strEquals(c, "caféé")).toBe(false);
	});

	test("3-byte UTF-8 (CJK characters)", () => {
		const c = cur("日本語");
		expect(strEquals(c, "日本語")).toBe(true);
		expect(strEquals(c, "日本")).toBe(false);
		expect(strEquals(c, "中文")).toBe(false);
	});

	test("mixed ASCII and multi-byte", () => {
		const c = cur("hello 世界 🌍");
		expect(strEquals(c, "hello 世界 🌍")).toBe(true);
		expect(strEquals(c, "hello 世界")).toBe(false);
	});
});

describe("error paths", () => {
	test("seekChild throws on non-indexed container", () => {
		const c = cur([1, 2, 3]); // no indexes option
		const child = makeCursor(c.data);
		expect(() => seekChild(child, c, 0)).toThrow("indexed");
	});

	test("seekChild throws on out-of-range index", () => {
		const c = cur([1, 2, 3], { indexes: 0 });
		const child = makeCursor(c.data);
		expect(() => seekChild(child, c, -1)).toThrow();
		expect(() => seekChild(child, c, 3)).toThrow();
	});

	test("findKey returns false on non-object", () => {
		const c = cur([1, 2, 3]);
		const v = makeCursor(c.data);
		expect(findKey(v, c, "key")).toBe(false);
	});
});

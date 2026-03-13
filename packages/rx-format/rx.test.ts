import { describe, expect, test } from "bun:test";
import { encode } from "@creationix/rex/rexc";
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
	open,
	handle,
	prepareKey,
	strHasPrefix,
	findByPrefix,
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

const p = prepareKey;

describe("strEquals", () => {
	test("matches ASCII strings", () => {
		const c = cur("hello");
		expect(strEquals(c, p("hello"))).toBe(true);
		expect(strEquals(c, p("world"))).toBe(false);
		expect(strEquals(c, p("hell"))).toBe(false);
		expect(strEquals(c, p("helloo"))).toBe(false);
	});

	test("matches unicode strings", () => {
		const c = cur("🚀");
		expect(strEquals(c, p("🚀"))).toBe(true);
		expect(strEquals(c, p("🔥"))).toBe(false);
	});

	test("matches empty string", () => {
		const c = cur("");
		expect(strEquals(c, p(""))).toBe(true);
		expect(strEquals(c, p("a"))).toBe(false);
	});
});

describe("strCompare", () => {
	test("ordering", () => {
		const a = cur("apple");
		const b = cur("banana");
		expect(strCompare(a, p("apple"))).toBe(0);
		expect(strCompare(a, p("banana"))).toBeLessThan(0);
		expect(strCompare(b, p("apple"))).toBeGreaterThan(0);
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
		expect(strEquals(c, p("café"))).toBe(true);
		expect(strEquals(c, p("cafe"))).toBe(false);
		expect(strEquals(c, p("caféé"))).toBe(false);
	});

	test("3-byte UTF-8 (CJK characters)", () => {
		const c = cur("日本語");
		expect(strEquals(c, p("日本語"))).toBe(true);
		expect(strEquals(c, p("日本"))).toBe(false);
		expect(strEquals(c, p("中文"))).toBe(false);
	});

	test("mixed ASCII and multi-byte", () => {
		const c = cur("hello 世界 🌍");
		expect(strEquals(c, p("hello 世界 🌍"))).toBe(true);
		expect(strEquals(c, p("hello 世界"))).toBe(false);
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

// ── open() Proxy API ──

function opened(value: unknown, opts?: Parameters<typeof encode>[1]) {
	return open(encode(value, opts));
}

describe("open() primitives", () => {
	test("integers", () => {
		expect(opened(0)).toBe(0);
		expect(opened(42)).toBe(42);
		expect(opened(-7)).toBe(-7);
	});

	test("floats", () => {
		expect(opened(3.14)).toBe(3.14);
		expect(opened(Infinity)).toBe(Infinity);
		expect(opened(-Infinity)).toBe(-Infinity);
		expect(opened(NaN)).toBeNaN();
	});

	test("strings", () => {
		expect(opened("")).toBe("");
		expect(opened("hello")).toBe("hello");
		expect(opened("🚀")).toBe("🚀");
	});

	test("booleans, null, undefined", () => {
		expect(opened(true)).toBe(true);
		expect(opened(false)).toBe(false);
		expect(opened(null)).toBe(null);
		expect(opened(undefined)).toBe(undefined);
	});
});

describe("open() arrays", () => {
	test("Array.isArray", () => {
		expect(Array.isArray(opened([]))).toBe(true);
		expect(Array.isArray(opened([1, 2]))).toBe(true);
	});

	test("length", () => {
		const arr = opened([10, 20, 30]) as unknown[];
		expect(arr.length).toBe(3);
	});

	test("index access", () => {
		const arr = opened([10, 20, 30]) as unknown[];
		expect(arr[0]).toBe(10);
		expect(arr[1]).toBe(20);
		expect(arr[2]).toBe(30);
		expect(arr[3]).toBe(undefined);
	});

	test("for...of iteration", () => {
		const arr = opened([1, 2, 3]) as unknown[];
		const vals: unknown[] = [];
		for (const v of arr) vals.push(v);
		expect(vals).toEqual([1, 2, 3]);
	});

	test("spread", () => {
		const arr = opened([1, 2, 3]) as unknown[];
		expect([...arr]).toEqual([1, 2, 3]);
	});

	test("JSON.stringify", () => {
		const arr = opened([1, "hello", true, null]);
		expect(JSON.stringify(arr)).toBe('[1,"hello",true,null]');
	});

	test("nested arrays", () => {
		const arr = opened([[1, 2], [3, 4]]) as unknown[][];
		expect(arr[0]![0]).toBe(1);
		expect(arr[0]![1]).toBe(2);
		expect(arr[1]![0]).toBe(3);
		expect(arr[1]![1]).toBe(4);
		expect(JSON.stringify(arr)).toBe("[[1,2],[3,4]]");
	});

	test("empty array", () => {
		const arr = opened([]) as unknown[];
		expect(arr.length).toBe(0);
		expect([...arr]).toEqual([]);
	});

	test("indexed array", () => {
		const arr = opened([10, 20, 30, 40, 50], { indexes: 0 }) as unknown[];
		expect(arr.length).toBe(5);
		expect(arr[0]).toBe(10);
		expect(arr[4]).toBe(50);
		expect([...arr]).toEqual([10, 20, 30, 40, 50]);
	});

	test("'in' operator", () => {
		const arr = opened([10, 20]) as unknown[];
		expect(0 in arr).toBe(true);
		expect(1 in arr).toBe(true);
		expect(2 in arr).toBe(false);
	});
});

describe("open() objects", () => {
	test("property access", () => {
		const obj = opened({ color: "red", size: 42 }) as any;
		expect(obj.color).toBe("red");
		expect(obj.size).toBe(42);
	});

	test("missing key returns undefined", () => {
		const obj = opened({ a: 1 }) as any;
		expect(obj.missing).toBe(undefined);
	});

	test("Object.keys", () => {
		const obj = opened({ x: 1, y: 2 }) as any;
		const keys = Object.keys(obj);
		expect(keys.sort()).toEqual(["x", "y"]);
	});

	test("Object.entries", () => {
		const obj = opened({ a: 1, b: 2 }) as any;
		const entries = Object.entries(obj);
		expect(entries.sort()).toEqual([["a", 1], ["b", 2]]);
	});

	test("'in' operator", () => {
		const obj = opened({ a: 1 }) as any;
		expect("a" in obj).toBe(true);
		expect("b" in obj).toBe(false);
	});

	test("JSON.stringify", () => {
		const obj = opened({ a: 1, b: "hello" }) as any;
		const parsed = JSON.parse(JSON.stringify(obj));
		expect(parsed.a).toBe(1);
		expect(parsed.b).toBe("hello");
	});

	test("nested objects", () => {
		const obj = opened({ outer: { inner: 42 } }) as any;
		expect(obj.outer.inner).toBe(42);
	});

	test("object containing array", () => {
		const obj = opened({ items: [10, 20, 30] }) as any;
		expect(Array.isArray(obj.items)).toBe(true);
		expect(obj.items.length).toBe(3);
		expect(obj.items[1]).toBe(20);
	});

	test("array of objects", () => {
		const data = opened([{ x: 1 }, { x: 2 }]) as any[];
		expect(data[0].x).toBe(1);
		expect(data[1].x).toBe(2);
	});

	test("empty object", () => {
		const obj = opened({}) as any;
		expect(Object.keys(obj)).toEqual([]);
	});

	test("length on object", () => {
		const obj = opened({ a: 1, b: 2, c: 3 }) as any;
		expect(obj.length).toBe(3);
	});
});

describe("open() schema objects", () => {
	test("property access on schema objects", () => {
		const data = opened([
			{ name: "alice", age: 30 },
			{ name: "bob", age: 25 },
			{ name: "carol", age: 20 },
		]) as any[];
		expect(data[0].name).toBe("alice");
		expect(data[0].age).toBe(30);
		expect(data[1].name).toBe("bob");
		expect(data[2].age).toBe(20);
	});

	test("Object.keys on schema objects", () => {
		const data = opened([
			{ name: "alice", age: 30 },
			{ name: "bob", age: 25 },
			{ name: "carol", age: 20 },
		]) as any[];
		expect(Object.keys(data[0]).sort()).toEqual(["age", "name"]);
		expect(Object.keys(data[1]).sort()).toEqual(["age", "name"]);
		// carol has inline keys (no schema)
		expect(Object.keys(data[2]).sort()).toEqual(["age", "name"]);
	});

	test("JSON.stringify with schema objects", () => {
		const data = opened([
			{ name: "alice", age: 30 },
			{ name: "bob", age: 25 },
		]) as any[];
		const parsed = JSON.parse(JSON.stringify(data));
		expect(parsed).toEqual([
			{ name: "alice", age: 30 },
			{ name: "bob", age: 25 },
		]);
	});
});

describe("open() pointers and chains", () => {
	test("pointer values resolve transparently", () => {
		const data = opened(["hello", "hello"]) as any[];
		expect(data[0]).toBe("hello");
		expect(data[1]).toBe("hello");
	});

	test("chain strings resolve", () => {
		const data = opened(["/foo/bar/baz", "/foo/bar/qux"]) as any[];
		expect(data[0]).toBe("/foo/bar/baz");
		expect(data[1]).toBe("/foo/bar/qux");
	});
});

describe("open() read-only", () => {
	test("set throws", () => {
		const obj = opened({ a: 1 }) as any;
		expect(() => { obj.a = 2; }).toThrow("read-only");
	});

	test("delete throws", () => {
		const obj = opened({ a: 1 }) as any;
		expect(() => { delete obj.a; }).toThrow("read-only");
	});
});

describe("open() handle escape hatch", () => {
	test("handle returns data and right offset", () => {
		const obj = opened({ a: 1 }) as any;
		const h = handle(obj);
		expect(h).toBeDefined();
		expect(h!.data).toBeInstanceOf(Uint8Array);
		expect(typeof h!.right).toBe("number");
	});

	test("handle returns undefined for non-proxy", () => {
		expect(handle(42)).toBe(undefined);
		expect(handle("hello")).toBe(undefined);
		expect(handle({})).toBe(undefined);
	});
});

describe("open() Symbol.iterator on objects", () => {
	test("iterates [key, value] pairs", () => {
		const obj = opened({ a: 1, b: 2 }) as any;
		const entries: [string, unknown][] = [];
		for (const pair of obj) entries.push(pair);
		expect(entries.sort((a, b) => a[0].localeCompare(b[0]))).toEqual([["a", 1], ["b", 2]]);
	});
});

// ── strHasPrefix ──

describe("strHasPrefix", () => {
	test("matches ASCII prefix", () => {
		const c = cur("hello world");
		expect(strHasPrefix(c, p("hello"))).toBe(true);
		expect(strHasPrefix(c, p("hello world"))).toBe(true);
		expect(strHasPrefix(c, p("world"))).toBe(false);
	});

	test("empty prefix matches everything", () => {
		const c = cur("hello");
		expect(strHasPrefix(c, p(""))).toBe(true);
		const empty = cur("");
		expect(strHasPrefix(empty, p(""))).toBe(true);
	});

	test("prefix longer than string does not match", () => {
		const c = cur("hi");
		expect(strHasPrefix(c, p("hello"))).toBe(false);
	});

	test("unicode prefix", () => {
		const c = cur("café latte");
		expect(strHasPrefix(c, p("café"))).toBe(true);
		expect(strHasPrefix(c, p("cafe"))).toBe(false);
	});

	test("chain strings match prefix", () => {
		const arr = cur(["/foo/bar/baz", "/foo/bar/qux"]);
		const tmp = makeCursor(arr.data);
		tmp.right = arr.val;
		read(tmp);
		// First child is a chain
		expect(strHasPrefix(tmp, p("/foo/bar"))).toBe(true);
		expect(strHasPrefix(tmp, p("/foo/baz"))).toBe(false);
	});
});

// ── strCompare / strEquals on non-string nodes ──

describe("strCompare on non-string nodes", () => {
	test("returns NaN for integer", () => {
		const c = cur(42);
		expect(strCompare(c, p("hello"))).toBeNaN();
	});

	test("strEquals returns false for non-string", () => {
		const c = cur(42);
		expect(strEquals(c, p("42"))).toBe(false);
	});

	test("strHasPrefix returns false for non-string", () => {
		const c = cur(42);
		expect(strHasPrefix(c, p("4"))).toBe(false);
	});
});

// ── findByPrefix ──

describe("findByPrefix", () => {
	test("finds matching keys (non-indexed)", () => {
		const obj = cur({ apple: 1, apricot: 2, banana: 3, avocado: 4 });
		const c = makeCursor(obj.data);
		const results: [string, number][] = [];
		findByPrefix(c, obj, "ap", (key, value) => {
			results.push([resolveStr(key), value.val]);
		});
		expect(results.sort()).toEqual([["apple", 1], ["apricot", 2]]);
	});

	test("finds matching keys (indexed)", () => {
		const obj = cur({ apple: 1, apricot: 2, banana: 3, avocado: 4 }, { indexes: 0 });
		const c = makeCursor(obj.data);
		const results: [string, number][] = [];
		findByPrefix(c, obj, "ap", (key, value) => {
			results.push([resolveStr(key), value.val]);
		});
		expect(results.sort()).toEqual([["apple", 1], ["apricot", 2]]);
	});

	test("no matches returns nothing", () => {
		const obj = cur({ apple: 1, banana: 2 });
		const c = makeCursor(obj.data);
		const results: string[] = [];
		findByPrefix(c, obj, "zzz", (key) => { results.push(resolveStr(key)); });
		expect(results).toEqual([]);
	});

	test("empty prefix matches all keys", () => {
		const obj = cur({ a: 1, b: 2 });
		const c = makeCursor(obj.data);
		const results: string[] = [];
		findByPrefix(c, obj, "", (key) => { results.push(resolveStr(key)); });
		expect(results.sort()).toEqual(["a", "b"]);
	});

	test("visitor returning false stops iteration", () => {
		const obj = cur({ a: 1, b: 2, c: 3 });
		const c = makeCursor(obj.data);
		const results: string[] = [];
		findByPrefix(c, obj, "", (key) => {
			results.push(resolveStr(key));
			return false; // stop after first
		});
		expect(results.length).toBe(1);
	});

	test("works with chain keys", () => {
		const obj = cur({ "/foo/bar": 1, "/foo/baz": 2, "/qux": 3 });
		const c = makeCursor(obj.data);
		const results: [string, number][] = [];
		findByPrefix(c, obj, "/foo/", (key, value) => {
			results.push([resolveStr(key), value.val]);
		});
		expect(results.sort()).toEqual([["/foo/bar", 1], ["/foo/baz", 2]]);
	});

	test("on non-object does nothing", () => {
		const arr = cur([1, 2, 3]);
		const c = makeCursor(arr.data);
		let called = false;
		findByPrefix(c, arr, "x", () => { called = true; });
		expect(called).toBe(false);
	});
});

// ── Proxy identity (memoization) ──

describe("open() proxy identity", () => {
	test("same container returns same proxy", () => {
		const obj = opened({ nested: { a: 1 } }) as any;
		expect(obj.nested).toBe(obj.nested);
	});

	test("same array element returns same proxy", () => {
		const arr = opened([{ x: 1 }, { x: 2 }]) as any[];
		expect(arr[0]).toBe(arr[0]);
	});

	test("pointer dedup returns same proxy", () => {
		// Two objects sharing the same nested value via pointer
		const shared = { inner: 42 };
		const arr = opened([shared, shared]) as any[];
		expect(arr[0]).toBe(arr[1]);
	});
});

// ── Proxy Array.prototype delegation ──

describe("open() array methods", () => {
	test("map", () => {
		const arr = opened([1, 2, 3]) as any[];
		const doubled = arr.map((x: number) => x * 2);
		expect(doubled).toEqual([2, 4, 6]);
	});

	test("filter", () => {
		const arr = opened([1, 2, 3, 4, 5]) as any[];
		const evens = arr.filter((x: number) => x % 2 === 0);
		expect(evens).toEqual([2, 4]);
	});

	test("indexOf", () => {
		const arr = opened([10, 20, 30]) as any[];
		expect(arr.indexOf(20)).toBe(1);
		expect(arr.indexOf(99)).toBe(-1);
	});

	test("includes", () => {
		const arr = opened(["a", "b", "c"]) as any[];
		expect(arr.includes("b")).toBe(true);
		expect(arr.includes("z")).toBe(false);
	});

	test("every / some", () => {
		const arr = opened([2, 4, 6]) as any[];
		expect(arr.every((x: number) => x % 2 === 0)).toBe(true);
		expect(arr.some((x: number) => x > 5)).toBe(true);
		expect(arr.some((x: number) => x > 10)).toBe(false);
	});

	test("reduce", () => {
		const arr = opened([1, 2, 3]) as any[];
		const sum = arr.reduce((acc: number, x: number) => acc + x, 0);
		expect(sum).toBe(6);
	});

	test("find", () => {
		const arr = opened([{ x: 1 }, { x: 2 }, { x: 3 }]) as any[];
		const found = arr.find((item: any) => item.x === 2);
		expect(found.x).toBe(2);
	});

	test("slice", () => {
		const arr = opened([10, 20, 30, 40]) as any[];
		expect(arr.slice(1, 3)).toEqual([20, 30]);
	});
});

// ── Proxy for...in iteration ──

describe("open() for...in", () => {
	test("iterates object keys", () => {
		const obj = opened({ x: 1, y: 2, z: 3 }) as any;
		const keys: string[] = [];
		for (const k in obj) keys.push(k);
		expect(keys.sort()).toEqual(["x", "y", "z"]);
	});

	test("accesses values during for...in", () => {
		const obj = opened({ a: 10, b: 20 }) as any;
		const entries: [string, number][] = [];
		for (const k in obj) entries.push([k, obj[k]]);
		expect(entries.sort()).toEqual([["a", 10], ["b", 20]]);
	});
});

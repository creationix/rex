import { describe, expect, test } from "bun:test";
import { encode } from "./rexc";
import {
	makeCursor,
	read,
	readStr,
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

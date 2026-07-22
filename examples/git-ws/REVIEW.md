# Git WebSocket Server — Rex Review Notes

Notes on what works well, what's awkward, and what could improve as I build this app.

## What Works Well

**Existence-based control flow is natural for server code.** Guard clauses like `unless user do return {error: "auth_required"} end` read clearly and the type narrowing means `user` is guaranteed non-none after the guard. This eliminates an entire class of null-check bugs.

**The middleware chain is elegant.** Variables set in `_middleware.rex` flow downstream automatically. The auth middleware sets `user` and every downstream handler just uses it. No dependency injection, no context objects, no boilerplate.

**Template literals and bare object keys make JSON-shaped responses pleasant.** Returning `{ok: true, user: user.id}` without quotes on keys feels right for a language that lives in JSON territory.

**The namespace pattern (db.get, kv.set, etc.) is clean.** Each namespace is a focused API surface. The type checker catches typos in method names. The opcode dispatch is simple on the host side.

**Discriminated unions + narrowing (new) will be great.** Being able to write `when obj.type == "commit" do obj.tree end` and have `obj.tree` resolve to `str` instead of `str | none` is exactly right for git object handling. This is a general win — any polymorphic host response benefits.

## Pain Points

**~~No early return from nested blocks.~~** Retracted — `return` works from anywhere (halts the script) and `break` exits loops. The original commit walker was just a bad algorithm: it used a `skipping` flag and duplicated logic instead of using `break`. Rewrote both the pagination skip and collect loops with `unless ... do break end` guards, and added `break` to inner entry searches. Much cleaner.

**Multi-level break?** The bytecode supports `break N` / `continue N` for nested loops, but there's no Rex syntax for it. A labeled break (`break outer`) would help in cases like "break out of the path walk when an entry is not found" — currently handled with `return` (which works since the error case exits the whole script anyway). Low priority since `return` covers the important case.

**String slicing is the only way to parse strings.** The auth middleware needs to strip "Bearer " from the Authorization header. I wrote `auth.slice(7, auth.size)` which works but is fragile — a `strip-prefix` method that returns `str | none` would be more rex-like:
```rex
when token = auth.strip-prefix("Bearer ") do
  // token is narrowed to str, cleanly extracted
end
```
Strings already have `starts-with` which returns `str | none`, but there's no way to get the remainder.

**Consolidated encode into `git.encode(obj)`.** Originally had four separate `git.encode-commit/tree/tag/blob` functions taking individual args. Replaced with one `git.encode(obj: GitCommit | GitTree | GitTag) -> blob` that mirrors `git.decode` — same object shape in, blob out. The host dispatches on `obj.type`. `encode-blob` stays separate because decoded blobs don't carry content (by design — Rex can't inspect raw bytes).

This feels right: construct a plain object literal, pass it to `git.encode`. The type checker validates the shape, the host handles serialization. Rex code reads naturally:
```rex
data = git.encode({type: "tree", entries: new-entries})
hash = cas.put(data)
```

**No way to break out of `for` loops early.** Walking a tree path means iterating all segments even after finding "not found". I work around it with a mutable flag variable, but `break` would be cleaner. (Edit: Rex does have `break` — I should use it in the path walkers.)

**The `kv.delete` pattern shows a broader design question.** Functions that returned `bool` now return the key or `none`. But what about `db.delete` — should it return the *old value* instead of the key? That would let you implement "pop" semantics: `when old = db.delete(key) do log("removed", old) end`. Returning the key is less useful since the caller already has it. Same argument applies to `kv.delete`.

## Feature Requests

**`str.strip-prefix(prefix)` and `str.strip-suffix(suffix)`** — return the remainder as `str | none`. More rex-like than manual slicing, plays well with `when` narrowing.

**`db.delete` and `kv.delete` now return the old value instead of the key.** The caller already has the key. The old value is the thing you lose access to. `when old = db.delete("session:" + token) do log("revoked", old) end`. This came up naturally while fixing the `-> bool` anti-pattern — every host function should return something the caller doesn't already have.

## Typechecker Bugs & Gaps

Issues found while getting all 24 route files to pass `rex check`.

### Bug: keywords as object field names cause infinite loop (FIXED)

`parse_type_pair` and `parse_obj_key` only accepted `SyntaxKind::Ident` as field names. Keywords like `type`, `for`, `end` are lexed as keyword tokens, not `Ident`. So `{type: "commit"}` caused the parser's `while` loop to spin forever — it couldn't consume the field name, never advanced, and allocated memory without bound. The LSP hit this on every keystroke and consumed 100+ GB of RAM.

**Fix:** Accept `kind.is_keyword()` as valid field names in `parse_type_pair`, `parse_obj_key`, and `extract_dotted_name`. Added progress guard to `parse_type_object`'s `while` loop.

### Bug: discriminated union narrowing doesn't fully narrow

`when obj.type == "commit" do obj.tree end` — the `FieldEquals` narrowing is implemented and filters the union, but downstream access still warns "unknown property 'tree' on some branches" with the full 4-variant union type. The narrowed type isn't being applied to the variable's scope, or the narrowed scope is being merged back with the un-narrowed parent.

**Repro:** Any file using `git.decode` + `when obj.type == "..." do` — e.g. `commits/index.rex`, `tree/[...path].rex`.

### Bug: `unless x do return/break end` doesn't narrow the continuation

```rex
unless current do break end
cas.get(current) // error: expected str, got str | none
```

After `unless current do break end`, the continuation should have `current` narrowed to non-none (the `break` path is dead). But the scope merging doesn't detect that the then-branch body is `Never`. The `then_definite` / `then_dead` flags are based on the *condition* being always/never none, not on the *body* returning `Never`.

**Expected:** When a branch body is `Never` (return/break), treat that branch as dead for scope merging — the continuation only has the inverse narrowing.

**Workaround:** Use `when x = expr do` assignment narrowing instead.

### Gap: `while` condition doesn't narrow the loop body

```rex
while current do
  cas.get(current) // error: expected str, got str | none
end
```

The `while` condition `current` proves non-none for each iteration, but the typechecker doesn't apply existence narrowing inside the loop body. `for` loops do this for the iteration variable — `while` should do it for the condition.

### Gap: nav expression existence narrowing doesn't work

```rex
when msg.id do
  // msg.id is still some | none, not some
end
```

`extract_narrowing_from_child` correctly pushes `Narrowing::Exists("msg.id")` for `NavExpr` nodes. But `apply_narrowings` calls `lookup_var("msg.id")` which fails — `"msg.id"` is a dotted path, not a variable name. The narrowing is extracted but never applied.

**Fix:** `apply_narrowings` should split dotted names and resolve through nav chains, or the narrowing should track the base variable + field path separately.

### Gap: `json.parse` returns `some` — no way to narrow to a specific shape

`json.parse` returns `some`. Every property access is `some | none`. After narrowing away `none`, you have `some` — still not `str` or `int`. Functions expecting `str` reject `some`.

**Workarounds used:**
- Template literals: `id = "${msg.id}"` coerces `some` to `str`
- `when` assignment: `when id = msg.id do` narrows to `some` (but `some` still isn't `str`)

**Possible solution:** A typed parse that narrows the whole document:
```rex
type PushMsg = {id: int, ref: str, new: str, old: str | none}
msg = json.parse(event.data) : PushMsg
// msg.id is int, msg.ref is str — fully typed
```
This would be a type assertion on `json.parse` (or any `some` value). The typechecker trusts the assertion; the host validates at runtime.

### Gap: `str & [str]` from params not assignable to `str`

`params` has type `{*: str & [str]}`. So `params.hash` is `str & [str] | none`. After narrowing, it's `str & [str]`. But `cas.get(hash: str)` rejects it because `str & [str]` isn't recognized as assignable to `str`. Intersection with a supertype should be assignable to either component.

## Design Observations

**The blob type boundary is well-placed.** Rex never parses raw bytes — it asks the host to decode/encode. This keeps Rex handlers focused on policy (auth, permissions, ref updates) while the host handles protocol (SHA-1, git object format, compression). A handler that needs to inspect a tree entry calls `git.decode`, not byte manipulation.

**The `.rexd` file is the contract.** It defines what the host provides and what Rex can use. Adding `db.cas` as a generic primitive (not git-specific) was the right call — it's useful for any optimistic concurrency pattern.

**WebSocket transforms are just functions from message to message.** The handler receives `event.data`, does policy checks, and returns the transformed message or `none` to suppress. The host handles connection management, binary frames, and pub/sub. Clean separation.

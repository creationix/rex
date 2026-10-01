# Git WebSocket Server — Rex Implementation Plan

Implement the [proposed git WebSocket protocol](proposed-git-websocket-protocol.md) as a rex-serve application with a ref watch extension.

## Design Principle

**Rex handles policy, host handles protocol.** Rex never touches binary frames or hashing. It validates permissions, enforces branch protection, shapes error messages, logs actions, and publishes ref change notifications. The host handles binary I/O, compression, SHA-1 verification, and object storage.

---

## 1. Host Interface

### Existing rex-serve externs (no changes)

| Extern | Used for |
|--------|----------|
| `db.get/set/delete/list` | Ref storage (`ref:{repo}/{refname}` → hash) |
| `kv.get/set/incr/publish` | Rate limiting, ref watch pub/sub |
| `json.parse/stringify` | Control message handling |
| `crypto.hash/hmac` | Token validation |
| `time.now/uuid` | Timestamps, request IDs |
| `log.info/warning/error` | Audit logging |
| `headers/method/params/body/query` | HTTP/WebSocket request data |
| `res.status/headers` | Response control |
| `event.data` | WebSocket message transform scripts |

### New externs needed

#### `db.cas(key, old, new) -> str | none` — Compare-and-swap

Atomic conditional write. Returns `none` on success, returns the actual current value on conflict. Existence-based: `when conflict = db.cas(...) do /* handle */ end`. Generic primitive — useful beyond git.

#### `cas.*` — Content-addressable store

New namespace for storing opaque blob data by hash.

```
extern cas.put(data: blob) -> str          // store bytes, return hash
extern cas.get(hash: str) -> blob | none   // retrieve by hash, none if missing
extern cas.has(hash: str) -> str | none      // return hash if exists, none if missing
```

`blob` is a new opaque type. Rex cannot read individual bytes or convert to/from string — if you need to inspect contents, the host provides a decoder (like `git.decode`). Supported operations:
- `.size` → byte count
- `.slice(start, end)` → binary (extract a range)
- `+` concatenation → binary (reassemble chunks)
- Passing to host functions
- Equality comparison

`cas.has` returns the hash itself on existence (not the data) — useful in `when` checks without fetching the full blob: `when cas.has(child) do /* already stored */ end`.

#### `git.*` — Git object encoding/decoding

Domain-specific layer built on `cas` + blob type.

```
// Decoding
extern git.decode(data: blob) -> GitObject
extern git.children(data: blob) -> [str]      // shortcut: extract referenced hashes

// Verification — returns hash on match, none on mismatch
extern git.verify(hash: str, data: blob) -> str | none

// Encoding — returns binary git objects
extern git.encode-commit(tree: str, parents: [str], message: str, author: str, time: int) -> blob
extern git.encode-tree(entries: [{name: str, mode: str, hash: str}]) -> blob
extern git.encode-tag(object: str, type: str, tag: str, tagger: str, message: str) -> blob

// Graph traversal — returns descendant hash if ancestor, none otherwise
extern git.is-ancestor(ancestor: str, descendant: str) -> str | none
```

All functions return values on success, `none` on failure — no booleans. For example:

```rex
unless git.is-ancestor(current, msg.new) do
  return {error: "non-fast-forward", current: current}
end
```

`git.decode` returns a **discriminated union** (see Rex Features below):

```
type GitCommit = {type: "commit", tree: str, parents: [str], message: str, author: str, time: int}
type GitTree   = {type: "tree", entries: [{name: str, mode: str, hash: str}]}
type GitBlob   = {type: "blob", size: int}
type GitTag    = {type: "tag", object: str, tag-type: str, tag: str, tagger: str, message: str}
type GitObject = GitCommit | GitTree | GitBlob | GitTag
```

Blobs also need encoding — git hashes objects as `"{type} {size}\0{content}"`, so raw bytes must be wrapped:

```
extern git.encode-blob(data: blob) -> blob
```

#### `ws.*` — WebSocket binary frame control

Per-connection state for managing push/fetch streams.

```
extern ws.id: str                                // connection-scoped identifier
extern ws.expect(id: int, hash: str) -> str      // add hash to expect set, returns hash
extern ws.expect-remaining(id: int) -> int        // count of remaining expected hashes
extern ws.send-object(hash: str) -> str | none   // send binary object frame, none on failure
extern ws.send-want(hashes: [str]) -> [str] | none // send binary want frame, none on failure
```

`ws.expect-remaining` returns a count instead of a boolean — `0` means complete, and the count is useful for logging/monitoring. The host fires a `{"status": "complete"}` event when the count reaches zero.

Host manages expect sets, binary frame parsing/construction, and zstd compression.

---

## 2. Rex Features Needed

### Discriminated union narrowing

`git.decode` returns `GitCommit | GitTree | GitBlob | GitTag`. Rex handlers need to branch on `obj.type` and get precise types:

```rex
obj = git.decode(data)
when obj.type == "commit" do
  obj.tree      // str, not str | none
  obj.parents   // [str], not [str] | none
end
```

**Current state:** `Narrowing::Equals` works on bare variables (`when x == "GET"`), but not property access (`when obj.type == "commit"`).

**Implementation:**
- Add `FieldEquals(var, field, literal)` variant to `Narrowing` enum
- Detect `nav.field == literal` in `extract_narrowing_from_child` (LHS is `NavExpr`)
- `apply_narrowings`: filter union to variants where field matches the literal
- `apply_narrowings_inverse`: exclude the matching variant (else branch gets remaining variants)
- Needs `Type::matches_field(field, expected) -> bool` helper to check if an object type's field is assignable to the expected literal

This is a general improvement — any host returning polymorphic objects benefits (API responses, event types, not just git).

### Opaque `blob` type (was `binary`)

New opaque type in the type system for host-provided byte data. Intentionally limited to prevent implementing parsers in Rex — if you need to read contents, the host provides a decoder.

Supported: `.size` (byte count), `.slice(start, end)`, `+` concatenation (binary + binary), passing to host functions, equality comparison.

Not supported: indexing individual bytes, iteration, conversion to/from string.

The type checker should reject `binary + "hello"` (mixed types) while allowing `binary + binary`.

---

## 3. File Structure

```
examples/git-ws/
  git-ws.rexd                         # domain type interface
  rex-serve.toml                      # server config
  routes/
    _middleware.rex                    # global: parse auth token (optional)
    _layouts/
      page.html                       # HTML layout template
    index.rex                         # landing / login page

    // ── Auth ───────────────────────
    auth/
      signup.rex                      # POST: create account
      login.rex                       # POST: create session token
      logout.rex                      # POST: revoke token

    // ── User management ────────────
    api/
      _middleware.rex                 # require valid token
      account.rex                     # GET/PUT/DELETE own account
      tokens.rex                      # GET/POST/DELETE personal API tokens

    // ── Admin ──────────────────────
    admin/
      _middleware.rex                 # require admin role
      users.rex                       # GET: list users
      users/
        [user-id].rex                 # GET/PUT/DELETE: manage user

    // ── Repository management ──────
    [owner]/
      index.rex                       # GET: list owner's repos
      [repo]/
        _middleware.rex               # check repo access
        settings.rex                  # GET/PUT: repo settings, branch protection
        index.rex                     # GET: repo overview (default branch tree)

        // ── Browse ─────────────────
        tree/
          [...path].rex               # GET: file/directory listing at ref
        blob/
          [...path].rex               # GET: file contents at ref
        commits/
          index.rex                   # GET: commit log for ref
          [hash].rex                  # GET: single commit detail

        // ── Git protocol ───────────
        git/
          push.rex                    # WebSocket: push handler
          fetch.rex                   # WebSocket: fetch handler
          watch.rex                   # WebSocket: ref watch
          refs.rex                    # REST: list/update refs
```

---

## 4. Data Model

All data in `db` (SQLite KV). Keys are prefixed by type for `db.list` queries.

```
// Users
user:{id}              → {id, username, email, password-hash, role, created}
username:{username}    → {id}          // unique lookup
email:{email}          → {id}          // unique lookup

// Sessions & tokens
session:{token}        → {user-id, created, expires}
api-token:{token}      → {user-id, name, created}
user-tokens:{user-id}:{token} → {name, created}  // index for listing

// Repositories
repo:{owner}/{name}    → {owner, name, description, default-branch, created, visibility}
user-repos:{owner}:    → (prefix scan lists all repos for owner)

// Access control
access:{owner}/{repo}:{user-id} → {read, write, admin}

// Branch protection
protect:{owner}/{repo}:{ref}    → {block-force, require-review}

// Git data
ref:{owner}/{repo}/{refname}    → hash
obj:{hash}                      → (in cas store, not db)
```

---

## 5. Handler Logic

### Auth & sessions

**Signup** (`auth/signup.rex` — POST):
- Validate username/email uniqueness via `db.get("username:{name}")`
- Hash password via `crypto.hash("sha256", password + salt)`
- Store user, username index, email index
- Return session token

**Login** (`auth/login.rex` — POST):
- Look up user by username, verify password hash
- Create session token via `crypto.random(32)`
- Store in `db.set("session:{token}", ...)` with expiry
- Return token

**Logout** (`auth/logout.rex` — POST):
- Delete session from db

**API middleware** (`api/_middleware.rex`):
- Extract bearer token from `headers.authorization`
- Look up via `db.get("session:{token}")` or `db.get("api-token:{token}")`
- Rate limit via `kv.incr("rate:{user-id}")` with 60s TTL window
- Set `user` variable for downstream handlers

### User management

**Account** (`api/account.rex`):
- GET: return own profile
- PUT: update email, password
- DELETE: remove account and all owned repos

**API tokens** (`api/tokens.rex`):
- GET: list user's API tokens via `db.list("user-tokens:{user-id}:")`
- POST: create new token
- DELETE: revoke token

### Admin

**Admin middleware** (`admin/_middleware.rex`):
- Check `user.role == "admin"`, 403 otherwise

**User management** (`admin/users.rex`, `admin/users/[user-id].rex`):
- List all users via `db.list("user:")`
- View/edit/delete individual users
- First user created becomes admin (bootstrap)

### Repository management

**Create repo** (`[owner]/index.rex` — POST):
- Verify `owner` matches authenticated user (or user is admin)
- Store repo metadata, initialize default branch ref
- Create access entry with admin permissions

**Repo settings** (`[owner]/[repo]/settings.rex`):
- GET: return repo config, branch protection rules
- PUT: update description, default branch, protection rules
- DELETE: remove repo and all refs/objects

### Repository browsing

**Commit log** (`[owner]/[repo]/commits/index.rex`):
- Resolve ref from `query.ref` or default branch
- Walk commit chain via `cas.get` + `git.decode`, collect commits
- Return list with hash, message, author, time
- Pagination via `query.after` (commit hash)

**Commit detail** (`[owner]/[repo]/commits/[hash].rex`):
- `cas.get(hash)` + `git.decode` → commit object
- Show message, author, parent hashes, tree hash

**Tree listing** (`[owner]/[repo]/tree/[...path].rex`):
- Resolve ref from `query.ref` or default branch
- Walk path segments through tree objects via `git.decode`
- Return entries with name, mode, hash, type
- Discriminated union narrowing on `git.decode` to handle tree vs blob

**File contents** (`[owner]/[repo]/blob/[...path].rex`):
- Same path resolution as tree
- Final object must be a blob — return size and content hash
- For text files, host could provide `binary.to-string(encoding)` or content is served as download

### Git protocol (`[owner]/[repo]/git/`)

**Push** (`push.rex`):

WebSocket transform script. Text frames are control messages; binary frames handled by host.

**On new push stream** (`msg.ref` and `msg.new` present):
1. Check branch protection rules via `db.get("protect:{repo}:{ref}")`
2. Validate ref update mode:
   - `msg.old` set → force-with-lease: verify current ref matches `msg.old`
   - `msg.force` → skip ancestry check
   - Default → `git.is-ancestor(current, msg.new)` for fast-forward
3. Seed expect set via `ws.expect(msg.id, msg.new)`

**On stream completion** (host sends `{"status": "complete"}` when expect set empties):
1. Perform ref update via `db.cas` / `db.set` depending on mode
2. Publish ref change to watchers via `kv.publish("watch:{repo}", ...)`
3. Return success/error control message

**Fetch** (`fetch.rex`):

1. Client requests refs → `db.list("ref:{repo}/{prefix}")` → return matching refs
2. Binary want/object frames handled entirely by host
3. Client sends `{"status": "done"}` when finished

**Watch** (`watch.rex`) — protocol extension:

New endpoint: `wss://<host>/repos/:owner/:repo/watch`

```
Client → Server:  {"subscribe": true, "prefix": "refs/heads/"}
Server → Client:  {"status": "refs", "refs": {"refs/heads/main": "abc..."}}
Server → Client:  {"ref": "refs/heads/main", "old": "abc...", "new": "def...",
                    "user": "tim", "time": 1712345678000}
```

1. Client subscribes with optional prefix filter
2. Server sends current matching refs as initial state
3. Push handlers publish ref changes via `kv.publish("watch:{repo}", ...)`
4. Watch connections receive events through existing KV pub/sub
5. Optional server-side prefix filtering via `kv.set("watch-filter:{repo}:{ws.id}", prefix)`

No new host infrastructure — uses existing pub/sub.

**REST refs** (`refs.rex`):

HTTP GET/PUT for tools that don't need WebSocket. Same ref update logic as push handler (branch protection, CAS, watch notification).

---

## 6. Host Binary Frame Loop

Not Rex — this runs in the host (Rust/Go/etc.):

**Push:** receive binary frame → verify hash in expect set → `git.verify` → `cas.put` → `git.children` → add missing children to expect set → when empty, fire `{"status": "complete"}` to Rex

**Fetch:** receive binary want frame → `cas.get` per hash → send as binary frames

---

## 7. Testing Strategy

1. Write `git-ws.rexd` domain interface first
2. Write all Rex handlers against that interface
3. `rex check --domain git-ws.rexd` validates type correctness across all handlers
4. Implement discriminated union narrowing, verify `git.decode` handlers type-check cleanly
5. Test branch protection / auth logic independently via unit Rex scripts

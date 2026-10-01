# WebSocket Object Sync Protocol for Git

> [!NOTE]
> An experimental alternative to the Git Smart HTTP protocol, not a replacement.
> Both protocols can coexist, sharing the same backing object store.

Instead of packfiles, clients sync individual git objects over WebSocket. The
server becomes a thin proxy over any content-addressable object store — no git
library needed server-side.

|                   | Smart HTTP (current)                | WebSocket (proposed)                          |
|-------------------|-------------------------------------|-----------------------------------------------|
| **Server memory** | O(repo size) — full packfile in RAM | O(1) — one object at a time                   |
| **Server CPU**    | Delta compression/decompression     | Near zero                                     |
| **Server code**   | Full git protocol library           | Small handler + WebSocket lib                 |
| **Push latency**  | 2 HTTP requests + pack assembly     | Continuous stream, bounded by wavefront depth |
| **Resumability**  | Retry entire packfile               | Resume from where it stopped                  |

---

## URL Scheme

A [git remote helper](https://git-scm.com/docs/gitremote-helpers) named
`git-remote-wsgit` teaches git the `wsgit://` URL scheme. The URL includes the
hostname so the protocol is not tied to any single provider:

```sh
git remote add origin wsgit://git.example.com/my-team/my-project
git push origin main
git clone wsgit://git.example.com/my-team/my-project
```

The remote helper resolves the URL to a WebSocket connection:

```
wsgit://git.example.com/my-team/my-project
  → wss://git.example.com/repos/my-team/my-project/push
  → wss://git.example.com/repos/my-team/my-project/fetch
```

Auth is passed as a bearer token during the HTTP upgrade handshake. How the
token is obtained is provider-specific (environment variable, credential helper,
config file, etc.).

---

## Protocol

Two WebSocket endpoints. **Frame types** — binary for data, text for JSON
control messages.

### Push: `wss://<host>/repos/:owner/:repo/push`

```
Binary frames (object):
  [type byte][20-byte SHA-1][zstd-compressed body]

  Type bytes:  1=commit  2=tree  3=blob  4=tag  5=delta
  Body:        canonical git object (type 1-4)
               or [20-byte base SHA-1][delta instructions] (type 5)

Binary frames (want):
  [20-byte SHA-1]×N  (concatenated hashes)

Text frames (JSON control):
  Client → Server:  {"id": 1, "ref": "refs/heads/main", "new": "def..."}
  Server → Client:  {"id": 1, "status": "done", ...}  or  {"id": 1, "status": "error", ...}
```

#### Push flow

1. Client sends a **control message** declaring the ref update and target hash
2. Server seeds an **expect set** with the target hash
3. Client streams objects as binary frames (depth-first order)
4. For each object, the server:
   - Validates the 20-byte hash prefix is in the expect set (fast reject if not)
   - Verifies content hash matches the declared hash (abort on mismatch)
   - Stores the object
   - Parses to discover children, checks store for each, adds missing to expect set
5. When expect set is empty, update the ref based on mode:
   - **Default:** verify new commit is a descendant of current ref, then conditional put
   - **`force: true`:** skip ancestry check, unconditional put
   - **`old` provided:** conditional put requiring ref equals `old` (force-with-lease)

> [!IMPORTANT]
> The server **never stores an unexpected object**. The expect set is derived entirely
> from the declared commit graph. No orphans, no garbage collection needed.

#### Control messages

```jsonc
// Fast-forward push — server verifies new is a descendant of current ref
{"id": 1, "ref": "refs/heads/main", "new": "def456..."}

// Force push — server skips ancestry check
{"id": 1, "ref": "refs/heads/main", "new": "def456...", "force": true}

// Force-with-lease — server does compare-and-swap against old
{"id": 1, "ref": "refs/heads/main", "new": "def456...", "old": "abc123..."}

// Success (server → client)
{"id": 1, "status": "done", "ref": "refs/heads/main", "hash": "def456..."}

// Rejected — not a fast-forward (server → client)
{"id": 1, "status": "error", "message": "non-fast-forward", "current": "fff..."}

// Conflict — force-with-lease failed, ref changed since old (server → client)
{"id": 1, "status": "error", "message": "ref conflict", "expected": "abc...", "actual": "fff..."}

// Hash mismatch — corrupt or malicious data (server → client)
{"id": 1, "status": "error", "message": "hash mismatch", "expected": "abc...", "got": "def..."}
```

> [!TIP]
> On conflict or non-fast-forward rejection, the client can retry **on the same
> connection** with a new control message. All objects are already stored.

#### Concurrent streams

Multiple ref updates can be in flight on one connection. Each has a unique
client-assigned `id`. Objects are routed to the queue containing their hash.
Shared objects (branch + tag pointing to same commit) only need to be sent once.

<details>
<summary><strong>Example: push branch + tag concurrently</strong></summary>

```
Client                              Server
──────                              ──────
{"id":1,"ref":"refs/heads/main",
 "new":"def456"}                    [stream 1 expect: {def456}]
{"id":2,"ref":"refs/tags/v1.0",
 "new":"def456"}                    [stream 2 expect: {def456}]

[def456][commit] ───────────────►  [store, parse → tree aaa]
                                    [stream 1: {aaa}]
                                    [stream 2: complete → update tag]
                 ◄──────────────── {"id":2,"status":"done"}

[aaa][tree] ────────────────────►  [store, parse → bbb,ccc,ddd]
                                    [ccc exists in store]
                                    [stream 1: {bbb, ddd}]
[bbb][blob] ────────────────────►  [store, stream 1: {ddd}]
[ddd][blob] ────────────────────►  [store, stream 1: empty]
                                    [update ref main → def456]
                 ◄──────────────── {"id":1,"status":"done"}
```

</details>

---

### Fetch: `wss://<host>/repos/:owner/:repo/fetch`

```
Binary frames use the same format as push (see above), with directions reversed:
  Client → Server:  want frames (concatenated hashes)
  Server → Client:  object frames ([type][hash][body])

Text frames (JSON control):
  Client → Server:  {"id": 1, "ref": "refs/heads/main"}      (ref prefix filter)
  Server → Client:  {"id": 1, "status": "refs", "refs": {...}}
  Client → Server:  {"id": 1, "status": "done"}               (client finished)
```

#### Fetch flow

1. Client sends a **control message** with a ref prefix filter
2. Server resolves matching refs → sends `refs` control message
3. Client compares against local state, sends binary want frames
4. Server pipelines object responses (hash prefix + zstd content)
5. Client parses objects, discovers more needs, sends more wants
6. Client sends `done` when finished

<details>
<summary><strong>Example: pull latest</strong></summary>

```
Client                              Server
──────                              ──────
{"id":1,"ref":"refs/heads/main"}
                 ◄──────────────── {"id":1,"status":"refs",
                                    "refs":{"refs/heads/main":"def456..."}}
[def456] (20 bytes) ───────────►
                 ◄──────────────── [def456][commit]
[aaa] (20 bytes) ──────────────►
                 ◄──────────────── [aaa][tree]
[bbb|ddd] (40 bytes) ──────────►
                 ◄──────────────── [bbb][blob]
                 ◄──────────────── [ddd][blob]
{"id":1,"status":"done"} ──────►
```

</details>

---

## Why This Scales Better

### Push: streaming instead of packfile assembly

The client streams objects as it walks its own graph. The server checks its
object store for each child and only requests what's missing — no need to
assemble or unpack a complete packfile on either side.

> [!TIP]
> **Optimizing existence checks:** At connection start, the server can list all
> objects for the repo and build an in-memory hash set. Until the listing
> completes, individual existence checks cover the first few objects. Once the
> set is ready, all subsequent checks are local. This avoids sending unnecessary
> "want" frames to clients on slow connections.

### Fetch: wavefront propagation, not packfile assembly

> [!IMPORTANT]
> Smart HTTP fetch has a **hidden latency cost**: the server reads ALL objects,
> computes delta chains, and assembles a complete packfile *before sending byte one*.
> For large repos this takes seconds. This protocol starts sending immediately.

Latency is bounded by object graph depth (typically 3-5 levels), not object
count. Each level of the tree fans out into parallel store lookups. Exact
numbers depend on the client's connection and store response times — worth
measuring with a prototype.

### Compression: zstd instead of zlib

The remote helper uses git's `import`/`export` capability, which provides raw
uncompressed data — the protocol is free to choose any wire compression. zstd is
faster at both compression and decompression while achieving better ratios than
zlib.

### Resumability is free

Objects are stored individually as they arrive. If a push fails mid-stream, the
client reconnects and re-sends the control message. The server walks the graph,
finds most objects already present, and only needs the remainder.

---

## Optional Extension: Delta Objects

Binary frames may carry a **delta** instead of a full object, using type byte
`5` in the standard frame format. The zstd-compressed body contains the 20-byte
base hash followed by delta instructions (git's REF_DELTA copy/insert format).
All implementations **must decode** deltas; creating them is optional.

### Decoding

When a receiver gets a delta frame:

1. Check if the base object is available locally (object store or local repo)
2. **Yes** → apply delta, store/process the resulting full object
3. **No** → buffer the delta, request the base via a want frame

The base always exists *somewhere* — it's from a previous commit. On push, the
server reads it from its own store. On fetch, the client either has it locally
or requests it, adding one extra wavefront generation.

### When to create deltas

Deltas trade bandwidth for wavefront depth. Encoders should only emit a delta
when the savings are substantial — for example, a large tree object where a
single entry changed between commits. Heuristics:

- Skip deltas for small objects (the overhead isn't worth the extra round trip)
- Skip deltas where most of the content changed (the delta is barely smaller)
- Good candidates: tree objects at the same path in parent vs child commit,
  where typically one entry was added, removed, or updated

---

## Prior Art

Evolved from the [lit](https://luvit.io/lit.html) package manager protocol
(WebSocket object sync over a content-addressable store). Lit demonstrated
faster package transfers than npm in production despite running on a single
server vs npm's CDN.

---

## Server Requirements

The protocol assumes a server backed by:

1. **A content-addressable object store** — any store that can put/get/check by
   SHA-1 hash (S3, GCS, Azure Blob, local filesystem, etc.)
2. **A ref store with conditional writes** — any store supporting
   compare-and-swap on key-value pairs (DynamoDB, etcd, PostgreSQL, Redis, etc.)

The server needs no git library. Object parsing is limited to extracting child
hashes from commits (parent + tree) and trees (entry hashes) — roughly 50 lines
of code in any language.

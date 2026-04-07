# git-remote-ws — Rex CLI Client

A git remote helper that speaks the [WebSocket object sync protocol](../git-ws/proposed-git-websocket-protocol.md), built on a new `rex-script` CLI host runtime.

## Architecture

Three things:

1. **`rex-script`** — A general-purpose CLI host for Rex (like `rex-serve` is for web servers). Knows nothing about git.
2. **`rex-git`** — Shared git object opcodes. Pure functions on blob data — no I/O, no state. Used by both rex-serve and rex-script.
3. **`git-remote-ws`** — A Rex script that implements the git remote helper protocol.

### Design Principle

**Rex works at the symbolic level.** It sees objects, strings, hashes, and arrays — never raw bytes, wire formats, or binary headers. The host handles all encoding/decoding. Rex decides *what* to send; the host decides *how* to encode it.

```
┌──────────────────────────────────────────┐
│  git-remote-ws.rex  (protocol logic)     │  Rex: objects & strings only
├──────────────┬───────────────────────────┤
│  rex-git     │  rex-script              │  Rust: binary encoding,
│  (git obj    │  (proc, fs, ws, blob,  │  process I/O, WebSocket
│   opcodes)   │   stdin/stdout, env)     │  framing, zstd, SHA-1
├──────────────┴───────────────────────────┤
│  rex-core  (compiler, interpreter, heap) │
└──────────────────────────────────────────┘
```

---

## 1. rex-git — Shared Git Object Crate

Pure opcode functions. No I/O. Already implemented in rex-serve, extracted into a shared crate.

```
// Decode raw git object bytes into a typed Rex object
extern git.decode(data: blob) -> GitObject

// Encode a structured Rex object back to git object format
extern git.encode(obj: GitCommit | GitTree | GitTag) -> blob
extern git.encode-blob(data: blob) -> blob

// Extract child hashes from raw git object content
// Accepts content bytes + type string (no header construction needed)
extern git.children-of(type: str, data: blob) -> [str]

// Verify hash matches data. Returns hash on match, none on mismatch.
extern git.verify(hash: str, data: blob) -> str | none
```

**Change from current API:** `git.children(data: blob) -> [str]` requires the full git object format with header. Replace with `git.children-of(type, data)` that accepts the type as a separate string and raw content bytes. The host prepends the header internally. Rex never touches `"commit 234\0..."`.

### What moves out of rex-serve

From `crates/rex-serve/src/opcodes.rs`:
- `op_git_decode`, `decode_commit`, `decode_tree`, `decode_tag`, `decode_ident`
- `op_git_encode`, `encode_commit_obj`, `encode_tree_obj`, `encode_tag_obj`
- `op_git_encode_blob`
- `op_git_children` → refactored as `op_git_children_of`
- `op_git_verify`
- Helper functions: `format_ident`, `obj_field_str`, `obj_field_val`, `obj_field_i64`

Expose a registration function:

```rust
pub fn register_opcodes(opcodes: &mut HashMap<String, OpcodeFn>) {
    opcodes.insert("gd".into(), op_git_decode);
    opcodes.insert("ge".into(), op_git_encode);
    opcodes.insert("gB".into(), op_git_encode_blob);
    opcodes.insert("gf".into(), op_git_children_of);
    opcodes.insert("gv".into(), op_git_verify);
}
```

`git.is-ancestor` stays in rex-serve (reads from CAS database — not pure).

---

## 2. rex-script Host Runtime

Generic CLI host. No domain knowledge.

### Externs

#### Program interface

```
extern args: [str]                          // command line arguments
extern env: {*: str}                        // environment variables
extern exit(code: int) -> never
```

#### Standard I/O

```
extern stdin.line() -> str | none           // read one line (none at EOF)
extern stdin.all() -> str | none            // read all remaining input
extern stdout.write(text: str) -> str       // write, return value
extern stdout.line(text: str) -> str        // write + newline
extern stderr.write(text: str) -> str
extern stderr.line(text: str) -> str
```

#### Filesystem

```
extern fs.read(path: str) -> str | none
extern fs.read-bytes(path: str) -> blob | none
extern fs.write(path: str, content: str) -> str
extern fs.write-bytes(path: str, data: blob) -> str
extern fs.append(path: str, content: str) -> str
extern fs.delete(path: str) -> str | none
extern fs.exists(path: str) -> str | none
extern fs.mkdir(path: str) -> str
extern fs.list(path: str) -> [str]
extern fs.glob(pattern: str) -> [str]
```

#### Process spawning

Simple (blocking):

```
extern proc.run(cmd: str, args: [str]) -> ProcResult
type ProcResult = {status: int, stdout: str, stderr: str}

extern proc.lines(cmd: str, args: [str]) -> [str]
extern proc.output(cmd: str, args: [str]) -> str | none
```

Persistent (streaming):

```
extern proc.spawn(cmd: str, args: [str]) -> int          // returns handle
extern proc.write(pid: int, data: str) -> str
extern proc.write-bytes(pid: int, data: blob) -> blob
extern proc.read-line(pid: int) -> str | none
extern proc.read-bytes(pid: int, n: int) -> blob | none
extern proc.close-stdin(pid: int) -> int
extern proc.wait(pid: int) -> int                         // returns exit code
```

#### WebSocket client

```
extern ws.connect(url: str, headers: {*: str}) -> int | none
extern ws.send(conn: int, message: str) -> str
extern ws.recv(conn: int) -> str | none
extern ws.send-binary(conn: int, data: blob) -> blob
extern ws.recv-binary(conn: int) -> blob | none
extern ws.close(conn: int) -> none
```

#### Blob utilities

Rex doesn't manipulate blob data directly, but it needs to convert between blobs and its own types:

```
extern blob.from-hex(hex: str) -> blob | none
extern blob.to-hex(data: blob) -> str
extern blob.from-str(text: str) -> blob
extern blob.to-str(data: blob) -> str | none
extern blob.zstd-compress(data: blob) -> blob
extern blob.zstd-decompress(data: blob) -> blob | none
```

#### JSON, logging, crypto

```
extern json.parse(text: str) -> some
extern json.stringify(value: some) -> str
extern log.info(message: some)                // → stderr
extern log.warning(message: some)
extern log.error(message: some)
extern crypto.hash(algorithm: str, data: str) -> str
extern crypto.hash-bytes(algorithm: str, data: blob) -> str
```

---

## 3. Wire Format — Host-Level

The WebSocket protocol uses binary frames with a specific layout. This encoding/decoding is done by the host, not Rex. Rex passes objects; the host serializes.

These are **rex-script generic opcodes** — they handle structured binary framing, not git-specific logic:

```
// Pack: {type: int, hash: str, data: blob} → binary frame
// [1 byte type][20 byte hash][zstd compressed data]
extern frame.pack(type: int, hash: str, data: blob) -> blob

// Unpack: binary frame → {type: int, hash: str, data: blob}
extern frame.unpack(data: blob) -> {type: int, hash: str, data: blob}

// Pack a list of hex hashes into concatenated 20-byte binary
extern frame.pack-hashes(hashes: [str]) -> blob

// Unpack concatenated 20-byte hashes back to hex strings
extern frame.unpack-hashes(data: blob) -> [str]
```

Actually — these are specific to this wire protocol. They shouldn't be in the generic host either. Better approach: put them in `rex-git` since they're part of the git WebSocket protocol. Or even simpler: **the WebSocket send/recv functions handle the framing**.

### Revised: WebSocket with typed messages

Instead of raw binary send/recv, the WebSocket interface understands the protocol's frame types:

```
// Send a git object over WebSocket (host builds the binary frame)
extern ws.send-object(conn: int, type: str, hash: str, data: blob) -> str

// Receive a git object from WebSocket (host parses the binary frame)
extern ws.recv-object(conn: int) -> {type: str, hash: str, data: blob} | none

// Send want hashes (host builds concatenated 20-byte frame)
extern ws.send-wants(conn: int, hashes: [str]) -> [str]
```

No — this makes the WebSocket module git-specific. Let me think again.

### Final approach: frame helpers in rex-git

The generic WebSocket module stays generic (send/recv binary blobs). Frame encoding is in `rex-git` since it's part of the git WebSocket protocol:

```
// rex-git additions:
extern git.pack-frame(type: str, hash: str, data: blob) -> blob
extern git.unpack-frame(frame: blob) -> {type: str, hash: str, data: blob}
extern git.pack-wants(hashes: [str]) -> blob
extern git.unpack-wants(frame: blob) -> [str]
```

Rex passes objects in, gets objects out. The host handles type-byte mapping, hex→binary hash conversion, and zstd compression. Rex never sees raw frame bytes.

---

## 4. git-remote-ws Script

### Git plumbing helpers

```rex
// ── Persistent cat-file for bulk object reads ────────────────────

cat-file = proc.spawn("git", ["cat-file", "--batch"])

git-cat = (hash) do
  proc.write(cat-file, hash + "\n")
  header = proc.read-line(cat-file)
  unless header do return none end

  parts = header.split(" ")
  when parts.1 == "missing" do return none end

  size = isInteger(parts.2)
  unless size do return none end

  data = proc.read-bytes(cat-file, size)
  proc.read-bytes(cat-file, 1)  // trailing newline
  {type: parts.1, size: size, data: data}
end

// ── One-shot git commands ────────────────────────────────────────

git-hash-object = (type, data) do
  p = proc.spawn("git", ["hash-object", "-w", "--stdin", "-t", type])
  proc.write-bytes(p, data)
  proc.close-stdin(p)
  hash = proc.read-line(p)
  proc.wait(p)
  hash
end

git-rev-list = (include, exclude) do
  args = ["rev-list", "--objects"] + include
  for hash in exclude do
    args = args + ["^" + hash]
  end
  proc.lines("git", args)
end

git-update-ref = (ref, hash) do
  proc.run("git", ["update-ref", ref, hash])
end

git-rev-parse = (rev) do
  result = proc.output("git", ["rev-parse", "--verify", "--quiet", rev])
  when result do result.trim() end
end
```

### Main program

```rex
remote = args.0
url = args.1

base-url = url.replace("wsgit://", "wss://")

remote-refs = {}

auth-headers = {}
when token = env.GIT_WS_TOKEN do
  auth-headers = {authorization: `Bearer ${token}`}
end

// ── Command loop ──────────────────────────────────────────────────

while line = stdin.line() do
  when line == "capabilities" do
    stdout.line("push")
    stdout.line("fetch")
    stdout.line("")
  end

  when line == "list" or line == "list for-push" do
    list-refs()
    stdout.line("")
  end

  when line.starts-with("push ") do
    pushes = [line]
    while next = stdin.line() do
      unless next do break end
      pushes = pushes + [next]
    end
    do-push(pushes)
    stdout.line("")
  end

  when line.starts-with("fetch ") do
    fetches = [line]
    while next = stdin.line() do
      unless next do break end
      fetches = fetches + [next]
    end
    do-fetch(fetches)
    stdout.line("")
  end
end

proc.close-stdin(cat-file)
proc.wait(cat-file)
```

### list-refs

```rex
list-refs = () do
  conn = ws.connect(`${base-url}/fetch`, auth-headers)
  unless conn do
    stderr.line("error: failed to connect")
    return none
  end

  ws.send(conn, json.stringify({id: 1, ref: ""}))
  response = json.parse(ws.recv(conn))
  when response.status == "refs" do
    remote-refs = response.refs
    for name of response.refs do
      stdout.line(`${response.refs.(name)} ${name}`)
    end
  end

  ws.send(conn, json.stringify({id: 1, status: "done"}))
  ws.close(conn)
end
```

### do-push

```rex
do-push = (lines) do
  conn = ws.connect(`${base-url}/push`, auth-headers)
  unless conn do
    stderr.line("error: failed to connect")
    return none
  end

  for line in lines do
    spec = line.slice(5, line.size)
    force = spec.starts-with("+")
    when force do spec = spec.slice(1, spec.size) end

    parts = spec.split(":")
    src = parts.0
    dst = parts.1

    local-hash = git-rev-parse(src)
    unless local-hash do
      stdout.line(`error ${dst} unknown source ref`)
      continue
    end

    exclude = []
    when remote-hash = remote-refs.(dst) do
      exclude = [remote-hash]
    end
    objects = git-rev-list([local-hash], exclude)

    msg = {id: 1, ref: dst, new: local-hash}
    when force do msg.force = true end
    when remote-refs.(dst) do msg.old = remote-refs.(dst) end
    ws.send(conn, json.stringify(msg))

    // Stream each object as a binary frame
    for obj-line in objects do
      hash = obj-line.split(" ").0
      obj = git-cat(hash)
      when obj do
        ws.send-binary(conn, git.pack-frame(obj.type, hash, obj.data))
      end
    end

    result = json.parse(ws.recv(conn))
    when result.status == "done" do
      stdout.line(`ok ${dst}`)
    end
    when result.status == "error" do
      stdout.line(`error ${dst} ${result.message}`)
    end
  end

  ws.close(conn)
end
```

### do-fetch

```rex
do-fetch = (lines) do
  targets = [{
    hash: line.split(" ").1
    ref: line.split(" ").2
  } for line in lines]

  conn = ws.connect(`${base-url}/fetch`, auth-headers)
  unless conn do
    stderr.line("error: failed to connect")
    return none
  end

  ws.send(conn, json.stringify({id: 1, ref: ""}))
  json.parse(ws.recv(conn))  // consume refs response

  // Request objects we don't have
  wants = [t.hash for t in targets and git-cat(t.hash) == none]
  when wants.size > 0 do
    ws.send-binary(conn, git.pack-wants(wants))
  end

  // Receive objects, discover children, request missing
  while frame = ws.recv-binary(conn) do
    obj = git.unpack-frame(frame)
    unless obj do continue end

    // Store in local repo
    git-hash-object(obj.type, obj.data)

    // Discover children and request any we're missing
    children = git.children-of(obj.type, obj.data)
    missing = [h for h in children and git-cat(h) == none]
    when missing.size > 0 do
      ws.send-binary(conn, git.pack-wants(missing))
    end
  end

  ws.send(conn, json.stringify({id: 1, status: "done"}))
  ws.close(conn)

  for target in targets do
    git-update-ref(target.ref, target.hash)
  end
end
```

Notice: **zero blob manipulation in Rex.** The script works entirely with strings (hashes, types, ref names) and opaque blobs (passed between host functions). `git.pack-frame` takes a type string, hash string, and blob data — returns an opaque blob. `git.unpack-frame` takes an opaque blob — returns an object with string fields and blob data. `git.children-of` takes a type string and blob content — returns an array of hash strings. Rex never constructs blobs, never inspects bytes, never thinks about headers.

---

## 5. File Structure

```
crates/rex-git/
  Cargo.toml                       # depends on rex-core, sha1, hex, zstd
  src/lib.rs                        # git object opcodes + frame helpers

crates/rex-script/
  Cargo.toml                       # depends on rex-core, rex-git
  src/
    main.rs                         # entry point, script loading
    opcodes.rs                      # generic host functions
    refs.rs                         # host objects (Stdin, Stdout, Env)

examples/git-remote-ws/
  PLAN.md
  git-remote-ws.rexd                # domain type interface
  git-remote-ws.rex                 # main script
```

### Updated rex-serve

```toml
# crates/rex-serve/Cargo.toml
[dependencies]
rex-git.workspace = true
```

rex-serve calls `rex_git::register_opcodes(&mut opcodes)` and removes its inline git functions.

---

## 6. What lives where

| Concern | Where | Rex sees |
|---------|-------|----------|
| Git object decode/encode | `rex-git` | Objects with typed fields |
| Git child hash extraction | `rex-git` (`children-of`) | Array of hash strings |
| Wire frame pack/unpack | `rex-git` | Object in, opaque blob out (and reverse) |
| Want frame pack/unpack | `rex-git` | Array of hash strings ↔ opaque blob |
| zstd, SHA-1, hex, type-byte mapping | `rex-git` internals | Never |
| Process spawning, stdin/stdout | `rex-script` | String/blob I/O |
| WebSocket connect/send/recv | `rex-script` | String + blob |
| Git CLI (cat-file, hash-object, rev-list) | Rex script | String commands via `proc.*` |
| Remote helper protocol dispatch | Rex script | Line-oriented text protocol |
| Ref negotiation, object streaming logic | Rex script | Hashes, type strings, opaque data |

---

## 7. Testing Strategy

1. Extract `rex-git` crate from rex-serve, verify rex-serve still passes tests
2. Implement `rex-script` crate with core externs
3. Write `git-remote-ws.rexd` and `git-remote-ws.rex`
4. Test: `echo "capabilities" | rex-script git-remote-ws.rex origin wsgit://...`
5. Integration: start git-ws server, push/fetch via the helper
6. End-to-end: `git clone wsgit://localhost:3000/tim/test`

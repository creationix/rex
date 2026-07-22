// Middleware provides:
extern user: some | none
extern owner: str
extern repo-name: str
extern can-write: bool

// WebSocket: push handler
// Text frames are control messages; binary frames handled by host.

unless user and can-write do
  return json.stringify({status: "error", message: "write_access_required"})
end

msg = json.parse(event.data)

// Stream completion — host sends this when expect set empties
when msg.status == "complete" do
  unless msg.id and msg.ref and msg.new do
    return json.stringify({status: "error", message: "missing_fields"})
  end
  id = `${msg.id}`
  ref = `${msg.ref}`
  new-hash = `${msg.new}`

  // Retrieve the push mode stored when the stream started
  mode = kv.get(`push-mode:${ws.id}:${id}`)

  when mode == "cas" do
    // Force-with-lease: compare-and-swap
    old = kv.get(`push-old:${ws.id}:${id}`)
    ref-key = `ref:${owner}/${repo-name}/${ref}`
    when old do
      when conflict = db.cas(ref-key, old, new-hash) do
        return json.stringify({
          id: id, status: "error", message: "ref conflict"
          expected: old, actual: conflict
        })
      end
    end
  end

  when mode == "force" do
    db.set(`ref:${owner}/${repo-name}/${ref}`, new-hash)
  end

  when mode == "ff" do
    db.set(`ref:${owner}/${repo-name}/${ref}`, new-hash)
  end

  // Publish ref change for watchers
  kv.publish(`watch:${owner}/${repo-name}`, json.stringify({
    ref: ref, old: `${msg.old-hash or ""}`, new: new-hash
    user: user.username, time: time.now()
  }))

  log.info(`push: ${owner}/${repo-name} ${ref} -> ${new-hash}`)

  return json.stringify({
    id: id, status: "done", ref: ref, hash: new-hash
  })
end

// New push stream — client declares ref update
when msg.ref and msg.new do
  ref = `${msg.ref}`
  id = `${msg.id}`
  new-hash = `${msg.new}`

  // Check branch protection
  when protection = db.get(`protect:${owner}/${repo-name}:${ref}`) do
    protection = json.parse(protection)
    when msg.force and protection.block-force do
      return json.stringify({
        id: id, status: "error", message: "force_push_blocked"
        ref: ref
      })
    end
  end

  current = db.get(`ref:${owner}/${repo-name}/${ref}`)

  when msg.old do
    old = `${msg.old}`
    // Force-with-lease: verify current ref matches old
    unless current == old do
      return json.stringify({
        id: id, status: "error", message: "ref conflict"
        expected: old, actual: current or ""
      })
    end
    kv.set(`push-mode:${ws.id}:${id}`, "cas")
    kv.set(`push-old:${ws.id}:${id}`, old)
    kv.set(`push-ref:${ws.id}:${id}`, ref)
  end

  when msg.force and (msg.old == none) do
    kv.set(`push-mode:${ws.id}:${id}`, "force")
    kv.set(`push-ref:${ws.id}:${id}`, ref)
  end

  unless msg.old or msg.force do
    // Default: fast-forward only
    when current do
      unless git.is-ancestor(current, new-hash) do
        return json.stringify({
          id: id, status: "error", message: "non-fast-forward"
          current: current
        })
      end
    end
    kv.set(`push-mode:${ws.id}:${id}`, "ff")
    kv.set(`push-ref:${ws.id}:${id}`, ref)
  end

  // Seed expect set with target hash
  stream-id = msg.id
  when stream-id do
    ws.expect(stream-id, new-hash)
  end

  // Store metadata for completion handler
  kv.set(`push-new:${ws.id}:${id}`, new-hash)
  when current do
    kv.set(`push-old-hash:${ws.id}:${id}`, current)
  end

  return json.stringify({id: id, status: "accepted"})
end

// Pass through unrecognized messages
event.data

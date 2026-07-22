// Middleware provides:
extern user: some | none
extern owner: str
extern repo-name: str
extern can-write: bool

// REST: list/update refs

when method == "GET" do
  prefix = query.prefix or ""
  entries = db.list(`ref:${owner}/${repo-name}/${prefix}`)

  refs = {}
  full-prefix = `ref:${owner}/${repo-name}/`
  for entry in entries do
    ref-name = entry.key.slice(full-prefix.size, entry.key.size)
    refs.(ref-name) = entry.value
  end

  return {ok: true, refs: refs}
end

when method == "PUT" do
  unless user and can-write do
    status = 403
    return {ok: false, error: "write_access_required"}
  end

  input = json.parse(body)
  unless input and input.ref and input.new do
    status = 422
    return {ok: false, error: "missing_fields"}
  end

  ref = `${input.ref}`
  new-hash = `${input.new}`
  ref-key = `ref:${owner}/${repo-name}/${ref}`

  // Check branch protection
  when protection = db.get(`protect:${owner}/${repo-name}:${ref}`) do
    protection = json.parse(protection)
    when input.force and protection.block-force do
      status = 403
      return {ok: false, error: "force_push_blocked"}
    end
  end

  current = db.get(ref-key)

  // Force-with-lease
  when input.old do
    old = `${input.old}`
    unless current == old do
      status = 409
      return {ok: false, error: "ref_conflict", expected: old, actual: current or ""}
    end
    when conflict = db.cas(ref-key, old, new-hash) do
      status = 409
      return {ok: false, error: "ref_conflict", expected: old, actual: conflict}
    end
  end

  // Force push
  when input.force and (input.old == none) do
    db.set(ref-key, new-hash)
  end

  // Fast-forward
  unless input.old or input.force do
    when current do
      unless git.is-ancestor(current, new-hash) do
        status = 409
        return {ok: false, error: "non_fast_forward", current: current}
      end
    end
    db.set(ref-key, new-hash)
  end

  // Notify watchers
  kv.publish(`watch:${owner}/${repo-name}`, json.stringify({
    ref: ref, old: current or "", new: new-hash
    user: user.username, time: time.now()
  }))

  return {ok: true, ref: ref, hash: new-hash}
end

status = 405
{ok: false, error: "method_not_allowed"}

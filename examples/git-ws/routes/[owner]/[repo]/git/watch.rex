// Middleware provides:
extern owner: str
extern repo-name: str

// WebSocket: ref watch — pub/sub for ref changes

msg = json.parse(event.data)

// Client subscribes to ref changes
when msg.subscribe do
  prefix = `${msg.prefix or ""}`

  // Store filter for this connection
  when prefix do
    kv.set(`watch-filter:${owner}/${repo-name}:${ws.id}`, prefix)
  end

  // Send current matching refs as initial state
  ref-prefix = `ref:${owner}/${repo-name}/${prefix}`
  entries = db.list(ref-prefix)

  refs = {}
  full-prefix = `ref:${owner}/${repo-name}/`
  for entry in entries do
    ref-name = entry.key.slice(full-prefix.size, entry.key.size)
    refs.(ref-name) = entry.value
  end

  return json.stringify({status: "refs", refs: refs})
end

// Client unsubscribes
when msg.unsubscribe do
  kv.set(`watch-filter:${owner}/${repo-name}:${ws.id}`, "")
  return json.stringify({status: "unsubscribed"})
end

// Ref change event from pub/sub — filter and forward
when msg.ref and msg.new do
  filter = kv.get(`watch-filter:${owner}/${repo-name}:${ws.id}`)
  ref-name = `${msg.ref}`

  // If no filter or ref matches prefix, forward the event
  when (filter == none) or (filter == "") or ref-name.starts-with(filter) do
    return event.data
  end

  // Filtered out — suppress
  return none
end

event.data

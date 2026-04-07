// Middleware provides:
extern owner: str
extern repo-name: str

// WebSocket: fetch handler
// Client requests refs, then binary want/object frames handled by host.

msg = json.parse(event.data)

// Client requests refs matching a prefix
when msg.ref do
  prefix = `ref:${owner}/${repo-name}/${msg.ref}`
  entries = db.list(prefix)

  refs = {}
  for entry in entries do
    // Strip the "ref:{owner}/{repo}/" prefix to get the ref name
    ref-prefix = `ref:${owner}/${repo-name}/`
    ref-name = entry.key.slice(ref-prefix.size, entry.key.size)
    refs.(ref-name) = entry.value
  end

  return json.stringify({
    id: msg.id, status: "refs", refs: refs
  })
end

// Client signals done
when msg.status == "done" do
  log.info(`fetch complete: ${owner}/${repo-name}`)
  return json.stringify({id: msg.id, status: "done"})
end

event.data

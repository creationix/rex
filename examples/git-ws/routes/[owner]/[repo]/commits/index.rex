// Middleware provides:
extern owner: str
extern repo-name: str
extern repo: some

// GET: commit log for ref

unless method == "GET" do
  status = 405
  return {ok: false, error: "method_not_allowed"}
end

ref = query.ref or repo.default-branch
ref-hash = db.get(`ref:${owner}/${repo-name}/${ref}`)
unless ref-hash do
  status = 404
  return {ok: false, error: "ref_not_found"}
end

// Walk commit chain
commits = []
current = ref-hash
limit = 50

// Skip to pagination cursor
when query.after do
  while current do
    when data = cas.get(current) do
      obj = git.decode(data)
      unless obj.type == "commit" do break end
      when current == query.after do
        // Found cursor — advance past it and start collecting
        current = obj.parents.0 or none
        break
      end
      current = obj.parents.0 or none
    end
  end
end

// Collect commits
while current and commits.size < limit do
  when hash = current do
    when data = cas.get(hash) do
      obj = git.decode(data)
      unless obj.type == "commit" do break end
      commits = commits + [{
        hash: hash
        message: obj.message
        author: obj.author
        committer: obj.committer
        parents: obj.parents
      }]
      current = obj.parents.0 or none
    end
  end
end

{ok: true, ref: ref, commits: commits}

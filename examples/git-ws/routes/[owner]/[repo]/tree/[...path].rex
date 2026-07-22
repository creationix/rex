// Middleware provides:
extern owner: str
extern repo-name: str
extern repo: some

// GET: file/directory listing at ref

unless method == "GET" do
  status = 405
  return {ok: false, error: "method_not_allowed"}
end

// Resolve ref (from query or default branch)
ref = query.ref or repo.default-branch
ref-hash = db.get(`ref:${owner}/${repo-name}/${ref}`)
unless ref-hash do
  status = 404
  return {ok: false, error: "ref_not_found"}
end

// Load commit
when commit-data = cas.get(ref-hash) do
  commit = git.decode(commit-data)

  // Walk path segments through tree objects
  current-hash = commit.tree
  segments = params.path

  for segment in segments do
    when current-hash do
      when tree-data = cas.get(current-hash) do
        obj = git.decode(tree-data)
        when obj.type == "tree" do
          found = none
          for entry in obj.entries do
            when entry.name == segment do
              found = entry.hash
              break
            end
          end
          unless found do
            status = 404
            return {ok: false, error: "path_not_found"}
          end
          current-hash = `${found}`
        end
      end
    end
  end

  // Return the final object
  when current-hash do
    when data = cas.get(current-hash) do
      obj = git.decode(data)

      when obj.type == "tree" do
        return {ok: true, type: "tree", entries: obj.entries}
      end
      when obj.type == "blob" do
        return {ok: true, type: "blob", size: obj.size, hash: current-hash}
      end

      return {ok: true, type: obj.type, hash: current-hash}
    end
  end
end

status = 404
{ok: false, error: "object_not_found"}

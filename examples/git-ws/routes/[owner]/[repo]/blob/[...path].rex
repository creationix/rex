// Middleware provides:
extern owner: str
extern repo-name: str
extern repo: some

// GET: file contents at ref

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

when commit-data = cas.get(ref-hash) do
  commit = git.decode(commit-data)

  // Walk path to find blob
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

  // Final object must be a blob
  when current-hash do
    when data = cas.get(current-hash) do
      obj = git.decode(data)

      unless obj.type == "blob" do
        status = 422
        return {ok: false, error: "not_a_blob"}
      end

      return {ok: true, hash: current-hash, size: obj.size}
    end
  end
end

status = 404
{ok: false, error: "object_not_found"}

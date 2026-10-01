// Middleware provides:
extern owner: str
extern repo-name: str
extern repo: some

// GET: repo overview — default branch tree listing

unless method == "GET" do
  status = 405
  return {ok: false, error: "method_not_allowed"}
end

// Resolve default branch
ref-hash = db.get(`ref:${owner}/${repo-name}/${repo.default-branch}`)

result = {
  ok: true
  owner: repo.owner
  name: repo.name
  description: repo.description
  default-branch: repo.default-branch
  visibility: repo.visibility
}

// If default branch exists, include its tree
when ref-hash do
  when data = cas.get(ref-hash) do
    obj = git.decode(data)
    when obj.type == "commit" do
      when tree-hash = obj.tree do
        when tree-data = cas.get(tree-hash) do
          tree = git.decode(tree-data)
          when tree.type == "tree" do
            result.tree = tree.entries
          end
        end
      end
    end
  end
end

result

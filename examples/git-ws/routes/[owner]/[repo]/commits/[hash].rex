// GET: single commit detail

unless method == "GET" do
  status = 405
  return {ok: false, error: "method_not_allowed"}
end

hash = params.hash
unless hash do
  status = 400
  return {ok: false, error: "missing_hash"}
end

data = cas.get(hash)
unless data do
  status = 404
  return {ok: false, error: "commit_not_found"}
end

obj = git.decode(data)
unless obj.type == "commit" do
  status = 422
  return {ok: false, error: "not_a_commit"}
end

{
  ok: true
  hash: hash
  tree: obj.tree
  parents: obj.parents
  message: obj.message
  author: obj.author
  committer: obj.committer
}

// Middleware provides:
extern user: some | none

// GET: list owner's repos, POST: create repo

owner = params.owner

when method == "GET" do
  entries = db.list(`repo:${owner}/`)
  repos = [ json.parse(e.value) for e in entries ]
  return {ok: true, repos: repos}
end

when method == "POST" do
  unless user do
    status = 401
    return {ok: false, error: "authentication_required"}
  end

  // Only the owner or an admin can create repos under this namespace
  unless user.username == owner or user.role == "admin" do
    status = 403
    return {ok: false, error: "forbidden"}
  end

  input = json.parse(body)
  unless input and input.name do
    status = 422
    return {ok: false, error: "missing_name"}
  end

  repo-key = `repo:${owner}/${input.name}`
  when db.get(repo-key) do
    status = 409
    return {ok: false, error: "repo_exists"}
  end

  now = time.now()
  repo = {
    owner: owner
    name: input.name
    description: input.description or ""
    default-branch: "refs/heads/main"
    created: now
    visibility: input.visibility or "private"
  }

  db.set(repo-key, json.stringify(repo))

  // Grant admin access to owner
  db.set(`access:${owner}/${input.name}:${user.id}`, json.stringify({
    read: true, write: true, admin: true
  }))

  status = 201
  return {ok: true, repo: repo}
end

status = 405
{ok: false, error: "method_not_allowed"}

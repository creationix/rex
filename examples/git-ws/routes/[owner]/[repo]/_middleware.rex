// Middleware provides:
extern user: some | none

// Repo middleware — verify repo exists and check access

owner = params.owner
repo-name = params.repo

repo-record = db.get(`repo:${owner}/${repo-name}`)
unless repo-record do
  status = 404
  return {ok: false, error: "repo_not_found"}
end
repo = json.parse(repo-record)

// Check access
can-read = false
can-write = false
can-admin = false

// Public repos are readable by anyone
when repo.visibility == "public" do
  can-read = true
end

// Check user-specific access
when user do
  when access-record = db.get(`access:${owner}/${repo-name}:${user.id}`) do
    access = json.parse(access-record)
    when access.read do can-read = true end
    when access.write do can-write = true end
    when access.admin do can-admin = true end
  end

  // Repo owner and admins get full access
  when user.username == owner or user.role == "admin" do
    can-read = true
    can-write = true
    can-admin = true
  end
end

unless can-read do
  status = 404
  return {ok: false, error: "repo_not_found"}
end

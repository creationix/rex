// Middleware provides:
extern owner: str
extern repo-name: str
extern repo: some
extern can-admin: bool

// GET/PUT: repo settings and branch protection

unless can-admin do
  status = 403
  return {ok: false, error: "admin_access_required"}
end

when method == "GET" do
  // Collect branch protection rules
  protections = db.list(`protect:${owner}/${repo-name}:`)
  rules = [ {ref: e.key, rule: json.parse(e.value)} for e in protections ]

  return {
    ok: true
    owner: repo.owner
    name: repo.name
    description: repo.description
    default-branch: repo.default-branch
    visibility: repo.visibility
    protections: rules
  }
end

when method == "PUT" do
  input = json.parse(body)
  unless input do
    status = 422
    return {ok: false, error: "invalid_body"}
  end

  when input.description do
    repo.description = input.description
  end
  when input.default-branch do
    repo.default-branch = input.default-branch
  end
  when input.visibility do
    repo.visibility = input.visibility
  end

  db.set(`repo:${owner}/${repo-name}`, json.stringify(repo))

  // Update branch protection rules
  when input.protections do
    for rule in input.protections do
      when rule.ref do
        db.set(`protect:${owner}/${repo-name}:${rule.ref}`, json.stringify({
          block-force: rule.block-force or false
          require-review: rule.require-review or false
        }))
      end
    end
  end

  return {ok: true}
end

status = 405
{ok: false, error: "method_not_allowed"}

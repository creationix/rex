// GET: list all users

unless method == "GET" do
  status = 405
  return {ok: false, error: "method_not_allowed"}
end

entries = db.list("user:")
users = [
  json.parse(e.value) for e in entries
]

// Strip sensitive fields
result = [{
  id: u.id
  username: u.username
  email: u.email
  role: u.role
  created: u.created
} for u in users]

{ok: true, users: result}

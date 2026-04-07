// Middleware provides:
extern token: str | none

// POST: revoke session token
unless method == "POST" do
  status = 405
  return {ok: false, error: "method_not_allowed"}
end

unless token do
  status = 401
  return {ok: false, error: "not_authenticated"}
end

db.delete(`session:${token}`)
{ok: true}

// Middleware provides:
extern user: some | none

// Admin middleware — require admin role

unless user do
  status = 401
  return {ok: false, error: "authentication_required"}
end

unless user.role == "admin" do
  status = 403
  return {ok: false, error: "admin_required"}
end

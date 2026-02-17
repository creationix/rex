/* Authentication and authorization policy matrix */

request-id = headers.x-request-id or trace-id()
route-key = method + " " + path

policies = {
  "GET /v1/me": {auth: "session", required-role: "user"}
  "GET /v1/admin/audit": {auth: "session", required-role: "admin"}
  "POST /v1/internal/reindex": {auth: "api-key", required-scope: "ops:reindex"}
  "POST /v1/webhooks/provider": {auth: "signature", provider: "acme"}
}

policy = policies.(route-key)
auth-ok = false
principal = undefined
status = 200
error-code = undefined

unless policy do
  status = 404
  error-code = "route_not_found"
end

when policy and policy.auth == "session" do
  token = cookies.session
  session = when token do session-parse(token) end

  unless session do
    status = 401
    error-code = "invalid_session"
  end

  when session do
    auth-ok = true
    principal = {
      kind: "user"
      user-id: session.user-id
      roles: session.roles
      scopes: session.scopes
    }
  end
end

when policy and policy.auth == "api-key" do
  api-key = headers.authorization
  key-meta = when api-key do api-key-lookup(api-key) end

  unless key-meta do
    status = 401
    error-code = "invalid_api_key"
  end

  when key-meta do
    auth-ok = true
    principal = {
      kind: "service"
      service: key-meta.service
      scopes: key-meta.scopes
    }
  end
end

when policy and policy.auth == "signature" do
  sig = headers.x-signature
  secret = provider-signing-secret(policy.provider)

  unless verify-signature(sig, body, secret) do
    status = 401
    error-code = "bad_signature"
  end

  when verify-signature(sig, body, secret) do
    auth-ok = true
    principal = {kind: "webhook", provider: policy.provider}
  end
end

when auth-ok and policy.required-role do
  unless principal.roles and contains(principal.roles, policy.required-role) do
    status = 403
    error-code = "insufficient_role"
  end
end

when auth-ok and policy.required-scope do
  unless principal.scopes and contains(principal.scopes, policy.required-scope) do
    status = 403
    error-code = "insufficient_scope"
  end
end

result = {
  request-id: request-id
  authorized: status < 400
  principal: when status < 400 do principal end
  status: status
  error: when status >= 400 do error-code end
}

result

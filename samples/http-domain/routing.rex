/*
  samples/http-domain/routing.rex
  Realistic HTTP routing + middleware policy document in Rex.
  Designed for manual testing of syntax, navigation, symbols, and editor tooling.
*/

// ---------- environment + request context ----------
service-name = "billing-api"
environment = env or "prod"
maintenance-mode = false

request-id = req.headers.x-request-id or trace-id()
request-path = req.path
request-method = req.method
route-key = request-method + " " + request-path

// ---------- canonical route table ----------
routes = {
  "GET /health": {
    operation: "health/read"
    auth: "none"
    rate-policy: "health"
    cache-policy: "none"
    timeout-ms: 250
    middlewares: [
      "request-id"
      "trace"
      "security-headers"
      "cors"
      "metrics"
    ]
  }

  "GET /v1/users": {
    operation: "users/list"
    auth: "session"
    rate-policy: "read-default"
    cache-policy: "short"
    timeout-ms: 1500
    middlewares: [
      "request-id"
      "trace"
      "tenant"
      "auth"
      "rate-limit"
      "etag"
      "cache-read"
      "metrics"
    ]
  }

  "GET /v1/users/(id)": {
    operation: "users/read"
    auth: "session"
    rate-policy: "read-default"
    cache-policy: "short"
    timeout-ms: 1200
    middlewares: [
      "request-id"
      "trace"
      "tenant"
      "auth"
      "rate-limit"
      "etag"
      "cache-read"
      "metrics"
    ]
  }

  "POST /v1/users": {
    operation: "users/create"
    auth: "session"
    rate-policy: "write-default"
    cache-policy: "none"
    timeout-ms: 3000
    middlewares: [
      "request-id"
      "trace"
      "tenant"
      "auth"
      "csrf"
      "rate-limit"
      "body-limit"
      "json-parse"
      "validate"
      "idempotency"
      "audit"
      "metrics"
    ]
  }

  "PATCH /v1/users/(id)": {
    operation: "users/update"
    auth: "session"
    rate-policy: "write-default"
    cache-policy: "none"
    timeout-ms: 3500
    middlewares: [
      "request-id"
      "trace"
      "tenant"
      "auth"
      "csrf"
      "rate-limit"
      "body-limit"
      "json-parse"
      "validate"
      "idempotency"
      "audit"
      "metrics"
    ]
  }

  "DELETE /v1/users/(id)": {
    operation: "users/delete"
    auth: "admin"
    rate-policy: "write-default"
    cache-policy: "none"
    timeout-ms: 2500
    middlewares: [
      "request-id"
      "trace"
      "tenant"
      "auth"
      "rate-limit"
      "audit"
      "metrics"
    ]
  }

  "POST /v1/payments/charge": {
    operation: "payments/charge"
    auth: "api-key"
    rate-policy: "payment-write"
    cache-policy: "none"
    timeout-ms: 5000
    middlewares: [
      "request-id"
      "trace"
      "tenant"
      "auth"
      "signature"
      "rate-limit"
      "body-limit"
      "json-parse"
      "validate"
      "idempotency"
      "retry-budget"
      "audit"
      "metrics"
    ]
  }

  "GET /v1/payments/(id)": {
    operation: "payments/read"
    auth: "api-key"
    rate-policy: "payment-read"
    cache-policy: "short"
    timeout-ms: 2000
    middlewares: [
      "request-id"
      "trace"
      "tenant"
      "auth"
      "rate-limit"
      "etag"
      "cache-read"
      "metrics"
    ]
  }
}

matched-route = routes.(route-key)

// ---------- defaults ----------
res.status = 200
res.headers = {}
response-body = {ok: true}
reject-reason = undefined

// ---------- global middleware behaviors ----------
req.headers.x-request-id = request-id
trace("request.start", {id: request-id, method: request-method, path: request-path})

// maintenance mode short-circuit
when maintenance-mode and request-path != "/health" do
  res.status = 503
  response-body = {ok: false, error: "service_unavailable"}
  reject-reason = "maintenance"
end

// route resolution
unless matched-route do
  res.status = 404
  response-body = {ok: false, error: "route_not_found"}
  reject-reason = "route"
end

// ---------- CORS ----------
res.headers.access-control-allow-origin = cors-allow-origin(req.headers.origin)
res.headers.access-control-allow-credentials = "true"
res.headers.vary = "Origin"

when request-method == "OPTIONS" do
  res.headers.access-control-allow-methods = "GET,POST,PATCH,DELETE,OPTIONS"
  res.headers.access-control-allow-headers = "authorization,content-type,x-request-id,x-tenant,x-signature"
  res.status = 204
end

// ---------- security headers ----------
res.headers.x-content-type-options = "nosniff"
res.headers.x-frame-options = "DENY"
res.headers.referrer-policy = "no-referrer"
res.headers.content-security-policy = "default-src 'none'"

// ---------- dynamic policy lookup ----------
rate-policies = {
  health: {window-ms: 1000, limit: 50}
  "read-default": {window-ms: 60000, limit: 300}
  "write-default": {window-ms: 60000, limit: 60}
  "payment-read": {window-ms: 60000, limit: 120}
  "payment-write": {window-ms: 60000, limit: 30}
}

cache-policies = {
  none: {enabled: false}
  short: {enabled: true, ttl-ms: 5000}
  medium: {enabled: true, ttl-ms: 30000}
}

route-rate-policy = when matched-route do rate-policies.(matched-route.rate-policy) end
route-cache-policy = when matched-route do cache-policies.(matched-route.cache-policy) end

// ---------- tenant middleware ----------
tenant-id = req.headers.x-tenant or req.query.tenant or "public"
tenant-policy = tenant-config(tenant-id)

unless tenant-policy do
  res.status = 403
  response-body = {ok: false, error: "unknown_tenant"}
  reject-reason = "tenant"
end

// ---------- auth middleware ----------
auth-mode = when matched-route do matched-route.auth end
api-key = req.headers.authorization
session-token = req.cookies.session

when auth-mode == "api-key" do
  unless api-key and api-key-valid(api-key, tenant-id) do
    res.status = 401
    response-body = {ok: false, error: "invalid_api_key"}
    reject-reason = "auth"
  end
end

when auth-mode == "session" do
  unless session-token and session-valid(session-token, tenant-id) do
    res.status = 401
    response-body = {ok: false, error: "invalid_session"}
    reject-reason = "auth"
  end
end

when auth-mode == "admin" do
  unless session-token and session-has-role(session-token, "admin") do
    res.status = 403
    response-body = {ok: false, error: "admin_required"}
    reject-reason = "auth"
  end
end

// ---------- request size middleware ----------
content-length = number(req.headers.content-length) or 0
max-body-bytes = when tenant-policy do tenant-policy.max-body-bytes else 1048576 end

when content-length > max-body-bytes do
  res.status = 413
  response-body = {ok: false, error: "payload_too_large"}
  reject-reason = "body-limit"
end

// ---------- signature middleware for webhooks ----------
when matched-route and matched-route.operation == "payments/charge" do
  unless verify-signature(req.headers.x-signature, req.body, tenant-policy.signing-secret) do
    res.status = 401
    response-body = {ok: false, error: "bad_signature"}
    reject-reason = "signature"
  end
end

// ---------- rate limit middleware ----------
rate-key = tenant-id + ":" + req.ip + ":" + route-key
when route-rate-policy do
  unless rate-limit-allow(rate-key, route-rate-policy.window-ms, route-rate-policy.limit) do
    res.status = 429
    res.headers.retry-after = "60"
    response-body = {ok: false, error: "rate_limited"}
    reject-reason = "rate-limit"
  end
end

// ---------- body parse + validation middleware ----------
parsed-body = when req.headers.content-type == "application/json" do json-parse(req.body) end
validation-schema = when matched-route do schema-for-operation(matched-route.operation) end

when validation-schema and parsed-body do
  unless validate-json(parsed-body, validation-schema) do
    res.status = 422
    response-body = {ok: false, error: "validation_failed", details: validation-errors()}
    reject-reason = "validate"
  end
end

// ---------- idempotency middleware ----------
idempotency-key = req.headers.idempotency-key
when request-method == "POST" or request-method == "PATCH" do
  when idempotency-key do
    cached-write = idempotency-read(tenant-id, idempotency-key)
    when cached-write do
      res.status = cached-write.status
      response-body = cached-write.body
      res.headers.x-idempotent-replay = "true"
      reject-reason = "idempotency-replay"
    end
  end
end

// ---------- cache read middleware ----------
cache-key = tenant-id + ":" + route-key + ":" + query-cache-key(req.query)
when route-cache-policy and route-cache-policy.enabled and request-method == "GET" do
  cached-response = cache-read(cache-key)
  when cached-response do
    res.status = cached-response.status
    response-body = cached-response.body
    res.headers.x-cache = "HIT"
    reject-reason = "cache-hit"
  end
end

// ---------- upstream execution ----------
should-execute = reject-reason == undefined

when should-execute do
  timeout-ms = when matched-route do matched-route.timeout-ms else 1000 end
  upstream = execute-operation(matched-route.operation, {
    tenant-id: tenant-id,
    request-id: request-id,
    path: request-path,
    method: request-method,
    headers: req.headers,
    query: req.query,
    body: parsed-body or req.body
  }, timeout-ms)

  unless upstream do
    res.status = 502
    response-body = {ok: false, error: "upstream_unavailable"}
    reject-reason = "upstream"
  end

  when upstream do
    res.status = upstream.status or 200
    response-body = upstream.body or {ok: true}
    res.headers = merge-headers(res.headers, upstream.headers or {})
  end
end

// ---------- cache write middleware ----------
when route-cache-policy and route-cache-policy.enabled and request-method == "GET" and res.status == 200 do
  cache-write(cache-key, {status: res.status, body: response-body}, route-cache-policy.ttl-ms)
  res.headers.x-cache = "MISS"
end

// ---------- audit + trace middleware ----------
audit-event = {
  request-id: request-id
  tenant-id: tenant-id
  method: request-method
  path: request-path
  route: route-key
  status: res.status
  rejected-by: reject-reason
}

when matched-route and matched-route.operation != "health/read" do
  audit-write(audit-event)
end

trace("request.finish", {
  id: request-id,
  route: route-key,
  status: res.status,
  rejected: reject-reason,
  cache: res.headers.x-cache
})

// ---------- response envelope middleware ----------
response-body = {
  ok: res.status < 400,
  request-id: request-id,
  data: when res.status < 400 do response-body end,
  error: when res.status >= 400 do response-body.error or "unknown" end
}

// ---------- final output ----------
result = {
  status: res.status,
  headers: res.headers,
  body: response-body
}

result

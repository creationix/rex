/* Layered rate limiting and burst controls */

request-id = req.headers.x-request-id or trace-id()
route-key = req.method + " " + req.path
tenant-id = req.headers.x-tenant or "public"
subject = auth.user-id or req.ip or "anonymous"

policies = {
  global: {window-ms: 60000, limit: 2000}
  tenant: {
    public: {window-ms: 60000, limit: 300}
    enterprise: {window-ms: 60000, limit: 5000}
  }
  route: {
    "GET /v1/search": {window-ms: 1000, limit: 25, burst: 50}
    "POST /v1/payments/charge": {window-ms: 60000, limit: 20, burst: 30}
    default: {window-ms: 60000, limit: 120, burst: 180}
  }
}

route-policy = policies.route.(route-key) or policies.route.default
tenant-policy = policies.tenant.(tenant-id) or policies.tenant.public

global-key = "global:" + req.ip
tenant-key = "tenant:" + tenant-id + ":" + subject
route-key-limit = "route:" + route-key + ":" + subject

res.status = 200
reject = undefined
res.headers = {}

unless rate-limit-allow(global-key, policies.global.window-ms, policies.global.limit) do
  res.status = 429
  reject = "global"
end

when res.status == 200 do
  unless rate-limit-allow(tenant-key, tenant-policy.window-ms, tenant-policy.limit) do
    res.status = 429
    reject = "tenant"
  end
end

when res.status == 200 do
  unless rate-limit-allow(route-key-limit, route-policy.window-ms, route-policy.limit) do
    res.status = 429
    reject = "route"
  end
end

when res.status == 429 do
  res.headers.retry-after = "60"
  res.headers.x-rate-limit-reject = reject
end

when res.status == 200 and route-policy.burst do
  unless token-bucket-allow(route-key-limit + ":burst", route-policy.burst) do
    res.status = 429
    reject = "burst"
    res.headers.retry-after = "1"
    res.headers.x-rate-limit-reject = reject
  end
end

body-out = when res.status == 200 do
  {ok: true, request-id: request-id}
else
  {ok: false, error: "rate_limited", rejected-by: reject, request-id: request-id}
end

{status: res.status, headers: res.headers, body: body-out}

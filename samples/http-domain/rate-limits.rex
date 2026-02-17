/* Layered rate limiting and burst controls */

request-id = headers.x-request-id or trace-id()
route-key = method + " " + path
tenant-id = headers.x-tenant or "public"
subject = auth.user-id or ip or "anonymous"

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

global-key = "global:" + ip
tenant-key = "tenant:" + tenant-id + ":" + subject
route-key-limit = "route:" + route-key + ":" + subject

status = 200
reject = undefined
headers-out = {}

unless rate-limit-allow(global-key, policies.global.window-ms, policies.global.limit) do
  status = 429
  reject = "global"
end

when status == 200 do
  unless rate-limit-allow(tenant-key, tenant-policy.window-ms, tenant-policy.limit) do
    status = 429
    reject = "tenant"
  end
end

when status == 200 do
  unless rate-limit-allow(route-key-limit, route-policy.window-ms, route-policy.limit) do
    status = 429
    reject = "route"
  end
end

when status == 429 do
  headers-out.retry-after = "60"
  headers-out.x-rate-limit-reject = reject
end

when status == 200 and route-policy.burst do
  unless token-bucket-allow(route-key-limit + ":burst", route-policy.burst) do
    status = 429
    reject = "burst"
    headers-out.retry-after = "1"
    headers-out.x-rate-limit-reject = reject
  end
end

body-out = when status == 200 do
  {ok: true, request-id: request-id}
else
  {ok: false, error: "rate_limited", rejected-by: reject, request-id: request-id}
end

{status: status, headers: headers-out, body: body-out}

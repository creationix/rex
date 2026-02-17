/* Multi-tenant policy sample: inheritance, quotas, and routing */

tenant-id = headers.x-tenant or query.tenant or "public"
request-id = headers.x-request-id or trace-id()
route-key = method + " " + path

base-policy = {
  limits: {
    requests-per-minute: 300
    max-body-bytes: 1048576
  }
  features: {
    exports: false
    webhooks: false
    advanced-search: false
  }
  region: "us-east"
}

tenant-overrides = {
  public: {}
  dev: {
    limits: {requests-per-minute: 1000}
    features: {advanced-search: true}
  }
  acme: {
    limits: {requests-per-minute: 5000, max-body-bytes: 5242880}
    features: {exports: true, webhooks: true, advanced-search: true}
    region: "us-west"
  }
  globex: {
    limits: {requests-per-minute: 2000}
    features: {exports: true}
    region: "eu-central"
  }
}

route-feature-flags = {
  "GET /v1/exports": "exports"
  "POST /v1/webhooks": "webhooks"
  "GET /v1/search/advanced": "advanced-search"
}

override = tenant-overrides.(tenant-id)
status = 200
body-out = {ok: true}
headers-out = {x-request-id: request-id}

unless override or tenant-id == "public" do
  status = 403
  body-out = {ok: false, error: "unknown_tenant"}
end

policy = merge-deep(base-policy, override or {})

feature-name = route-feature-flags.(route-key)
when status == 200 and feature-name do
  unless policy.features.(feature-name) == true do
    status = 403
    body-out = {ok: false, error: "feature_disabled", feature: feature-name}
  end
end

when status == 200 do
  rpm-key = "tenant-rpm:" + tenant-id
  unless rate-limit-allow(rpm-key, 60000, policy.limits.requests-per-minute) do
    status = 429
    body-out = {ok: false, error: "tenant_rate_limited"}
  end
end

when status == 200 do
  max-body = policy.limits.max-body-bytes
  content-length = number(headers.content-length) or 0
  when content-length > max-body do
    status = 413
    body-out = {ok: false, error: "payload_too_large", max: max-body}
  end
end

when status == 200 do
  upstream = route-by-region(policy.region, route-key, {
    tenant-id: tenant-id,
    request-id: request-id,
    method: method,
    path: path,
    query: query,
    body: body
  })

  unless upstream do
    status = 502
    body-out = {ok: false, error: "upstream_unavailable"}
  end

  when upstream do
    status = upstream.status or 200
    body-out = upstream.body or {ok: true}
    headers-out = merge-headers(headers-out, upstream.headers or {})
    headers-out.x-tenant = tenant-id
    headers-out.x-region = policy.region
  end
end

{status: status, headers: headers-out, body: body-out}

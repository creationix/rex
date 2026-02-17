/* Multi-tenant policy sample: inheritance, quotas, and routing */

tenant-id = req.headers.x-tenant or req.query.tenant or "public"
request-id = req.headers.x-request-id or trace-id()
route-key = req.method + " " + req.path

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
res.status = 200
body-out = {ok: true}
res.headers = {x-request-id: request-id}

unless override or tenant-id == "public" do
  res.status = 403
  body-out = {ok: false, error: "unknown_tenant"}
end

policy = merge-deep(base-policy, override or {})

feature-name = route-feature-flags.(route-key)
when res.status == 200 and feature-name do
  unless policy.features.(feature-name) == true do
    res.status = 403
    body-out = {ok: false, error: "feature_disabled", feature: feature-name}
  end
end

when res.status == 200 do
  rpm-key = "tenant-rpm:" + tenant-id
  unless rate-limit-allow(rpm-key, 60000, policy.limits.requests-per-minute) do
    res.status = 429
    body-out = {ok: false, error: "tenant_rate_limited"}
  end
end

when res.status == 200 do
  max-body = policy.limits.max-body-bytes
  content-length = number(req.headers.content-length) or 0
  when content-length > max-body do
    res.status = 413
    body-out = {ok: false, error: "payload_too_large", max: max-body}
  end
end

when res.status == 200 do
  upstream = route-by-region(policy.region, route-key, {
    tenant-id: tenant-id,
    request-id: request-id,
    method: req.method,
    path: req.path,
    query: req.query,
    body: req.body
  })

  unless upstream do
    res.status = 502
    body-out = {ok: false, error: "upstream_unavailable"}
  end

  when upstream do
    res.status = upstream.status or 200
    body-out = upstream.body or {ok: true}
    res.headers = merge-headers(res.headers, upstream.headers or {})
    res.headers.x-tenant = tenant-id
    res.headers.x-region = policy.region
  end
end

{status: res.status, headers: res.headers, body: body-out}

/* Cache policy sample: read-through, write-through, etag, stale fallback */

request-id = headers.x-request-id or trace-id()
route-key = method + " " + path
tenant-id = headers.x-tenant or "public"
cache-key = tenant-id + ":" + route-key + ":" + query-cache-key(query)

policies = {
  "GET /v1/catalog": {enabled: true, ttl-ms: 30000, stale-ms: 120000}
  "GET /v1/catalog/(id)": {enabled: true, ttl-ms: 10000, stale-ms: 60000}
  "GET /v1/profile": {enabled: false}
}

policy = policies.(route-key) or {enabled: false}
status = 200
headers-out = {x-request-id: request-id}
body-out = {ok: true}
source = "upstream"

etag-in = headers.if-none-match
cached = when policy.enabled do cache-read(cache-key) end

when cached and cached.etag and etag-in == cached.etag do
  status = 304
  headers-out.etag = cached.etag
  headers-out.x-cache = "NOT_MODIFIED"
  body-out = undefined
  source = "cache-not-modified"
end

when status == 200 and cached and cached.body do
  status = cached.status or 200
  headers-out.etag = cached.etag
  headers-out.x-cache = "HIT"
  body-out = cached.body
  source = "cache-hit"
end

when source == "upstream" do
  upstream = fetch-resource(route-key, {
    request-id: request-id,
    tenant-id: tenant-id,
    query: query,
    headers: headers
  })

  unless upstream do
    status = 502
    body-out = {ok: false, error: "upstream_unavailable"}
  end

  when upstream do
    status = upstream.status or 200
    body-out = upstream.body or {ok: true}
    headers-out = merge-headers(headers-out, upstream.headers or {})

    generated-etag = etag-of(body-out)
    headers-out.etag = generated-etag

    when policy.enabled and status == 200 do
      cache-write(cache-key, {
        status: status,
        body: body-out,
        etag: generated-etag,
        written-at: now-ms()
      }, policy.ttl-ms)
      headers-out.x-cache = "MISS"
    end
  end
end

// stale-if-error fallback
when status >= 500 and cached and cached.body and policy.stale-ms do
  age-ms = now-ms() - (cached.written-at or 0)
  when age-ms < policy.stale-ms do
    status = cached.status or 200
    body-out = cached.body
    headers-out.x-cache = "STALE"
    headers-out.warning = "110 - Response is stale"
  end
end

{status: status, headers: headers-out, body: body-out}

/* Webhook ingestion sample: signature, replay protection, retry strategy */

request-id = req.headers.x-request-id or trace-id()
provider = req.headers.x-provider or "unknown"
event-id = req.headers.x-event-id
sig = req.headers.x-signature
timestamp = req.headers.x-signature-ts

res.status = 202
res.headers = {x-request-id: request-id}
body-out = {ok: true}
reject = undefined

secret = provider-signing-secret(provider)

unless provider and secret do
  res.status = 400
  body-out = {ok: false, error: "unknown_provider"}
  reject = "provider"
end

when res.status < 400 do
  unless event-id and sig and timestamp do
    res.status = 400
    body-out = {ok: false, error: "missing_signature_headers"}
    reject = "headers"
  end
end

when res.status < 400 do
  skew-ms = abs(now-ms() - number(timestamp))
  when skew-ms > 300000 do
    res.status = 401
    body-out = {ok: false, error: "signature_expired"}
    reject = "timestamp"
  end
end

when res.status < 400 do
  unless verify-signature(sig, timestamp + "." + req.body, secret) do
    res.status = 401
    body-out = {ok: false, error: "bad_signature"}
    reject = "signature"
  end
end

when res.status < 400 do
  when replay-seen(provider + ":" + event-id) do
    res.status = 200
    body-out = {ok: true, duplicate: true}
    reject = "replay"
  end
end

when res.status < 400 and reject == undefined do
  replay-write(provider + ":" + event-id, 86400)

  parsed = json-parse(req.body)
  route = webhook-route(provider, parsed.type)

  unless route do
    res.status = 202
    body-out = {ok: true, ignored: true}
  end

  when route do
    attempts = 0
    delivered = false

    for [1, 2, 3] do
      attempts += 1
      delivery = deliver-event(route, parsed, {
        request-id: request-id,
        provider: provider,
        event-id: event-id,
        attempt: attempts
      })

      when delivery and delivery.ok do
        delivered = true
        break
      end

      when attempts < 3 do
        backoff-ms(100 * attempts * attempts)
      end
    end

    unless delivered do
      res.status = 502
      body-out = {ok: false, error: "delivery_failed", attempts: attempts}
    end
  end
end

trace("webhook.finish", {
  id: request-id,
  provider: provider,
  event: event-id,
  status: res.status,
  reject: reject
})

{status: res.status, headers: res.headers, body: body-out}

/* Event ingestion + streaming response policy sample */

request-id = req.headers.x-request-id or trace-id()
route-key = req.method + " " + req.path
res.status = 200
res.headers = {
  x-request-id: request-id
  content-type: "application/json"
}
body-out = {ok: true}

routes = {
  "POST /v1/events/ingest": {mode: "ingest"}
  "GET /v1/events/stream": {mode: "stream"}
}

route = routes.(route-key)
unless route do
  res.status = 404
  body-out = {ok: false, error: "route_not_found"}
end

when res.status == 200 and route.mode == "ingest" do
  parsed = json-parse(req.body)

  when array(parsed.events) do
    accepted = 0
    dropped = 0

    for event in parsed.events do
      normalized = {
        id: event.id or event-id()
        ts: event.ts or now-ms()
        type: event.type
        source: event.source or "unknown"
        payload: event.payload or {}
      }

      when validate-event(normalized) do
        enqueue-event(normalized)
        accepted += 1
      else
        dropped += 1
      end
    end

    body-out = {
      ok: true,
      accepted: accepted,
      dropped: dropped,
      request-id: request-id
    }
    res.status = 202
  else
    res.status = 422
    body-out = {ok: false, error: "invalid_events_payload"}
  end
end

when res.status == 200 and route.mode == "stream" do
  cursor = req.query.cursor or "0"
  batch-size = number(req.query.limit) or 100
  stream = stream-read(cursor, batch-size)

  when stream do
    res.headers.content-type = "application/x-ndjson"
    res.headers.cache-control = "no-store"
    res.headers.x-stream-next-cursor = stream.next-cursor or cursor

    // represent stream result as structured object for policy layer
    body-out = {
      ok: true,
      mode: "stream",
      cursor: cursor,
      next-cursor: stream.next-cursor,
      events: stream.events or []
    }
  else
    res.status = 503
    body-out = {ok: false, error: "stream_unavailable"}
  end
end

trace("events.request", {
  id: request-id,
  route: route-key,
  status: res.status
})

{status: res.status, headers: res.headers, body: body-out}

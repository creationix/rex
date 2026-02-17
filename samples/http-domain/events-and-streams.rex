/* Event ingestion + streaming response policy sample */

request-id = headers.x-request-id or trace-id()
route-key = method + " " + path
status = 200
headers-out = {
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
  status = 404
  body-out = {ok: false, error: "route_not_found"}
end

when status == 200 and route.mode == "ingest" do
  parsed = json-parse(body)

  unless parsed and array(parsed.events) do
    status = 422
    body-out = {ok: false, error: "invalid_events_payload"}
  end

  when parsed and array(parsed.events) do
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
    status = 202
  end
end

when status == 200 and route.mode == "stream" do
  cursor = query.cursor or "0"
  batch-size = number(query.limit) or 100
  stream = stream-read(cursor, batch-size)

  unless stream do
    status = 503
    body-out = {ok: false, error: "stream_unavailable"}
  end

  when stream do
    headers-out.content-type = "application/x-ndjson"
    headers-out.cache-control = "no-store"
    headers-out.x-stream-next-cursor = stream.next-cursor or cursor

    // represent stream result as structured object for policy layer
    body-out = {
      ok: true,
      mode: "stream",
      cursor: cursor,
      next-cursor: stream.next-cursor,
      events: stream.events or []
    }
  end
end

trace("events.request", {
  id: request-id,
  route: route-key,
  status: status
})

{status: status, headers: headers-out, body: body-out}

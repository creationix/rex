{
  cases: [
    {
      name: "ingest with empty body"
      input: {
        refs: {
          H: {}
          M: "POST"
          P: "/v1/events/ingest"
          Q: {}
          C: {}
          B: "{}"
        }
      }
      expect: {
        value: {
          status: 422
          headers: {
            content-type: "application/json"
          }
          body: {
            ok: false
            error: "invalid_events_payload"
          }
        }
      }
    }
    {
      name: "stream route"
      input: {
        refs: {
          H: {}
          M: "GET"
          P: "/v1/events/stream"
          Q: {}
          C: {}
          B: undefined
        }
      }
      expect: {
        value: {
          status: 503
          headers: {
            content-type: "application/json"
          }
          body: {
            ok: false
            error: "stream_unavailable"
          }
        }
      }
    }
    {
      name: "unknown route"
      input: {
        refs: {
          H: {}
          M: "GET"
          P: "/v1/events/other"
          Q: {}
          C: {}
          B: undefined
        }
      }
      expect: {
        value: {
          status: 404
          headers: {
            content-type: "application/json"
          }
          body: {
            ok: false
            error: "route_not_found"
          }
        }
      }
    }
  ]
}

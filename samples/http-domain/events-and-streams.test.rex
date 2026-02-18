{
  cases: [
    {
      name: "ingest with empty body"
      input: {
        refs: {
          "43": {}
          "48": "POST"
          "51": "/v1/events/ingest"
          "52": {}
          "38": {}
          "37": "{}"
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
          "43": {}
          "48": "GET"
          "51": "/v1/events/stream"
          "52": {}
          "38": {}
          "37": undefined
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
          "43": {}
          "48": "GET"
          "51": "/v1/events/other"
          "52": {}
          "38": {}
          "37": undefined
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

{
  cases: [
    {
      name: "health route upstream unavailable"
      input: {
        refs: {
          "43": {}
          "48": "GET"
          "51": "/health"
          "52": {}
          "38": {}
          "37": undefined
        }
      }
      expect: {
        value: {
          status: 502
          headers: {}
          body: {
            ok: false
            error: "upstream_unavailable"
          }
        }
      }
    }
    {
      name: "users route unauthorized"
      input: {
        refs: {
          "43": {}
          "48": "GET"
          "51": "/v1/users"
          "52": {}
          "38": {}
          "37": undefined
        }
      }
      expect: {
        value: {
          status: 401
          headers: {}
          body: {
            ok: false
            error: "unauthorized"
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
          "51": "/unknown"
          "52": {}
          "38": {}
          "37": undefined
        }
      }
      expect: {
        value: {
          status: 404
          headers: {}
          body: {
            ok: false
            error: "route_not_found"
          }
        }
      }
    }
  ]
}

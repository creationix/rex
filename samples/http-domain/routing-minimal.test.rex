{
  cases: [
    {
      name: "health route upstream unavailable"
      input: {
        refs: {
          H: {}
          M: "GET"
          P: "/health"
          Q: {}
          C: {}
          B: undefined
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
          H: {}
          M: "GET"
          P: "/v1/users"
          Q: {}
          C: {}
          B: undefined
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
          H: {}
          M: "GET"
          P: "/unknown"
          Q: {}
          C: {}
          B: undefined
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

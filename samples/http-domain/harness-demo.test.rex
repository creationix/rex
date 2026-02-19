{
  cases: [
    {
      name: "health route"
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
          status: 200
          route: "GET /health"
          operation: "health/read"
          tenant: "public"
        }
      }
    }
    {
      name: "users route with tenant header"
      input: {
        refs: {
          H: {x-tenant: "acme"}
          M: "GET"
          P: "/v1/users"
          Q: {}
          C: {}
          B: undefined
        }
      }
      expect: {
        value: {
          status: 200
          route: "GET /v1/users"
          operation: "users/list"
          tenant: "acme"
        }
      }
    }
    {
      name: "unknown route"
      input: {
        refs: {
          H: {}
          M: "PATCH"
          P: "/missing"
          Q: {}
          C: {}
          B: undefined
        }
      }
      expect: {
        value: {
          status: 404
          route: "PATCH /missing"
          error: "route_not_found"
          tenant: "public"
        }
      }
    }
  ]
}

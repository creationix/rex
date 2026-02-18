{
  cases: [
    {
      name: "health route"
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
          "43": {x-tenant: "acme"}
          "48": "GET"
          "51": "/v1/users"
          "52": {}
          "38": {}
          "37": undefined
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
          "43": {}
          "48": "PATCH"
          "51": "/missing"
          "52": {}
          "38": {}
          "37": undefined
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

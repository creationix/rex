{
  cases: [
    {
      name: "public tenant health"
      input: {
        refs: {
          "43": {}
          "48": "GET"
          "51": "/v1/health"
          "52": {}
          "38": {}
          "37": undefined
        }
      }
      expect: {
        value: {
          status: 429
          headers: {}
          body: {
            ok: false
            error: "tenant_rate_limited"
          }
        }
      }
    }
    {
      name: "acme exports feature check"
      input: {
        refs: {
          "43": {x-tenant: "acme"}
          "48": "GET"
          "51": "/v1/exports"
          "52": {}
          "38": {}
          "37": undefined
        }
      }
      expect: {
        value: {
          status: 403
          headers: {}
          body: {
            ok: false
            error: "feature_disabled"
            feature: "exports"
          }
        }
      }
    }
    {
      name: "unknown tenant"
      input: {
        refs: {
          "43": {x-tenant: "nope"}
          "48": "GET"
          "51": "/v1/health"
          "52": {}
          "38": {}
          "37": undefined
        }
      }
      expect: {
        value: {
          status: 403
          headers: {}
          body: {
            ok: false
            error: "unknown_tenant"
          }
        }
      }
    }
  ]
}

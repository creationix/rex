{
  cases: [
    {
      name: "public tenant health"
      input: {
        refs: {
          H: {}
          M: "GET"
          P: "/v1/health"
          Q: {}
          C: {}
          B: undefined
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
          H: {x-tenant: "acme"}
          M: "GET"
          P: "/v1/exports"
          Q: {}
          C: {}
          B: undefined
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
          H: {x-tenant: "nope"}
          M: "GET"
          P: "/v1/health"
          Q: {}
          C: {}
          B: undefined
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

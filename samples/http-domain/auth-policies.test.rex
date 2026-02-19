{
  cases: [
    {
      name: "session route without cookie"
      input: {
        refs: {
          H: {}
          M: "GET"
          P: "/v1/me"
          Q: {}
          C: {}
          B: undefined
        }
      }
      expect: {
        value: {
          status: 403
          error: "insufficient_role"
        }
      }
    }
    {
      name: "api-key route without auth"
      input: {
        refs: {
          H: {}
          M: "POST"
          P: "/v1/internal/reindex"
          Q: {}
          C: {}
          B: undefined
        }
      }
      expect: {
        value: {
          status: 403
          error: "insufficient_scope"
        }
      }
    }
    {
      name: "signature route without signature"
      input: {
        refs: {
          H: {}
          M: "POST"
          P: "/v1/webhooks/provider"
          Q: {}
          C: {}
          B: undefined
        }
      }
      expect: {
        value: {
          status: 401
          error: "bad_signature"
        }
      }
    }
  ]
}

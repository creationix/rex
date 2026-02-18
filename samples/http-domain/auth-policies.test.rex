{
  cases: [
    {
      name: "session route without cookie"
      input: {
        refs: {
          "43": {}
          "48": "GET"
          "51": "/v1/me"
          "52": {}
          "38": {}
          "37": undefined
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
          "43": {}
          "48": "POST"
          "51": "/v1/internal/reindex"
          "52": {}
          "38": {}
          "37": undefined
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
          "43": {}
          "48": "POST"
          "51": "/v1/webhooks/provider"
          "52": {}
          "38": {}
          "37": undefined
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

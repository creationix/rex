{
  cases: [
    {
      name: "unknown route with unknown tenant"
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
          status: 403
          headers: {
            access-control-allow-credentials: "true"
            vary: "Origin"
            x-content-type-options: "nosniff"
            x-frame-options: "DENY"
            referrer-policy: "no-referrer"
            content-security-policy: "default-src 'none'"
          }
          body: {
            error: "unknown_tenant"
          }
        }
      }
    }
    {
      name: "health route rate limited"
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
          status: 429
          headers: {
            access-control-allow-credentials: "true"
            vary: "Origin"
            x-content-type-options: "nosniff"
            x-frame-options: "DENY"
            referrer-policy: "no-referrer"
            content-security-policy: "default-src 'none'"
            retry-after: "60"
          }
          body: {
            error: "rate_limited"
          }
        }
      }
    }
    {
      name: "users route rate limited"
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
          status: 429
          headers: {
            access-control-allow-credentials: "true"
            vary: "Origin"
            x-content-type-options: "nosniff"
            x-frame-options: "DENY"
            referrer-policy: "no-referrer"
            content-security-policy: "default-src 'none'"
            retry-after: "60"
          }
          body: {
            error: "rate_limited"
          }
        }
      }
    }
  ]
}

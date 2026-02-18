{
  cases: [
    {
      name: "search route global limit"
      input: {
        refs: {
          "43": {}
          "48": "GET"
          "51": "/v1/search"
          "52": {}
          "38": {}
          "37": undefined
        }
      }
      expect: {
        value: {
          status: 429
          headers: {
            retry-after: "60"
            x-rate-limit-reject: "global"
          }
          body: {
            ok: false
            error: "rate_limited"
            rejected-by: "global"
          }
        }
      }
    }
    {
      name: "payments route global limit"
      input: {
        refs: {
          "43": {}
          "48": "POST"
          "51": "/v1/payments/charge"
          "52": {}
          "38": {}
          "37": undefined
        }
      }
      expect: {
        value: {
          status: 429
          headers: {
            retry-after: "60"
            x-rate-limit-reject: "global"
          }
          body: {
            ok: false
            error: "rate_limited"
            rejected-by: "global"
          }
        }
      }
    }
    {
      name: "other route global limit"
      input: {
        refs: {
          "43": {}
          "48": "GET"
          "51": "/v1/other"
          "52": {}
          "38": {}
          "37": undefined
        }
      }
      expect: {
        value: {
          status: 429
          headers: {
            retry-after: "60"
            x-rate-limit-reject: "global"
          }
          body: {
            ok: false
            error: "rate_limited"
            rejected-by: "global"
          }
        }
      }
    }
  ]
}

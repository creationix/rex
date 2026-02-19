{
  cases: [
    {
      name: "search route global limit"
      input: {
        refs: {
          H: {}
          M: "GET"
          P: "/v1/search"
          Q: {}
          C: {}
          B: undefined
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
          H: {}
          M: "POST"
          P: "/v1/payments/charge"
          Q: {}
          C: {}
          B: undefined
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
          H: {}
          M: "GET"
          P: "/v1/other"
          Q: {}
          C: {}
          B: undefined
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

{
  cases: [
    {
      name: "unknown operation"
      input: {
        refs: {
          H: {}
          M: "GET"
          P: "/x"
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
            code: "UPSTREAM_UNAVAILABLE"
          }
        }
      }
    }
    {
      name: "users operation header"
      input: {
        refs: {
          H: {x-operation: "users/list"}
          M: "GET"
          P: "/x"
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
            code: "UPSTREAM_UNAVAILABLE"
          }
        }
      }
    }
    {
      name: "payments operation header"
      input: {
        refs: {
          H: {x-operation: "payments/charge"}
          M: "POST"
          P: "/x"
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
            code: "UPSTREAM_UNAVAILABLE"
          }
        }
      }
    }
  ]
}

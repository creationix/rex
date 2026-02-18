{
  cases: [
    {
      name: "unknown operation"
      input: {
        refs: {
          "43": {}
          "48": "GET"
          "51": "/x"
          "52": {}
          "38": {}
          "37": undefined
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
          "43": {x-operation: "users/list"}
          "48": "GET"
          "51": "/x"
          "52": {}
          "38": {}
          "37": undefined
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
          "43": {x-operation: "payments/charge"}
          "48": "POST"
          "51": "/x"
          "52": {}
          "38": {}
          "37": undefined
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

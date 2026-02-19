{
  cases: [
    {
      name: "catalog upstream miss"
      input: {
        refs: {
          H: {}
          M: "GET"
          P: "/v1/catalog"
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
          }
        }
      }
    }
    {
      name: "catalog with if-none-match"
      input: {
        refs: {
          H: {if-none-match: "etag123"}
          M: "GET"
          P: "/v1/catalog"
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
          }
        }
      }
    }
    {
      name: "profile policy path"
      input: {
        refs: {
          H: {}
          M: "GET"
          P: "/v1/profile"
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
          }
        }
      }
    }
  ]
}

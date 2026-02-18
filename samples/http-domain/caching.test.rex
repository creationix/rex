{
  cases: [
    {
      name: "catalog upstream miss"
      input: {
        refs: {
          "43": {}
          "48": "GET"
          "51": "/v1/catalog"
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
          }
        }
      }
    }
    {
      name: "catalog with if-none-match"
      input: {
        refs: {
          "43": {if-none-match: "etag123"}
          "48": "GET"
          "51": "/v1/catalog"
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
          }
        }
      }
    }
    {
      name: "profile policy path"
      input: {
        refs: {
          "43": {}
          "48": "GET"
          "51": "/v1/profile"
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
          }
        }
      }
    }
  ]
}

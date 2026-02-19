{
  defaults: {
    refs: {
      H: {}
      D: "localhost"
      M: "GET"
      P: "/"
      Q: {}
      C: {}
      B: ""
    }
  }
  cases: [
    {
      name: "provider missing"
      input: {
        refs: {
          H: {}
          M: "POST"
          P: "/v1/webhooks/provider"
          Q: {}
          C: {}
          B: "{}"
        }
      }
      expect: {
        value: {
          status: 400
          headers: {}
          body: {
            ok: false
            error: "unknown_provider"
          }
        }
      }
    }
    {
      name: "provider only"
      input: {
        refs: {
          H: {x-provider: "acme"}
          M: "POST"
          P: "/v1/webhooks/provider"
          Q: {}
          C: {}
          B: "{}"
        }
      }
      expect: {
        value: {
          status: 400
          headers: {}
          body: {
            ok: false
            error: "unknown_provider"
          }
        }
      }
    }
    {
      name: "full signature headers"
      input: {
        refs: {
          H: {
            x-provider: "acme"
            x-event-id: "e1"
            x-signature: "sig"
            x-signature-ts: "1"
          }
          M: "POST"
          P: "/v1/webhooks/provider"
          Q: {}
          C: {}
          B: "{}"
        }
      }
      expect: {
        value: {
          status: 400
          headers: {}
          body: {
            ok: false
            error: "unknown_provider"
          }
        }
      }
    }
  ]
}

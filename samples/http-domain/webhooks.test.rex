{
  cases: [
    {
      name: "provider missing"
      input: {
        refs: {
          "43": {}
          "48": "POST"
          "51": "/v1/webhooks/provider"
          "52": {}
          "38": {}
          "37": "{}"
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
          "43": {x-provider: "acme"}
          "48": "POST"
          "51": "/v1/webhooks/provider"
          "52": {}
          "38": {}
          "37": "{}"
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
          "43": {
            x-provider: "acme"
            x-event-id: "e1"
            x-signature: "sig"
            x-signature-ts: "1"
          }
          "48": "POST"
          "51": "/v1/webhooks/provider"
          "52": {}
          "38": {}
          "37": "{}"
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

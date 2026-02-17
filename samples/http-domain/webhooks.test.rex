{
  cases: [
    {
      name: "smoke: webhooks executes with minimal headers"
      input: {
        vars: {
          method: "POST"
          path: "/v1/webhooks/provider"
          headers: {}
          body: "{}"
        }
      }
    }
  ]
}

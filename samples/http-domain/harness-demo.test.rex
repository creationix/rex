{
  cases: [
    {
      name: "smoke: harness-demo executes known route"
      input: {
        vars: {
          method: "GET"
          path: "/health"
          headers: {}
        }
      }
    }
  ]
}

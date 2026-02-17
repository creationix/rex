{
  cases: [
    {
      name: "smoke: multi-tenant executes for public tenant"
      input: {
        vars: {
          method: "GET"
          path: "/v1/health"
          headers: {}
          query: {}
          body: undefined
        }
      }
    }
  ]
}

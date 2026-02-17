{
  cases: [
    {
      name: "smoke: routing-minimal executes for health route"
      input: {
        vars: {
          method: "GET"
          path: "/health"
          headers: {}
          cookies: {}
          query: {}
          body: undefined
        }
      }
    }
  ]
}

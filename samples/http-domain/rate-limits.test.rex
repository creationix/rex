{
  cases: [
    {
      name: "smoke: rate-limits executes for search route"
      input: {
        vars: {
          method: "GET"
          path: "/v1/search"
          headers: {}
          auth: {}
          ip: "127.0.0.1"
        }
      }
    }
  ]
}

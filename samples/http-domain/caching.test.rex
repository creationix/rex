{
  cases: [
    {
      name: "smoke: caching executes for catalog route"
      input: {
        vars: {
          method: "GET"
          path: "/v1/catalog"
          headers: {}
          query: {}
        }
      }
    }
  ]
}

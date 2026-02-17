{
  cases: [
    {
      name: "smoke: routing executes with unknown route"
      input: {
        vars: {
          method: "GET"
          path: "/unknown"
          headers: {}
          cookies: {}
          query: {}
          body: undefined
        }
      }
    }
  ]
}

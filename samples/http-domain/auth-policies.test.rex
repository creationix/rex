{
  cases: [
    {
      name: "smoke: auth-policies executes for known session route"
      input: {
        vars: {
          method: "GET"
          path: "/v1/me"
          headers: {}
          cookies: {}
          body: "{}"
        }
      }
    }
  ]
}

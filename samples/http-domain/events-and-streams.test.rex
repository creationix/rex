{
  cases: [
    {
      name: "smoke: events-and-streams executes ingest path"
      input: {
        vars: {
          method: "POST"
          path: "/v1/events/ingest"
          headers: {}
          query: {}
          body: "{}"
        }
      }
    }
  ]
}

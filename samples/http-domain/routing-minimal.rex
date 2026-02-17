/* Minimal routing + middleware sample */

request-id = req.headers.x-request-id or trace-id()
route-key = req.method + " " + req.path
default-timeout-ms = edge-config.routing.default-operation-timeout-ms or 2000
response-tag = edge-config.routing.response-tag

routes = {
  "GET /health": {op: "health", auth: "none"}
  "GET /v1/users": {op: "users/list", auth: "session"}
  "POST /v1/users": {op: "users/create", auth: "session"}
}

route = routes.(route-key)
res.status = 200
res.headers = {x-request-id: request-id}
body-out = {ok: true}

when response-tag do
  res.headers.x-response-tag = response-tag
end

unless route do
  res.status = 404
  body-out = {ok: false, error: "route_not_found"}
end

when route and route.auth == "session" do
  unless req.cookies.session and session-valid(req.cookies.session) do
    res.status = 401
    body-out = {ok: false, error: "unauthorized"}
  end
end

when res.status == 200 and route do
  op-result = execute-operation(route.op, {
    request-id: request-id,
    method: req.method,
    path: req.path,
    query: req.query,
    body: req.body,
    timeout-ms: route.timeout-ms or default-timeout-ms
  })

  unless op-result do
    res.status = 502
    body-out = {ok: false, error: "upstream_unavailable"}
  end

  when op-result do
    res.status = op-result.status or 200
    body-out = op-result.body or {ok: true}
  end
end

{status: res.status, headers: res.headers, body: body-out}

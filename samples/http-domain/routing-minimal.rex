/* Minimal routing + middleware sample */

request-id = headers.x-request-id or trace-id()
route-key = method + " " + path
default-timeout-ms = edge-config.routing.default-operation-timeout-ms or 2000
response-tag = edge-config.routing.response-tag

routes = {
  "GET /health": {op: "health", auth: "none"}
  "GET /v1/users": {op: "users/list", auth: "session"}
  "POST /v1/users": {op: "users/create", auth: "session"}
}

route = routes.(route-key)
status = 200
body-out = {ok: true}

headers.x-request-id = request-id
when response-tag do
  headers.x-response-tag = response-tag
end

unless route do
  status = 404
  body-out = {ok: false, error: "route_not_found"}
end

when route and route.auth == "session" do
  unless cookies.session and session-valid(cookies.session) do
    status = 401
    body-out = {ok: false, error: "unauthorized"}
  end
end

when status == 200 and route do
  op-result = execute-operation(route.op, {
    request-id: request-id,
    method: method,
    path: path,
    query: query,
    body: body,
    timeout-ms: route.timeout-ms or default-timeout-ms
  })

  unless op-result do
    status = 502
    body-out = {ok: false, error: "upstream_unavailable"}
  end

  when op-result do
    status = op-result.status or 200
    body-out = op-result.body or {ok: true}
  end
end

{status: status, headers: {x-request-id: request-id}, body: body-out}

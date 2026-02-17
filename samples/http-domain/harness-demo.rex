/* Deterministic program for sample harness vectors */

route-key = method + " " + path
routes = {
  "GET /health": {operation: "health/read"}
  "GET /v1/users": {operation: "users/list"}
  "POST /v1/users": {operation: "users/create"}
}

matched = routes.(route-key)
status = 200
error = undefined
operation = matched.operation
tenant = headers.x-tenant or "public"

unless matched do
  status = 404
  error = "route_not_found"
end

result = {
  status: status
  route: route-key
  operation: operation
  error: error
  tenant: tenant
}

result

/* Deterministic program for sample harness vectors */

route-key = req.method + " " + req.path
routes = {
  "GET /health": {operation: "health/read"}
  "GET /v1/users": {operation: "users/list"}
  "POST /v1/users": {operation: "users/create"}
}

matched = routes.(route-key)
res.status = 200
error = undefined
operation = matched.operation
tenant = req.headers.x-tenant or "public"

unless matched do
  res.status = 404
  error = "route_not_found"
end

result = {
  status: res.status
  route: route-key
  operation: operation
  error: error
  tenant: tenant
}

result

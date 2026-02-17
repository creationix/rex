/* Error normalization sample for HTTP gateways */

request-id = headers.x-request-id or trace-id()
operation = headers.x-operation or "unknown"

status = 200
headers-out = {x-request-id: request-id}
body-out = {ok: true}

raw = execute-operation(operation, {
  request-id: request-id,
  method: method,
  path: path,
  query: query,
  body: body
})

unless raw do
  status = 502
  body-out = {
    ok: false,
    error: "upstream_unavailable",
    code: "UPSTREAM_UNAVAILABLE"
  }
end

when raw do
  upstream-status = raw.status or 500
  upstream-error = raw.error

  when upstream-status < 400 do
    status = upstream-status
    body-out = raw.body or {ok: true}
    headers-out = merge-headers(headers-out, raw.headers or {})
  end

  when upstream-status >= 400 do
    status = upstream-status

    // map transient/network class
    when upstream-error and starts-with(upstream-error.code, "ECONN") do
      status = 503
      body-out = {
        ok: false,
        error: "service_unavailable",
        code: "UPSTREAM_NETWORK",
        retryable: true
      }
    end

    // map validation class
    when upstream-status == 400 or upstream-status == 422 do
      body-out = {
        ok: false,
        error: "invalid_request",
        code: "VALIDATION_FAILED",
        details: raw.details or validation-errors()
      }
    end

    // map auth class
    when upstream-status == 401 do
      body-out = {
        ok: false,
        error: "unauthorized",
        code: "UNAUTHORIZED"
      }
    end

    when upstream-status == 403 do
      body-out = {
        ok: false,
        error: "forbidden",
        code: "FORBIDDEN"
      }
    end

    // map not found
    when upstream-status == 404 do
      body-out = {
        ok: false,
        error: "not_found",
        code: "NOT_FOUND"
      }
    end

    // map conflict
    when upstream-status == 409 do
      body-out = {
        ok: false,
        error: "conflict",
        code: "CONFLICT"
      }
    end

    // map fallback 5xx
    when upstream-status >= 500 and body-out.ok != false do
      status = 502
      body-out = {
        ok: false,
        error: "upstream_failure",
        code: "UPSTREAM_FAILURE",
        retryable: true
      }
    end

    headers-out.x-error-code = body-out.code or "UNKNOWN"
  end
end

trace("gateway.error-map", {
  id: request-id,
  operation: operation,
  status: status,
  code: body-out.code
})

{status: status, headers: headers-out, body: body-out}

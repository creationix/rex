// Middleware provides:
extern user: some | none

// API middleware — require valid auth token, rate limit

unless user do
  status = 401
  return {ok: false, error: "authentication_required"}
end

// Rate limit: 1000 requests per 60s window
rate-key = `rate:${user.id}`
count = kv.incr(rate-key)
when count > 1000 do
  status = 429
  return {ok: false, error: "rate_limit_exceeded"}
end

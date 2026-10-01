/* API middleware: key-based authentication.
   Protects all /api/* routes. A valid key must exist in the database.
   Seed one with: sqlite3 examples/knowledge-base/data.db "INSERT INTO kv VALUES('keys:demo','1')" */

api-key = headers.authorization

unless api-key do
  res.status = 401
  return { ok: false error: "missing_api_key" hint: "Add Authorization header. Seed a key: sqlite3 data.db \"INSERT INTO kv VALUES('keys:demo','1')\"" }
end

key-valid = db.get("keys:" + api-key)

unless key-valid do
  key-valid
  res.status = 401
  return { ok: false error: "invalid_api_key" }
end
key-valid

/* Authenticated identity — available to downstream handlers */
log.info(`authenticated: ${api-key}`)

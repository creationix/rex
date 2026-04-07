// POST: create account
unless method == "POST" do
  status = 405
  return {ok: false, error: "method_not_allowed"}
end

input = json.parse(body)
unless input and input.username and input.password and input.email do
  status = 422
  return {ok: false, error: "missing_fields"}
end

// Check username uniqueness
when db.get(`username:${input.username}`) do
  status = 409
  return {ok: false, error: "username_taken"}
end

// Check email uniqueness
when db.get(`email:${input.email}`) do
  status = 409
  return {ok: false, error: "email_taken"}
end

// Hash password
salt = crypto.random(16)
password-hash = crypto.hash("sha256", `${input.password}${salt}`)

id = time.uuid()
now = time.now()

// Check if first user — make them admin
users = db.list("user:")
role = "user"
when users.size == 0 do
  role = "admin"
end

user-record = json.stringify({
  id: id
  username: input.username
  email: input.email
  password-hash: password-hash
  salt: salt
  role: role
  created: now
})

db.set(`user:${id}`, user-record)
db.set(`username:${input.username}`, json.stringify({id: id}))
db.set(`email:${input.email}`, json.stringify({id: id}))

// Create session
token = crypto.random(32)
db.set(`session:${token}`, json.stringify({
  user-id: id
  created: now
  expires: now + 86400000
}))

status = 201
{ok: true, token: token, user-id: id}

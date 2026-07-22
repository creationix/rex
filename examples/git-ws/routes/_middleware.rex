// Global middleware — parse auth token if present

res.headers.x-content-type-options = "nosniff"

token = none
when auth = headers.authorization do
  // Strip "Bearer " prefix
  when auth.size > 7 and auth.slice(0, 7) == "Bearer " do
    token = auth.slice(7, auth.size)
  end
end

user = none
when token do
  // Try session token first, then API token
  when session = db.get(`session:${token}`) do
    session = json.parse(session)
    when expires = session.expires do
      when expires > time.now() do
        when record = db.get(`user:${session.user-id}`) do
          user = json.parse(record)
        end
      end
    end
  end
  unless user do
    when api-token = db.get(`api-token:${token}`) do
      api-token = json.parse(api-token)
      when record = db.get(`user:${api-token.user-id}`) do
        user = json.parse(record)
      end
    end
  end
end

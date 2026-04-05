// These can be overridden by seeding the interpreter state
extern app: str | none
extern port: int | none
extern host: str | none
app: str = app or "myapp"
port: int = port or 8080
host: str = host or "0.0.0.0"
{
  name:app
  listen:`${host}:${port}`
  database:{
    url:`postgres://localhost:5432/${app}`
    pool-size:10
    timeout:30
  }
  cache:{
    url:`redis://localhost:6379/0`
    ttl:300
  }
  cors:{
    origins:[ `http://localhost:${port}` "https://myapp.com" ]
    methods:[ "GET" "POST" "PUT" "DELETE" ]
  }
}

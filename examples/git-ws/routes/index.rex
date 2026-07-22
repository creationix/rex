// Landing page
res.headers.content-type = "text/html"
html`
  <h1>Git WebSocket Server</h1>
  <p>A git hosting server using the WebSocket object sync protocol.</p>
  <nav>
    <a href="/auth/login">Login</a>
    <a href="/auth/signup">Sign Up</a>
  </nav>
`

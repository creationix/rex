/* Global middleware: security headers, request ID, and view-source tool.
   This runs for EVERY request — static files, API, and pages alike. */

request-id = headers.x-request-id or time.uuid()
res.headers.x-request-id = request-id
res.headers.x-content-type-options = "nosniff"
res.headers.x-powered-by = "rex-serve"

/* View-source tool: send X-View-Source header to see the .rex source
   for the current route. This is opt-in per project (via this middleware). */
when headers.x-view-source do
   /* Map the URL path back to a source file */source-path = "routes" + path + ".rex"
  source = fs.read(source-path)

  /* Try index.rex for directory paths */
  unless source do
    source-path = "routes" + path + "/index.rex"
    source = fs.read(source-path)
  end

  /* Try [slug].rex pattern — read the directory for any bracket file */
  when source do
    res.status = 418
    res.headers.content-type = "text/plain; charset=utf-8"
    res.headers.x-source-path = source-path
    "// Source: " + source-path + "

" + source
  end
end

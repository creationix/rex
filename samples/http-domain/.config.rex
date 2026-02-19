{
  data: {
    // HTTP Request Information
    H: {
      names: ['req.headers' 'headers']
      desc: 'Request headers exposed as a case-insensitive multi-value map'
      type: '(string[]|(string[]|string){})'
      default: {}
    }
    M: {
      names: ['req.method' 'method']
      desc: 'Request method (GET POST ... )'
      type: 'string'
      default: 'GET'
    }
    D: {
      names: ['req.domain' 'domain']
      desc: 'Host/domain name value that behaves like both full string and array of segments'
      type: '(string|string[])'
      default: 'localhost'
    }
    P: {
      names: ['req.path' 'path']
      desc: 'URL path value that behaves like both full string and array of segments'
      type: '(string|string[])'
      default: '/'
    }
    Q: {
      names: ['req.query' 'query']
      desc: 'Raw querystring that also exposes parsed multi-value parameters'
      type: '(string|string[]|(string[]|string){})'
      default: {}
    }
    C: {
      names: ['req.cookies' 'cookies']
      desc: 'Inbound cookie map'
      type: 'string{}'
      default: {}
    } 
    I: {
      names: ['req.ip' 'ip']
      desc: 'Client IP address as string'
      type: 'string'
      default: '127.0.0.1'
    }
    B: {
      names: ['req.body' 'body']
      desc: 'Request body as string'
      type: 'string'
      default: ''
    }
    // HTTP Response Output
    S: {
      names: ['res.status' 'status']
      desc: 'Response status code'
      type: 'integer'
      default: 200
    }
    RH: {
      names: ['res.headers']
      desc: 'Response headers as a read-only case-insensitive multi-value map'
      type: '(string[]|(string[]|string){})'
      default: {}
    }
    RB: {
      names: ['res.body']
      desc: 'Response body as string'
      type: 'string'
      default: ''
    }
    // Edge Config
    EC: {
      names: ['config']
      desc: 'Host-managed read-only project edge configuration attached by customers'
      type: 'any'
      read-only: true
      default: {}
    }
    SC: {
      names: ['secrets']
      desc: 'Host-managed read-only secrets store for sensitive configuration values'
      type: 'string{}'
      read-only: true
      default: {}
    }
  }
  functions: {
    // Logging
    li: {
      names: ['log.info']
      desc: 'Specialized logger for general informational messages about request processing and operations'
      args: { message: 'any' }
    }
    lw: {
      names: ['log.warning']
      desc: 'Specialized logger for warnings that should be monitored and investigated'
      args: { message: 'any' }
    }
    le: {
      names: ['log.error']
      desc: 'Specialized logger for errors that should be monitored and investigated'
      args: { message: 'any' }
    }
    // Response control
    rw: {
      names: ['res.rewrite' 'rewrite']
      desc: 'Performs internal rewrite to another path and restarts routing/middleware execution'
      args: { url: 'string' }
      returns: 'never'
    }
    rd: {
      names: ['res.redirect' 'redirect']
      desc: 'Performs HTTP redirect by returning a response with Location header and given status code'
      args: { url: 'string' status: 'number' message: 'string' }
      returns: 'never'
    }
    rp: {
      names: ['res.proxy' 'proxy']
      desc: 'Proxies request to another URL and returns the response'
      args: { url: 'string' }
    }
    // JSON serialization
    jp: {
      names: ['json.parse']
      desc: 'Parses JSON text into an object'
      args: { text: 'string' }
      returns: 'any'
    }
    js: {
      names: ['json.stringify']
      desc: 'Stringifies an object into JSON text'
      args: { value: 'any' }
      returns: 'string'
    }
  }
}

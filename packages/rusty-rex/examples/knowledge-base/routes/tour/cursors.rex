/* Tour Stop: Live Cursors — real-time collaboration demo */
res.headers.content-type = "text/html"
layout = fs.read("routes/_layouts/page.html")

body = html`<h1>Live Cursors</h1>
<p>Move your mouse around. Every connected browser sees everyone else's cursor in
real-time. Open this page in multiple windows to see it in action.</p>

<div class="card">
<p><strong>How it works:</strong> Each browser connects to a WebSocket pub/sub channel
(<code>/__ws/cursors</code>). Mouse movements are published as JSON messages.
Every subscriber receives them and renders colored dots.</p>
<p>The server is just a relay — no Rex script processes cursor events. The
<code>kv.publish</code>/<code>kv.subscribe</code> system handles fan-out natively.
A Rex middleware layer could add auth, rate limiting, or spatial filtering.</p>
</div>

<div id="cursor-area" style="position:relative;height:60vh;border:1px solid var(--border);border-radius:0.5rem;margin:1.5rem 0;overflow:hidden;cursor:crosshair;background:var(--surface)">
  <div id="status" style="position:absolute;top:0.75rem;left:1rem;font-size:0.85rem;color:var(--muted)">Connecting...</div>
</div>

<details class="try-it">
<summary>How the pub/sub channel works</summary>
<p>The WebSocket endpoint <code>/__ws/cursors</code> is a generic pub/sub channel.
Any message sent by a client is broadcast to all other subscribers. The server
maintains the channel in an in-memory KV store with automatic cleanup.</p>
<pre>/* Any Rex handler can also publish to this channel: */
kv.publish("cursors", json.stringify({x: 100, y: 200, color: "#ff0000"}))</pre>
</details>`

cursor-script = `<script>
(function() {
  var COLORS = ['#ff6b6b','#ffd93d','#6bcb77','#4d96ff','#ff9ff3','#54a0ff','#5f27cd','#01a3a4'];
  var myId = Math.random().toString(36).slice(2, 8);
  var myColor = COLORS[Math.floor(Math.random() * COLORS.length)];
  var cursors = {};
  var area = document.getElementById('cursor-area');
  var status = document.getElementById('status');
  var ws, reconnectTimer;

  function connect() {
    ws = new WebSocket('ws://' + location.host + '/__ws/cursors');
    ws.onopen = function() { status.textContent = 'Connected — move your mouse'; };
    ws.onclose = function() {
      status.textContent = 'Disconnected — reconnecting...';
      reconnectTimer = setTimeout(connect, 1000);
    };
    ws.onerror = function() { ws.close(); };
    ws.onmessage = function(e) {
      try {
        var msg = JSON.parse(e.data);
        if (msg.id === myId) return;
        if (msg.gone) { removeCursor(msg.id); return; }
        updateCursor(msg.id, msg.x, msg.y, msg.color, msg.name);
      } catch(err) {}
    };
  }

  function updateCursor(id, x, y, color, name) {
    var el = cursors[id];
    if (!el) {
      el = document.createElement('div');
      el.style.cssText = 'position:absolute;pointer-events:none;transition:left 0.05s,top 0.05s;z-index:10';
      el.innerHTML = '<svg width="16" height="20" viewBox="0 0 16 20"><path d="M0 0l16 12h-9l-3 8z" fill="' + color + '" stroke="rgba(0,0,0,0.3)" stroke-width="0.5"/></svg>'
        + '<span style="position:absolute;left:16px;top:12px;font-size:11px;color:' + color + ';white-space:nowrap;font-weight:600">' + (name || id) + '</span>';
      area.appendChild(el);
      cursors[id] = el;
    }
    el.style.left = (x * 100) + '%';
    el.style.top = (y * 100) + '%';
  }

  function removeCursor(id) {
    if (cursors[id]) { cursors[id].remove(); delete cursors[id]; }
  }

  area.addEventListener('mousemove', function(e) {
    if (!ws || ws.readyState !== 1) return;
    var rect = area.getBoundingClientRect();
    var x = (e.clientX - rect.left) / rect.width;
    var y = (e.clientY - rect.top) / rect.height;
    ws.send(JSON.stringify({id: myId, x: x, y: y, color: myColor, name: myId}));
  });

  area.addEventListener('mouseleave', function() {
    if (ws && ws.readyState === 1) {
      ws.send(JSON.stringify({id: myId, gone: true}));
    }
  });

  window.addEventListener('beforeunload', function() {
    if (ws && ws.readyState === 1) {
      ws.send(JSON.stringify({id: myId, gone: true}));
    }
  });

  connect();
})();
</script>`

template.render(layout, {
  title: "Live Cursors"
  body: body + cursor-script
  footer: "<a href='/tour/experience'>&larr; DX Report</a> &middot; <a href='/'>Home</a>"
})

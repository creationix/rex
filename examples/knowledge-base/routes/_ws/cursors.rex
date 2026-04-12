/* WebSocket transform for the cursors channel.
   Runs on every message before publishing to subscribers.
   Receives: event.data (JSON string)
   Returns: transformed message (string/object) or none to suppress */

msg = json.parse(event.data)

/* Pass through disconnect messages unchanged */
when msg.gone do
  return event.data
end

/* Mirror the y-axis — cursors appear inverted vertically */
y = msg.y
when msg.x and y do
  return json.stringify({
    id: msg.id
    x: msg.x
    y: 1 - y
    color: msg.color
    name: msg.name
  })
end

/* Pass through anything else unchanged */
event.data

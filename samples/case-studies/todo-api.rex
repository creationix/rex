// Todo API — CRUD operations with type checking
// rex check --domain todo-api.rexd todo-api.rex

when method == "GET" do
  entries = db.list("todo:")
  todos = [json.parse(e.value) for e in entries]
  return {ok: true, todos: todos}
end

when method == "POST" do
  input = json.parse(body)

  unless input and input.title do
    status = 422
    return {ok: false, error: "title required"}
  end

  id = time.now()
  todo = {
    id: id
    title: input.title
    done: false
  }
  db.set(`todo:${id}`, json.stringify(todo))
  status = 201
  return {ok: true, todo: todo}
end

when method == "DELETE" do
  input = json.parse(body)

  unless input and input.id do
    status = 422
    return {ok: false, error: "id required"}
  end

  db.delete(`todo:${input.id}`)
  return {ok: true, deleted: input.id}
end

status = 405
{ok: false, error: "method not allowed"}

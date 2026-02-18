# Rex for Visual Studio Code

Syntax highlighting for [Rex](https://github.com/creationix/rex) — programmable JSON with small arms and a big bite.

![Rex syntax highlighting](img/highlight-example.png)

## Features

- Syntax highlighting for `.rex` files
- Rex code blocks in Markdown (` ```rex `)
- Rex tagged template literals in TypeScript and JavaScript
- Parser-backed diagnostics for `.rex`
- Outline, Go to Definition, and Find References for local Rex symbols
- Optional domain-aware completion and hover via `.config.rex` at workspace root

## Domain Schema (`.config.rex`)

To provide domain API completions/hover without imports, add `.config.rex` in your repo root:

- The extension reads only the current `.config.rex` format (`data` / `functions` entries with `names`, `type`, `desc`, optional `args`/`returns`).

```rex
{
  data: {
    H: {
      names: ['headers']
      type: 'object'
      desc: 'Inbound request headers'
    }
  }
}
```

Then in `.rex` files:

- Typing `headers.` offers property completions
- Hovering `headers` or `headers.x-action` shows type/docs

## What is Rex?

Rex is a data format that extends JSON with high-level infix logic. Any valid JSON is already valid Rex, and you can add conditionals, assignment, loops, and comprehensions directly in source. Rex compiles to compact `rexc` bytecode that you can store, serialize, and diff like any other data.

```rex
actions = {
  create-user: "users/create"
  delete-user: "users/delete"
  update-profile: "users/update-profile"
}

when handler = actions.(headers.x-action) do
  headers.x-handler = handler
end
```

Learn more at [github.com/creationix/rex](https://github.com/creationix/rex).

# rexc Token Map

Working document for mapping rexc tokens as a superset of RX.

## Non-b64 printable ASCII

These are all the characters available as tags (everything outside the b64 digit alphabet `0-9 a-z A-Z - _`):

```
!  "  #  $  %  &  '  (  )  *  +  ,  .  /  :  ;  <  =  >  ?  @  [  \  ]  ^  {  |  }  ~
```

29 characters total.

---

## RX layer (locked — cannot change)

These are claimed by the RX data format. rexc inherits them as-is.

| Tag | Name    | Kind          | Layout                 |
|-----|---------|---------------|------------------------|
| `+` | Number  | scalar        | `+[zigzag]`            |
| `*` | Decimal | scalar prefix | `[base]+[base]*[exp]`  |
| `,` | String  | sized body    | `[bytes],[length]`     |
| `'` | Ref     | scalar        | `'[name]`              |
| `;` | List    | sized body    | `[children];[size]`    |
| `:` | Map     | sized body    | `[pairs]:[size]`       |
| `^` | Pointer | scalar        | `^[delta]`             |
| `.` | Chain   | sized body    | `[segments].[size]`    |
| `#` | Index   | sized body    | `[entries]#[compound]` |

9 tags claimed. Reading direction is right-to-left. Sized bodies always carry a content-size varint.

---

## rexc language layer

20 characters remaining. Paired delimiters must match (no `(` without `)`).

### Scalars (tag + varint, no body)

| Tag  | Name         | Varint meaning        | Status         |
|------|--------------|-----------------------|----------------|
| `$`  | Variable     | name (string)         | keep           |
| `%`  | Opcode       | mnemonic (string)     | keep           |
| `\`  | Loop control | kind + depth (int)    | keep           |

For `$` and `%`, varint is a string identifier (b64 chars = name), not numeric. Same convention as `'` refs.

Break/continue encoding: `kind = n % 2` (0=break, 1=continue), `depth = floor(n/2) + 1`. Requires `\\` escaping in JSON strings.

### Paired containers (opener + body + closer, optional size varint after closer)

| Pair    | Name       | Evaluation | Returns    | Status |
|---------|------------|------------|------------|--------|
| `(` `)` | Call       | eager      | call result | keep  |
| `[` `]` | Eager list | eager, sequential | list of all results | **new** |
| `{` `}` | Do block   | eager, sequential | last result | **new** |

### Compound openers (prefix + paired container)

Old rexc used a prefix character before `(`, `[`, or `{` to form compound openers. The prefix determines the semantics; the delimiter determines the container shape.

| Closer | Opener | Name                        | Status                       |
|--------|--------|-----------------------------|------------------------------|
| `(`    | `)?`   | Cond (variadic when/unless) | keep                         |
| `(`    | `)\|`  | Or (variadic)               | keep                         |
| `(`    | `)&`   | And (variadic)              | keep                         |
| `(`    | `)>`   | For-in loop                 | keep                         |
| `[`    | `]>`   | For-in list comprehension   | keep                         |
| `{`    | `}>`   | For-in object comprehension | keep                         |
| `(`    | `)<`   | For-of loop                 | keep                         |
| `[`    | `]<`   | For-of list comprehension   | keep                         |
| `{`    | `}<`   | For-of object comprehension | keep                         |
| `(`    | `)#`   | While loop                  | keep, `#` is index, but safe |
| `[`    | `]#`   | While list comp             | keep                         |
| `{`    | `}#`   | While object comp           | keep                         |

### Fixed-arity operators (tag + body + optional size varint)

| Tag | Name     | Body contains | Status |
|-----|----------|---------------|--------|
| `=` | Set      | place, value  | keep   |
| `/` | Swap-set | place, value  | keep   |
| `~` | Delete   | place         | keep   |

---

## Evaluation model

RX data containers (`;` `:`) use **lazy evaluation** — rexc code embedded inside is only executed when accessed. This enables random access without unwanted side effects.

The paired delimiters provide **eager evaluation** — all expressions are executed sequentially, in order.

| Container | Evaluation | Returns         | Example              |
|-----------|-----------|-----------------|----------------------|
| `;` `:`   | lazy (on access) | data structure | `+4(ad%x$2+);4` — list where elements compute on read |
| `[` `]`   | eager, sequential | list of all results | `[a b c]` → `[a, b, c]` |
| `{` `}`   | eager, sequential | last result only | `{a b c}` → `c` |
| `(` `)`   | eager | call result | `(ad%2+4+)` → `3` |

Comprehensions (`>[...]`, `<[...]`, `>[{...]`, etc.) are eager — they iterate and collect.

---

## Resolved

- **While compounds** — `#` is safe as a compound suffix after `)`, `]`, `}` because RX `#` is always followed by b64 index data. No ambiguity. `)#`, `]#`, `}#` all keep working.
- **Bare strings** — dropped. Removed as part of the RX split. Use `,` strings everywhere.
- **`{ }` bare** — do block. Eager sequential evaluation, returns last result.
- **`[ ]` bare** — eager list. Eager sequential evaluation, returns all results as a list.
- **`\` loop control** — break/continue scalar. Replaces old `;` which was claimed by RX list.

---

## Available characters

Completely unassigned (no role at all):

```
"  @  !
```

Used only as compound suffixes (could also serve as standalone scalars):

```
?  &  >  <
```

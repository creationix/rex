# Known Issues

## Formatter is lossy

`rex format` round-trips through the compiler pipeline (lex -> parse -> lower -> decompile), which loses information:

- Comments are stripped
- `extern` declarations are stripped (no bytecode representation)
- Type annotations are lost (`bonus: int = 10` -> `bonus = int`)
- Dynamic navigation becomes static (`grades.(subj)` -> `grades.subj`)
- No trailing newline

The LSP formatting provider is disabled until this is fixed.

**Fix:** Implement a CST-based formatter that operates directly on the parse tree, adjusting only whitespace and indentation while preserving all tokens.

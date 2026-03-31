# rex-lang (legacy)

> **This package is from the original TypeScript implementation.** The canonical Rex compiler is now written in Rust at [`packages/rusty-rex/crates/rex-core`](../rusty-rex/crates/rex-core). This package may not support all current language features (template literals, `return`, variadic `and`/`or`, `type`/`extern` declarations, etc.).

Original TypeScript compiler for Rex, built on the [Ohm](https://ohmjs.org/) parsing framework. Includes the grammar (`rex.ohm`), parser, lowerer, and bytecode encoder.

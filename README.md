# oop-worm

A larvae worm that holds the privacy line in Luau OOP. An underscore or
`@private` member belongs to its class. Outside the class, a use of it
does not lint clean. Inside the class, everything works as before.

The worm claims no extension. Its lints run over every `.luau` file
through `lints_luau`, and over the Luau shadow of a claimed file when the
claiming worm sets `shared` (the luaux worm does).

## What counts as inside

A class is a table a statement binds by name: `local Class = {}` or
`Class = setmetatable({}, Super)`. These places belong to it:

- a function stored on it: `function Class.m()`, `function Class:m()`,
  `Class.m = function()`
- the constructor table itself, nested tables included
- a table stored on it: `Class.handlers = { ... }`
- `self.anything`, wherever `self` appears

Inside those, privates lint clean, on any receiver the class declares:
`other._x` inside `Vec:add(other)` works. The defining file also keeps
its metamethod idiom: `Class.__index = Class` is legal at module scope.
Everything else is outside.

## Lints

Set levels under `[lint.rules.oop]`; the names below carry the default.

- `private_access` (**deny**) — an underscore or `@private` member is
  used outside a function or a table of its class.
- `implicit_self` (warn) — `function Class:m()` leaves `self` untyped;
  `function Class.m(self: typeof(Class))` keeps the type.
- `missing_index` (warn) — `setmetatable(x, Class)` runs and
  `Class.__index` is never set, so method calls on the instance fail.
- `unused_private` (warn) — a private member that nothing in its own
  file uses is dead, because outside files must not touch it.

A `-- larvae: allow(oop.private_access)` comment holds a line exempt,
like any other lint.

## Editor

The lints reach the editor through `larvae lsp`, so a private access
shows as a diagnostic while you type.

The worm no longer filters completion lists or marks private members on
hover. Since larvae 0.10, larvae serves lint, format, and fixes in the
editor, and luau-lsp serves completion and hover. The
`hide_private_completions` setting went with the filter.

## Install

```toml
[worms]
oop = { path = "../oop-worm" }
```

Build the worm with `cargo build --release`; `entry` points at the
binary.

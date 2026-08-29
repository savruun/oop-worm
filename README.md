# oop-worm

A larvae worm that holds the privacy line in Luau OOP. An underscore or
`@private` member belongs to its class; outside the class it neither
lints clean nor completes, and inside the class everything works as
before.

The worm claims no extension. Its lints run over every `.luau` file
through `lints_luau`, and over the Luau shadow of a claimed file when the
claiming worm sets `shared` (the luaux worm does). Its editor hooks
answer in the same files.

## What counts as inside

A class is a table a statement binds by name: `local Class = {}` or
`Class = setmetatable({}, Super)`. These places belong to it:

- a function stored on it: `function Class.m()`, `function Class:m()`,
  `Class.m = function()`
- the constructor table itself, nested tables included
- a table stored on it: `Class.handlers = { ... }`
- `self.anything`, wherever `self` appears

Inside those, privates lint clean and complete, on any receiver the
class declares: `other._x` inside `Vec:add(other)` works. The defining
file also keeps its metamethod idiom: `Class.__index = Class` is legal
at module scope. Everything else is outside.

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

- Completions after `.` or `:` drop underscore members and members whose
  docs carry `@private`, outside the class. Inside the class, and on
  `self.`, the full list stays.
- Hover keeps answering for a fully typed private, and the card gains a
  `private by convention` line.

The filter is a setting:

```toml
[worms.oop.config]
hide_private_completions = false
```

## Install

```toml
[worms]
oop = { path = "../oop-worm" }
```

Build the worm with `cargo build --release`; `entry` points at the
binary.

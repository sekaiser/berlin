# Rust publication composition

`berlin_publication::view!` constructs the same `ComponentCall` tree as the
ordinary typestate builders. It does not render markup or load a theme.

```rust
use berlin_publication::*;

let view = view! {
    Portfolio() {
        Introduction(text: "Selected engineering work.")
        Section(title: "Storage") {
            slot body {
                for source in selected {
                    FeaturedWork(source: source, annotation: None)
                }
            }
        }
    }
};
```

Component paths use Rust paths. Property values are Rust expressions, evaluated
once in written order. `key` assigns an occurrence key. Child bodies map to
`body`; explicit `slot name { ... }` bodies support other declared slots.
`for pattern in expression { ... }`, `if condition { ... } else { ... }`, and
`(helper_returning_fragment())` preserve order. Parenthesize an iterator or
condition expression when its braces would otherwise look like the view body.
There is no `else if` shorthand: nest an `if` in the `else` body.

Unknown or duplicate setters, wrong types and omitted required properties or
slots fail compilation. Empty loop results, duplicate occurrence keys, unknown
sources and resource limits are checked by the shared runtime registry. A typed
view still needs publication resolution before rendering.

## Component contracts

```rust
use berlin_publication::*;

#[derive(Component)]
#[component(
    key = "example.panel@1",
    channels(html, text),
    slot(name = "body", required),
    slot(name = "caption", maximum = 1)
)]
pub struct Panel {
    pub title: String,
    pub explanation: Option<String>,
}
```

The derive supplies `Component::schema`, typed property lowering and
`Panel::builder()`. Required setters must be called before `build`; each setter
can be called once. Optional properties default to `None`. A missing optional
slot normalizes to an empty fragment. Register contracts explicitly with
`RegistryBuilder::register::<Panel>()` and provide each channel's interpretation.

Supported property types implement `ComponentProperty`: strings, booleans,
signed integers through `i64` (also `isize`), source/excerpt references, built-in
enums, `Option<T>` and `Vec<T>`. Property and tree budgets bound vectors at
normalization. An external type can deliberately implement the trait; arbitrary
serialization is not a schema. Generic component structs are not supported.

When the runtime dependency is renamed, `view!` follows the facade's hygienic
crate path automatically. On a derived component, set `runtime = notebook` in
the attribute if the dependency is named `notebook`. This explicit override
keeps procedural expansion free of manifest scanning and environment reads.

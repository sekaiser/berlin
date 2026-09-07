<a id="code-snippet--country-extraction"></a>
```rust { linenos=true, linenostart=20, anchorlinenos=true, lineanchors=org-coderef--c81016 }
fn country_name(value: &Value) -> &Value {
    &value["countryObject"]["name"]
}
```

The [country lookup](#org-coderef--c81016-21) extracts the nested name.

See [the complete example](#code-snippet--country-extraction).

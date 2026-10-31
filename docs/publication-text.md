# Direct publication text

`render_text` interprets a resolved publication independently of
HTML. It walks the shared component/semantic document representation; it neither
loads Tera nor renders and strips HTML.

```rhai
output render_text(composed, "_text",
    text_config(#{profile: "linkedin_draft"}));
```

The output directory contains `publication.txt` and `manifest.json`. Source
titles, original URLs, descriptions, annotations, selected excerpt content,
attribution and authored ordering remain present. Text keeps literal characters
such as `&` and `<` rather than HTML entities. Code indentation and whitespace
are retained. Source-local assets and links use original-source URLs.

`plain` produces unrestricted draft text within the configured output budget.
`linkedin_draft` uses Berlin's existing Unicode-character count and 3,000-character
draft policy. Over-limit text is retained in full and accompanied by a diagnostic
in the text manifest, check report and operational receipt. It is not truncated
and is never submitted remotely. A character-count diagnostic is not publication
approval or evidence that a remote platform accepted the text.

Every built-in has an explicit text interpretation. Unknown components,
unregistered semantic source components and portable raw HTML fail. PDF, slides
and platform API integrations are outside this renderer.

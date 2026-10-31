// Synthetic, offline composition. No author content or deployment credentials.
fn system_design(catalog) {
    let selected = catalog.select( #{
        topics_any: ["system-design", "storage"],
        published_from: "2025-01-01",
        order: "published_desc",
        limit: 8
    });
    let entries = [];
    for source in selected {
        entries.push(view_component("berlin.featured_work@1", #{source: source}, #{}));
    }
    let group = view_component("berlin.section@1", #{title: "Storage and reliability"}, #{body: entries});
    let root = view_component("berlin.portfolio@1", #{}, #{body: [
        view_component("berlin.introduction@1", #{text: "Selected synthetic work and the decisions behind it."}, #{}), group
    ]});
    publication(#{id: "system-design", title: "Selected Work in System Design"}, root)
}

fn curated(catalog) {
    let ordered = catalog.select( #{ids: ["local:queues", "local:storage"], order: "curated"});
    let entries = [];
    for source in ordered {
        entries.push(view_component("berlin.featured_work@1", #{source: source, annotation: "An editorial annotation for this publication."}, #{}));
    }
    publication(#{id: "curated", title: "Two Design Decisions"},
        view_component("berlin.portfolio@1", #{}, #{body: entries}))
}

fn excerpt_publication(catalog) {
    let excerpt = section_ref(catalog, "local:storage", "handoff");
    publication(#{id: "handoffs", title: "The Handoff Contract"},
        view_component("berlin.portfolio@1", #{}, #{body: [
            view_component("berlin.introduction@1", #{text: "One source section, reused with two distinct editorial placements."}, #{}),
            view_component("berlin.excerpt@1", #{source: excerpt, key: "first", annotation: "Read the invariant first."}, #{}),
            view_component("berlin.excerpt@1", #{source: excerpt, key: "second", annotation: "The same section can support another discussion."}, #{}),
            view_component("berlin.related_work@1", #{sources: [resource_ref(catalog, "local:queues")], title: "Continue with queue admission"}, #{})
        ]}))
}

pipeline portfolio {
    let documents = documents(load_markdown("content/*.md"));
    let composed = compose(catalog([documents], publication_options(#{source_site_url: "https://example.test/notebook"})), Fn("system_design"));
    output render_html(composed, "_portfolio", html_config(#{theme: "@notebook", url: "https://example.test/portfolio", author: "Example Author"}));
}
pipeline curated {
    let composed = compose(catalog([documents(load_markdown("content/*.md"))], publication_options(#{source_site_url: "https://example.test/notebook"})), Fn("curated"));
    output render_html(composed, "_curated", html_config(#{theme: "@notebook", url: "https://example.test/curated"}));
}
pipeline excerpts {
    let composed = compose(catalog([documents(load_markdown("content/*.md"))], publication_options(#{source_site_url: "https://example.test/notebook"})), Fn("excerpt_publication"));
    output render_html(composed, "_excerpts", html_config(#{theme: "@notebook", url: "https://example.test/excerpts"}));
}
pipeline text {
    let composed = compose(catalog([documents(load_markdown("content/*.md"))], publication_options(#{source_site_url: "https://example.test/notebook"})), Fn("system_design"));
    output render_text(composed, "_text", text_config(#{profile: "plain"}));
}
pipeline excerpt_text {
    let composed = compose(catalog([documents(load_markdown("content/*.md"))], publication_options(#{source_site_url: "https://example.test/notebook"})), Fn("excerpt_publication"));
    output render_text(composed, "_excerpt_text", text_config(#{profile: "linkedin_draft"}));
}

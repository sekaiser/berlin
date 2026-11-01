// Synthetic Org notebook. Custom composers are optional.
pipeline org {
    output export_org(load_org("data/*.org"), "ox-hugo", ".berlin/generated/org", "notes");
}
let content = catalog([
    documents(load_markdown(".berlin/generated/org/content/notes/*.md")),
    resources(load_data("data/resources.yaml"))
], publication_options(#{source_site_url: "https://example.com"}));

pipeline site {
    let website = compose(content, notebook(#{
        id: "example-notebook", title: "Example notebook",
        pages: [#{id: "about", route: "about.html", title: "About",
                  introduction: "A synthetic publishing fixture."}]
    }));
    output render_html(website, "_site", html_config(#{
        theme: "@notebook", url: "https://example.com", title: "Example notebook",
        author: "Example Author", description: "A synthetic publishing fixture"
    })).with_assets(load_assets(".berlin/generated/org/static/attachments/**/*").named("org_assets"),"attachments");
}
pipeline linkedin {
    output render_text(compose(content, article_draft()), ".berlin/linkedin", text_config(#{profile: "linkedin_draft"}));
}

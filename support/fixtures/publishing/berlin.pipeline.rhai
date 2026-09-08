// Synthetic publishing project used by Berlin's tests and acceptance check.
pipeline org {
    output export_org(load_org("data/*.org"), "ox-hugo", ".berlin/generated/org", "notes");
}
fn load_documents() {
    parse_markdown(load_markdown(".berlin/generated/org/content/notes/*.md"))
}
pipeline site {
    let presentation = website_config(#{
        url: "https://example.com",
        title: "Example notebook",
        author: "Example Author",
        description: "A synthetic publishing fixture"
    });
    let website = assemble_website(load_documents(), parse_feed(load_data("data/feed.csv")));
    output render_website(website, "_site", presentation);
    output compile_css(load_css("css/styles.css"), "_site/css/styles.css");
    output copy_assets(load_assets("static/**/*"), "_site/static");
    output copy_assets(
        load_assets(".berlin/generated/org/static/attachments/**/*").named("org_assets"),
        "_site/attachments"
    ).named("published_org_assets");
}
pipeline linkedin {
    output render_linkedin(load_documents(), ".berlin/linkedin");
}

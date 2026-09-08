// Synthetic theme specimen. No personal content or remote publishing.
pipeline site {
    let website = assemble_website(
        parse_markdown(load_markdown("content/notes/*.md")),
        parse_feed(load_data("data/feed.csv"))
    );
    output render_website(website, "_site", website_config(#{
        theme: "themes/notebook",
        url: "http://localhost:8082",
        title: "Notebook specimens",
        author: "Example Author",
        description: "Synthetic examples for inspecting the notebook theme."
    }));
}

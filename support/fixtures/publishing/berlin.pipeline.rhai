// Synthetic Org notebook using the same catalog/composition path as portfolios.
pipeline org {
    output export_org(load_org("data/*.org"), "ox-hugo", ".berlin/generated/org", "notes");
}
fn sources() {
    catalog([
        documents(load_markdown(".berlin/generated/org/content/notes/*.md")),
        resources(load_data("data/resources.yaml"))
    ],publication_options(#{source_site_url:"https://example.com"}))
}
fn notebook(c) {
    let authored=c.select(#{origins_any:["local"],order:"published_desc"});
    let reading=c.select(#{origins_any:["external"],order:"added_desc"});
    let result=site("example-notebook",#{title:"Example notebook"});
    result.add_page("home","index.html",home_page(authored,reading));
    result.add_page("notes","notes.html",publication("notes",#{title:"Notes"},[section("Notes",authored.map_featured())]));
    result.add_page("reading","feed.html",publication("reading",#{title:"Reading"},[reading_list("Reading",reading.map_cards())]));
    result.add_page("about","about.html",publication("about",#{title:"About"},[view_component("berlin.introduction@1",#{text:"A synthetic publishing fixture."},#{})]));
    result.add_document_pages(authored,"notes");
    let all=c.select(#{order:"id_asc"});
    result.add_tag_pages(all);
    result.add_search(all);
    result.add_redirects(authored);
    result
}
fn article_drafts(c) {
    let entries=[];
    for source in c.select(#{origins_any:["local"],order:"id_asc"}) {
        entries.push(view_component("berlin.excerpt@1",#{source:document_excerpt(c,source.id),key:source.id},#{}));
    }
    publication("article-drafts",#{title:"Notebook drafts"},entries)
}
pipeline site {
    let presentation = html_config(#{
        theme: "@notebook",
        url: "https://example.com",
        title: "Example notebook",
        author: "Example Author",
        description: "A synthetic publishing fixture"
    });
    output render_html(compose(sources(),Fn("notebook")),"_site",presentation)
        .with_assets(load_assets(".berlin/generated/org/static/attachments/**/*").named("org_assets"),"attachments");
}
pipeline linkedin {
    output render_text(compose(sources(),Fn("article_drafts")),".berlin/linkedin",text_config(#{profile:"linkedin_draft"}));
}

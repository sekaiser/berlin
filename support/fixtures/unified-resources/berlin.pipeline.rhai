// Offline synthetic project. Source originals are explicit catalog configuration,
// shared by HTML and text; destination mounts may differ between presentations.
fn sources() {
    catalog([
        documents(load_markdown("content/*.md")),
        resources(load_data("data/resources.yaml"))
    ], publication_options(#{source_site_url:"https://example.test/notebook"}))
}
fn system_design_portfolio(catalog) {
    let authored=catalog.select(#{origins_any:["local"],topics_any:["system-design","distributed-systems"],order:"published_desc"});
    let references=catalog.select(#{origins_any:["external"],topics_any:["storage","distributed-systems"],order:"title_asc"});
    let work=section("Selected work",authored.map_featured());
    let reading=reading_list("References",references.map_cards());
    publication("system-design",#{title:"Selected system design work"},[work,reading])
}
fn notebook(catalog) {
    let authored=catalog.select(#{origins_any:["local"],order:"published_desc"});
    let references=catalog.select(#{origins_any:["external"],order:"added_desc"});
    let site=site("notebook",#{title:"Engineering notebook"});
    site.add_page("home","index.html",home_page(authored,references));
    let writing=section("Notes and guides",authored.map_featured());
    site.add_page("writing","notes.html",publication("writing",#{title:"Notes"},[writing]));
    let reading=reading_list("Reading",references.map_cards());
    site.add_page("reading","feed.html",publication("reading",#{title:"Reading"},[reading]));
    let about=view_component("berlin.introduction@1",#{text:"An explicitly authored synthetic about page."},#{});
    site.add_page("about","about.html",publication("about",#{title:"About"},[about]));
    site.add_document_pages(authored,"notes");
    let all=catalog.select(#{order:"id_asc"});
    site.add_tag_pages(all);
    site.add_search(all);
    site.add_redirects(authored);
    site
}
fn excerpt_portfolio(catalog) {
    let source=section_ref(catalog,"local:footnotes-example","prefix");
    let first=view_component("berlin.excerpt@1",#{source:source,key:"first",annotation:"First reading."},#{});
    let second=view_component("berlin.excerpt@1",#{source:source,key:"second",annotation:"Another placement."},#{});
    publication("excerpts",#{title:"A prefix, twice"},[first,second])
}
pipeline portfolio {
    output render_html(compose(sources(),Fn("system_design_portfolio")),"_portfolio",html_config(#{theme:"@notebook",url:"https://example.test/portfolio",author:"Example Author"}));
}
pipeline text {
    output render_text(compose(sources(),Fn("system_design_portfolio")),"_text",text_config(#{profile:"plain"}));
}
pipeline site {
    output render_html(compose(sources(),Fn("notebook")),"_site",html_config(#{theme:"@notebook",url:"https://example.test/notebook",title:"Engineering notebook",author:"Example Author"}))
        .with_assets(load_assets("attachments/**/*"),"attachments");
}
pipeline excerpts {
    output render_html(compose(sources(),Fn("excerpt_portfolio")),"_excerpts",html_config(#{theme:"@notebook",url:"https://example.test/excerpts"}));
}
pipeline excerpt_text {
    output render_text(compose(sources(),Fn("excerpt_portfolio")),"_excerpt_text",text_config(#{profile:"linkedin_draft"}));
}

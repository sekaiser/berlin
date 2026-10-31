// Synthetic theme specimen. Local preview only.
fn reading_cards(selected) {
    let annotations=#{"external:reference-1":"Synthetic annotation used to inspect the reading-list layout.","external:reference-3":"Another synthetic reference for checking combinations of selected tags.","external:stress-reference-1":"This entry remains available when no tag is selected.","external:stress-reference-2":"Twenty tags exercise the filter controls and entry metadata using the real reading-list implementation."};
    let cards=[];
    for source in selected {
        let props=#{resource:source};
        if annotations.contains(source.id) {props.annotation=annotations[source.id];}
        cards.push(view_component("berlin.resource_card@1",props,#{}));
    }
    cards
}
fn notebook(c) {
    let authored=c.select(#{origins_any:["local"],order:"published_desc"});
    let reading=c.select(#{origins_any:["external"],order:"added_desc"});
    let result=site("example-notebook",#{title:"Example notebook"});
    result.add_page("home","index.html",home_page(authored,reading));
    let notes=[];
    if authored.len()>0 {notes.push(section("Notes",authored.map_featured()));}
    else {notes.push(view_component("berlin.introduction@1",#{text:"No notes yet."},#{}));}
    result.add_page("notes","notes.html",publication("notes",#{title:"Notes"},notes));
    let references=[];
    if reading.len()>0 {references.push(reading_list("Reading",reading_cards(reading)));}
    else {references.push(view_component("berlin.introduction@1",#{text:"No saved references yet."},#{}));}
    result.add_page("reading","feed.html",publication("reading",#{title:"Reading"},references));
    result.add_page("about","about.html",publication("about",#{title:"About"},[view_component("berlin.introduction@1",#{text:"A synthetic publishing fixture."},#{})]));
    result.add_document_pages(authored,"notes");
    let all=c.select(#{order:"id_asc"});
    result.add_tag_pages(all);
    result.add_search(all);
    result.add_redirects(authored);
    result
}
pipeline site {
    let sources=catalog([
        documents(load_markdown("content/notes/*.md")),
        resources(load_data("data/resources.yaml"))
    ],publication_options(#{source_site_url:"http://localhost:8082"}));
    output render_html(compose(sources,Fn("notebook")),"_site",html_config(#{
        theme:"themes/notebook",url:"http://localhost:8082",title:"Notebook specimens",
        author:"Example Author",description:"Synthetic examples for inspecting the notebook theme."
    })).with_assets(load_assets("authoring/*.org"),"examples/org");
}

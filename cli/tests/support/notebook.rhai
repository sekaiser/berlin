// Shared synthetic intentions, compiled into each test's explicit pipeline input.
fn notebook(c) {
    let authored=c.select(#{origins_any:["local"],order:"published_desc"});
    let opening=view_component("berlin.introduction@1",#{text:"A synthetic notebook."},#{});
    let entries=[opening];
    if authored.len()>0 {entries.push(section("Notes",authored.map_featured()));}
    let result=site("test-notebook",#{title:"Test notebook"});
    result.add_page("home","index.html",publication("home",#{title:"Notebook"},entries));
    result.add_page("notes","notes.html",publication("notes",#{title:"Notes"},entries));
    result.add_page("reading","feed.html",publication("reading",#{title:"Reading"},[opening]));
    result.add_page("about","about.html",publication("about",#{title:"About"},[opening]));
    result.add_document_pages(authored,"notes");
    result.add_tag_pages(authored);
    result.add_search(authored);
    result.add_redirects(authored);
    result
}

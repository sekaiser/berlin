# Document Title


[render]
[[render.index.input]]
source = 'content/notes/*.md'
sort = { key = date_published, order = "desc" }

[[render.index.input]]
source = 'data/feed.csv'
aggregate = 'parse_csv'

[[render.index.params]]
name = 'index'


[[mount]]


Types of Documents (ParsedDocument)

MarkdownDocument
CsvDocument
TemplateDocument


render([ParsedDocument], RenderContext) -> [Page]

// data 
// referenced data

(data
 name: "notes"
 pattern: "content/notes/*.md"
 sort: {key = "date_published", order = "desc"})

(data
 name: "recent_notes"
 ref:  "notes"
 limit: 6)

(data
 name: "feed"
 pattern: "data/feed.csv"
 sort: {key = "date_published", order = "desc"}
 fn: aggregate_all) // split
 
(data
 name: "tags")

(pages
 name: "index"
 template: "index.tera"
 output: "index.html"
 data: {
   
 }
 
 input: [
    ('content/notes/*.md', bln_input_sort_by_date_published),
    ('data/feed.csv', bln_input_feed_aggregate_all)])
 params: {
   index: collect_articles,
   feed: parse_feed,
   tags: [('index', extract_tags), ('feed', extract_tags_from_feed)]
 }
 
```rust
RenderBuilder::new("index", "index.tera", "index.html")
// Files -> AggregatedSources
.input(&[
  Files("content/notes/*.md*")
  Aggregation(Files("content/notes/*.md*"), &fn_notes),
  Aggregation("data/feed.csv", &fn_feed)
  Aggregation("[file1.md, file2.md]", &fn_md)
  ])
// AggregatedSources -> Context 
.template_vars()
```

pub type InputAggregate<'a> =
    &'a dyn Fn(&str, &[ParsedSource], Option<SortFn>) -> AggregatedSources;
 

use files::FileInfo;
use parser::FrontMatter;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub file_info: FileInfo,
    pub meta: FrontMatter,
    /// The actual content of the page, e.g. markdown
    pub raw_content: String,
    pub slug: String,
    /// The URL path of the page, always starting with a slash
    pub path: String,
    pub components: Vec<String>,
    pub summary: String,
    /// The HTML rendered of the page
    pub content: String,
}

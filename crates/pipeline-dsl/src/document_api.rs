//! Rhai access to Berlin's semantic document model.
//!
//! This module owns the adapter types and API manifest exposed to document
//! mapper functions. It also applies section mappers recursively while keeping
//! Berlin's document representation out of Rhai-specific code elsewhere.

use berlin_document::Block;
use berlin_document::Document;
use berlin_document::Inline;
use berlin_document::ListItem;
use rhai::Dynamic;
use rhai::Engine;
use rhai::EvalAltResult;
use rhai::FnPtr;
use rhai::ImmutableString;
use rhai::NativeCallContext;

#[derive(Clone)]
pub(super) struct ScriptDocument(Document);

impl ScriptDocument {
    pub(super) fn new(document: Document) -> Self {
        Self(document)
    }

    pub(super) fn into_document(self) -> Document {
        self.0
    }

    fn id(&mut self) -> String {
        self.0.id.0.clone()
    }

    fn title(&mut self) -> String {
        self.0.metadata.title.clone().unwrap_or_default()
    }

    fn set_title(&mut self, title: ImmutableString) {
        self.0.metadata.title = Some(title.into());
    }

    fn description(&mut self) -> String {
        self.0.metadata.description.clone().unwrap_or_default()
    }

    fn set_description(&mut self, description: ImmutableString) {
        self.0.metadata.description = Some(description.into());
    }

    fn draft(&mut self) -> bool {
        self.0.metadata.draft
    }

    fn set_draft(&mut self, draft: bool) {
        self.0.metadata.draft = draft;
    }

    fn tags(&mut self) -> rhai::Array {
        self.0
            .metadata
            .tags
            .iter()
            .cloned()
            .map(Dynamic::from)
            .collect()
    }

    fn add_tag(&mut self, tag: ImmutableString) {
        let tag = tag.to_string();
        if !self.0.metadata.tags.contains(&tag) {
            self.0.metadata.tags.push(tag);
        }
    }

    fn remove_tag(&mut self, tag: ImmutableString) {
        self.0
            .metadata
            .tags
            .retain(|candidate| candidate != tag.as_str());
    }
}

#[derive(Clone)]
struct ScriptSection {
    id: berlin_document::ComponentId,
    level: u8,
    role: Option<String>,
    title: Vec<Inline>,
    blocks: Vec<Block>,
}

impl ScriptSection {
    fn id(&mut self) -> String {
        self.id.0.clone()
    }

    fn level(&mut self) -> i64 {
        i64::from(self.level)
    }

    fn role(&mut self) -> String {
        self.role.clone().unwrap_or_default()
    }

    fn set_role(&mut self, role: ImmutableString) {
        self.role = (!role.is_empty()).then(|| role.into());
    }

    fn title(&mut self) -> String {
        inline_plain_text(&self.title)
    }

    fn set_title(&mut self, title: ImmutableString) {
        self.title = vec![Inline::Text {
            value: title.into(),
        }];
    }
}

pub(super) fn register_document_api(engine: &mut Engine) {
    engine.register_type_with_name::<ScriptDocument>("Document");
    engine.register_get("id", ScriptDocument::id);
    engine.register_get("title", ScriptDocument::title);
    engine.register_set("title", ScriptDocument::set_title);
    engine.register_get("description", ScriptDocument::description);
    engine.register_set("description", ScriptDocument::set_description);
    engine.register_get("draft", ScriptDocument::draft);
    engine.register_set("draft", ScriptDocument::set_draft);
    engine.register_get("tags", ScriptDocument::tags);
    engine.register_fn("add_tag", ScriptDocument::add_tag);
    engine.register_fn("remove_tag", ScriptDocument::remove_tag);

    engine.register_type_with_name::<ScriptSection>("Section");
    engine.register_get("id", ScriptSection::id);
    engine.register_get("level", ScriptSection::level);
    engine.register_get("role", ScriptSection::role);
    engine.register_set("role", ScriptSection::set_role);
    engine.register_get("title", ScriptSection::title);
    engine.register_set("title", ScriptSection::set_title);
    engine.register_fn("map_sections", map_document_sections);
}

fn map_document_sections(
    context: NativeCallContext,
    mut document: ScriptDocument,
    mapper: FnPtr,
) -> Result<ScriptDocument, Box<EvalAltResult>> {
    document.0.blocks = map_sections(&context, &mapper, document.0.blocks)?;
    Ok(document)
}

fn map_sections(
    context: &NativeCallContext<'_>,
    mapper: &FnPtr,
    blocks: Vec<Block>,
) -> Result<Vec<Block>, Box<EvalAltResult>> {
    blocks
        .into_iter()
        .map(|block| map_sections_in_block(context, mapper, block))
        .collect()
}

fn map_sections_in_block(
    context: &NativeCallContext<'_>,
    mapper: &FnPtr,
    block: Block,
) -> Result<Block, Box<EvalAltResult>> {
    Ok(match block {
        Block::Section {
            id,
            level,
            role,
            title,
            blocks,
        } => {
            let section = ScriptSection {
                id,
                level,
                role,
                title,
                blocks: map_sections(context, mapper, blocks)?,
            };
            let section = mapper.call_within_context::<ScriptSection>(context, (section,))?;
            Block::Section {
                id: section.id,
                level: section.level,
                role: section.role,
                title: section.title,
                blocks: section.blocks,
            }
        }
        Block::BlockQuote { blocks } => Block::BlockQuote {
            blocks: map_sections(context, mapper, blocks)?,
        },
        Block::List(mut list) => {
            list.items = list
                .items
                .into_iter()
                .map(|item| {
                    Ok(ListItem {
                        checked: item.checked,
                        blocks: map_sections(context, mapper, item.blocks)?,
                    })
                })
                .collect::<Result<Vec<_>, Box<EvalAltResult>>>()?;
            Block::List(list)
        }
        Block::FootnoteDefinition { name, blocks } => Block::FootnoteDefinition {
            name,
            blocks: map_sections(context, mapper, blocks)?,
        },
        Block::Component {
            id,
            name,
            properties,
            blocks,
        } => Block::Component {
            id,
            name,
            properties,
            blocks: map_sections(context, mapper, blocks)?,
        },
        block => block,
    })
}

fn inline_plain_text(inlines: &[Inline]) -> String {
    let mut text = String::new();
    for inline in inlines {
        match inline {
            Inline::Text { value } | Inline::Code { value } | Inline::Html { value } => {
                text.push_str(value)
            }
            Inline::Emphasis { content }
            | Inline::Strong { content }
            | Inline::Strikethrough { content }
            | Inline::Link { content, .. } => text.push_str(&inline_plain_text(content)),
            Inline::DocumentLink(link) => text.push_str(&inline_plain_text(&link.content)),
            Inline::Image { description, .. } => text.push_str(&inline_plain_text(description)),
            Inline::SoftBreak | Inline::LineBreak => text.push(' '),
            Inline::FootnoteReference { name } => text.push_str(name),
        }
    }
    text
}

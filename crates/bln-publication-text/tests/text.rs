use berlin_content::DocumentCollection;
use berlin_document::*;
use berlin_publication::*;
use berlin_publication_text::*;

fn catalog(limits: PublicationLimits) -> PublicationCatalog {
    PublicationCatalog::new(
        DocumentCollection::new(vec![Document {
            id: ContentId("source".into()),
            kind: DocumentKind::Article,
            metadata: Metadata {
                title: Some("A source".into()),
                description: Some("A **description**.".into()),
                ..Default::default()
            },
            blocks: vec![Block::Section {
                id: ComponentId("selected".into()),
                level: 2,
                role: None,
                title: vec![Inline::Text {
                    value: "Selected section".into(),
                }],
                blocks: vec![Block::Code(CodeBlock {
                    language: Some("rust".into()),
                    value: "  let x = \"<&>\";\n\n".into(),
                    references: CodeReferences::default(),
                    caption: None,
                    highlights: vec![],
                })],
            }],
            relations: vec![],
            provenance: Provenance {
                source: "file:///private/source.org".into(),
                source_format: SourceFormat::Org,
                source_hash: "a".repeat(64),
            },
        }]),
        PublicationOptions {
            source_site_url: Some("https://source.example/notebook".parse().unwrap()),
            limits,
            ..Default::default()
        },
    )
    .unwrap()
}
fn assemble(root: ComponentCall, catalog: PublicationCatalog) -> PublicationAssembly {
    PublicationAssembly::resolve(
        &PublicationSpec {
            schema_version: 1,
            metadata: PublicationMetadata {
                id: PublicationId("example".into()),
                title: "Selected work".into(),
                description: None,
            },
            root,
        },
        catalog,
        &ComponentRegistry::builtins(),
    )
    .unwrap()
}

#[test]
fn e01_e02_e03_e04_all_builtins_render_directly_and_keep_code_literal_text_and_urls() {
    let c = catalog(PublicationLimits::default());
    let reference = ResourceRef::local("source").unwrap();
    let root = Portfolio {}.into_call().with_body(ViewFragment(vec![
        Introduction {
            text: "Editorial <intro> & notes".into(),
        }
        .into_call(),
        Section {
            title: "Work".into(),
        }
        .into_call()
        .with_body(ViewFragment(vec![
            FeaturedWork {
                source: reference.clone(),
                annotation: Some("A personal annotation.".into()),
            }
            .into_call(),
            Excerpt {
                source: c.section_ref("source", "selected").unwrap(),
                annotation: Some("Why this section matters.".into()),
            }
            .into_call(),
        ])),
        Callout {
            kind: CalloutKind::Warning,
            title: Some("Scope".into()),
        }
        .into_call()
        .with_body(
            Introduction {
                text: "A deliberate limit.".into(),
            }
            .into_call(),
        ),
        RelatedWork {
            sources: vec![reference],
            title: Some("Read the original".into()),
        }
        .into_call(),
    ]));
    let assembly = assemble(root, c);
    let result = render(
        &assembly,
        &ComponentRegistry::builtins(),
        &TextConfig::default(),
    )
    .unwrap();
    for text in [
        "Selected work",
        "Editorial <intro> & notes",
        "A source\nhttps://source.example/notebook/notes/a-source.html",
        "A description.",
        "A personal annotation.",
        "Why this section matters.",
        "Selected section",
        "  let x = \"<&>\";\n\n",
        "From A source",
        "[warning] Scope",
        "A deliberate limit.",
        "Read the original",
    ] {
        assert!(
            result.text.contains(text),
            "missing {text:?} in {:?}",
            result.text
        );
    }
    assert!(!result.text.contains("&lt;"));
    assert!(!result.text.contains("file:///"));
    assert_eq!(result.manifest.character_count, result.text.chars().count());
    let manifest = serde_json::to_string(&result.manifest).unwrap();
    assert!(!manifest.contains("private/source"));
    assert_eq!(
        result.manifest.publication.composition_fingerprint,
        assembly.composition_fingerprint()
    );
}

#[test]
fn text_byte_golden_is_deterministic() {
    let c = catalog(PublicationLimits::default());
    let root = Portfolio {}.into_call().with_body(
        Introduction {
            text: "First paragraph.\n\nSecond paragraph.".into(),
        }
        .into_call(),
    );
    let assembly = assemble(root, c);
    let a = render(
        &assembly,
        &ComponentRegistry::builtins(),
        &TextConfig::default(),
    )
    .unwrap();
    let b = render(
        &assembly,
        &ComponentRegistry::builtins(),
        &TextConfig::default(),
    )
    .unwrap();
    assert_eq!(
        a.text,
        "Selected work\n\nFirst paragraph.\n\nSecond paragraph.\n"
    );
    assert_eq!(a.text, b.text);
    assert_eq!(
        serde_json::to_vec(&a.manifest).unwrap(),
        serde_json::to_vec(&b.manifest).unwrap()
    );
}

#[test]
fn e05_linkedin_profile_retains_over_limit_text_and_uses_existing_unicode_policy() {
    let text = "🦀".repeat(3100);
    let assembly = assemble(
        Portfolio {}
            .into_call()
            .with_body(Introduction { text: text.clone() }.into_call()),
        catalog(PublicationLimits::default()),
    );
    let result = render(
        &assembly,
        &ComponentRegistry::builtins(),
        &TextConfig {
            profile: TextProfile::LinkedInDraft,
        },
    )
    .unwrap();
    assert!(result.text.ends_with(&format!("{text}\n")));
    assert_eq!(result.manifest.diagnostics.len(), 1);
    assert_eq!(
        result.manifest.diagnostics,
        berlin_document_linkedin::draft_diagnostics(
            &result.text,
            berlin_document_linkedin::LINKEDIN_POST_CHARACTER_LIMIT
        )
    );
    let plain = render(
        &assembly,
        &ComponentRegistry::builtins(),
        &TextConfig::default(),
    )
    .unwrap();
    assert_eq!(plain.text, result.text);
    assert!(plain.manifest.diagnostics.is_empty());
    assert_ne!(
        plain.manifest.publication.render_input_fingerprint,
        result.manifest.publication.render_input_fingerprint
    );
}

#[test]
fn f17_f19_text_output_is_bounded_and_profile_values_are_closed() {
    let assembly = assemble(
        Portfolio {}.into_call().with_body(
            Introduction {
                text: "Longer than this budget".into(),
            }
            .into_call(),
        ),
        catalog(PublicationLimits {
            output_bytes: 20,
            ..Default::default()
        }),
    );
    assert_eq!(
        render(
            &assembly,
            &ComponentRegistry::builtins(),
            &TextConfig::default()
        )
        .err()
        .unwrap()
        .code,
        Code::ResourceLimit
    );
    assert!(serde_json::from_str::<TextConfig>(r#"{"profile":"remote_publish"}"#).is_err());
    assert!(serde_json::from_str::<TextConfig>(r#"{"profile":"plain","token":"secret"}"#).is_err());
}

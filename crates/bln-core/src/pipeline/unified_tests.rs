use super::*;

fn catalog_node(parts: Vec<ArtifactKind>) -> PipelineNode {
    PipelineNode::new(
        "catalog",
        Operation::AssembleCatalog {
            parts,
            options: Box::default(),
        },
    )
}
fn documents() -> PipelinePlan {
    PipelinePlan::new()
        .with_node(PipelineNode::new(
            "source",
            Operation::LoadMarkdown {
                pattern: "content/*.md".into(),
            },
        ))
        .with_node(PipelineNode::new("documents", Operation::ParseMarkdown).depends_on("source"))
}
#[test]
fn heterogeneous_inputs_check_every_declared_type_and_duplicate_dependency() {
    let valid = documents()
        .with_node(PipelineNode::new(
            "data",
            Operation::LoadData {
                pattern: "data/*.yaml".into(),
            },
        ))
        .with_node(
            PipelineNode::new(
                "resources",
                Operation::ParseResources {
                    namespace: "reading".into(),
                },
            )
            .depends_on("data"),
        )
        .with_node(
            catalog_node(vec![
                ArtifactKind::Documents,
                ArtifactKind::ExternalResources,
            ])
            .depends_on("documents")
            .depends_on("resources"),
        );
    valid.validate().unwrap();
    let wrong = documents()
        .with_node(catalog_node(vec![ArtifactKind::ExternalResources]).depends_on("documents"));
    assert!(matches!(
        wrong.validate(),
        Err(PipelineValidationError::InvalidInputs { .. })
    ));
    let duplicated = documents().with_node(
        catalog_node(vec![ArtifactKind::Documents, ArtifactKind::Documents])
            .depends_on("documents")
            .depends_on("documents"),
    );
    assert!(matches!(
        duplicated.validate(),
        Err(PipelineValidationError::InvalidCatalog { .. })
    ));
    let unavailable =
        documents().with_node(catalog_node(vec![ArtifactKind::Documents]).depends_on("unknown"));
    assert!(matches!(
        unavailable.validate(),
        Err(PipelineValidationError::UnknownDependency { .. })
    ));
}
#[test]
fn catalog_arity_limit_is_checked_at_zero_exact_limit_and_one_over() {
    let mut valid = PipelinePlan::new().with_node(PipelineNode::new(
        "source",
        Operation::LoadMarkdown {
            pattern: "content/*.md".into(),
        },
    ));
    let mut catalog = catalog_node(vec![ArtifactKind::Documents; 64]);
    for n in 0..64 {
        let id = format!("documents-{n}");
        valid = valid.with_node(
            PipelineNode::new(id.clone(), Operation::ParseMarkdown).depends_on("source"),
        );
        catalog = catalog.depends_on(id);
    }
    valid.with_node(catalog).validate().unwrap();
    for count in [0, 65] {
        assert!(matches!(
            PipelinePlan::new()
                .with_node(catalog_node(vec![ArtifactKind::Documents; count]))
                .validate(),
            Err(PipelineValidationError::InvalidCatalog { .. })
        ));
    }
    assert!(matches!(
        PipelinePlan::new()
            .with_node(catalog_node(vec![ArtifactKind::DataSources]))
            .validate(),
        Err(PipelineValidationError::InvalidCatalog { .. })
    ));
}
#[test]
fn graph_cycles_and_asset_mount_conflicts_fail_before_runtime() {
    let cycle = PipelinePlan::new()
        .with_node(
            PipelineNode::new(
                "a",
                Operation::MapDocuments {
                    mapper: FunctionRef::new("map"),
                },
            )
            .depends_on("b"),
        )
        .with_node(
            PipelineNode::new(
                "b",
                Operation::MapDocuments {
                    mapper: FunctionRef::new("map"),
                },
            )
            .depends_on("a"),
        )
        .with_node(catalog_node(vec![ArtifactKind::Documents]).depends_on("a"));
    assert!(matches!(
        cycle.validate(),
        Err(PipelineValidationError::Cycle(_))
    ));
    let config = Box::new(WebsiteConfig {
        url: Some("https://example.test".parse().unwrap()),
        ..Default::default()
    });
    for mounts in [
        vec!["../escape".into()],
        vec!["attachments".into(), "attachments/nested".into()],
        vec!["files".into(); 65],
    ] {
        let plan = PipelinePlan::new().with_node(
            PipelineNode::new(
                "html",
                Operation::RenderHtml {
                    config: config.clone(),
                    asset_mounts: mounts,
                },
            )
            .output_to("_site"),
        );
        assert!(plan.validate().is_err());
    }
}

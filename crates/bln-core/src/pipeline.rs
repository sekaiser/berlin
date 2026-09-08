use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use crate::WebsiteConfig;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<&str> for NodeId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for NodeId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FunctionRef(String);

impl FunctionRef {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    OrgSources,
    MarkdownSources,
    Documents,
    DataSources,
    Feed,
    WebsiteAssembly,
    CssSources,
    Stylesheets,
    StaticAssets,
    Website,
    LinkedInDrafts,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    LoadOrg { pattern: String },
    LoadMarkdown { pattern: String },
    LoadData { pattern: String },
    LoadCss { pattern: String },
    LoadAssets { pattern: String },
    ExportOrg { backend: String, section: String },
    ParseMarkdown,
    ParseFeed,
    MapDocuments { mapper: FunctionRef },
    AssembleWebsite,
    CompileCss,
    CopyAssets,
    RenderWebsite { config: Box<WebsiteConfig> },
    RenderLinkedIn,
}

impl Operation {
    pub fn signature(&self) -> OperationSignature {
        match self {
            Self::LoadOrg { .. } => OperationSignature::source(ArtifactKind::OrgSources),
            Self::LoadMarkdown { .. } => OperationSignature::source(ArtifactKind::MarkdownSources),
            Self::LoadData { .. } => OperationSignature::source(ArtifactKind::DataSources),
            Self::LoadCss { .. } => OperationSignature::source(ArtifactKind::CssSources),
            Self::LoadAssets { .. } => OperationSignature::source(ArtifactKind::StaticAssets),
            Self::ExportOrg { .. } => OperationSignature::artifact_and_output(
                &[ArtifactKind::OrgSources],
                ArtifactKind::MarkdownSources,
            ),
            Self::ParseMarkdown => OperationSignature::artifact(
                &[ArtifactKind::MarkdownSources],
                ArtifactKind::Documents,
            ),
            Self::ParseFeed => {
                OperationSignature::artifact(&[ArtifactKind::DataSources], ArtifactKind::Feed)
            }
            Self::MapDocuments { .. } => {
                OperationSignature::artifact(&[ArtifactKind::Documents], ArtifactKind::Documents)
            }
            Self::AssembleWebsite => OperationSignature::artifact(
                &[ArtifactKind::Documents, ArtifactKind::Feed],
                ArtifactKind::WebsiteAssembly,
            ),
            Self::CompileCss => {
                OperationSignature::output(&[ArtifactKind::CssSources], ArtifactKind::Stylesheets)
            }
            Self::CopyAssets => OperationSignature::output(
                &[ArtifactKind::StaticAssets],
                ArtifactKind::StaticAssets,
            ),
            Self::RenderWebsite { .. } => {
                OperationSignature::output(&[ArtifactKind::WebsiteAssembly], ArtifactKind::Website)
            }
            Self::RenderLinkedIn => {
                OperationSignature::output(&[ArtifactKind::Documents], ArtifactKind::LinkedInDrafts)
            }
        }
    }

    pub fn output_kind(&self) -> ArtifactKind {
        self.signature().output
    }

    fn source_pattern(&self) -> Option<&str> {
        match self {
            Self::LoadOrg { pattern }
            | Self::LoadMarkdown { pattern }
            | Self::LoadData { pattern }
            | Self::LoadCss { pattern }
            | Self::LoadAssets { pattern } => Some(pattern),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationSignature {
    pub inputs: &'static [ArtifactKind],
    pub output: ArtifactKind,
    pub produces_value: bool,
    pub writes_output: bool,
}

impl OperationSignature {
    const fn source(output: ArtifactKind) -> Self {
        Self::artifact(&[], output)
    }

    const fn artifact(inputs: &'static [ArtifactKind], output: ArtifactKind) -> Self {
        Self {
            inputs,
            output,
            produces_value: true,
            writes_output: false,
        }
    }

    const fn artifact_and_output(inputs: &'static [ArtifactKind], output: ArtifactKind) -> Self {
        Self {
            writes_output: true,
            ..Self::artifact(inputs, output)
        }
    }

    const fn output(inputs: &'static [ArtifactKind], output: ArtifactKind) -> Self {
        Self {
            inputs,
            output,
            produces_value: false,
            writes_output: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PipelineNode {
    pub id: NodeId,
    pub operation: Operation,
    #[serde(default)]
    pub dependencies: Vec<NodeId>,
    pub output_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment: Option<crate::DeploymentTarget>,
}

impl PipelineNode {
    pub fn new(id: impl Into<NodeId>, operation: Operation) -> Self {
        Self {
            id: id.into(),
            operation,
            dependencies: Vec::new(),
            output_path: None,
            deployment: None,
        }
    }

    pub fn depends_on(mut self, id: impl Into<NodeId>) -> Self {
        self.dependencies.push(id.into());
        self
    }

    pub fn output_to(mut self, path: impl Into<PathBuf>) -> Self {
        self.output_path = Some(path.into());
        self
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PipelinePlan {
    nodes: Vec<PipelineNode>,
}

pub trait PipelineLoader {
    type Error: std::error::Error;

    fn load(&self, name: &str) -> Result<PipelinePlan, Self::Error>;
}

impl PipelinePlan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_node(mut self, node: PipelineNode) -> Self {
        self.nodes.push(node);
        self
    }

    pub fn nodes(&self) -> &[PipelineNode] {
        &self.nodes
    }

    pub fn validate(&self) -> Result<(), PipelineValidationError> {
        let nodes_by_id = self.validate_nodes()?;
        self.validate_dependencies(&nodes_by_id)?;
        TopologicalTraversal::new(nodes_by_id).order(&self.nodes)?;
        Ok(())
    }

    fn validate_nodes(&self) -> Result<HashMap<NodeId, &PipelineNode>, PipelineValidationError> {
        let mut nodes_by_id = HashMap::with_capacity(self.nodes.len());
        let mut output_paths = HashMap::new();

        for node in &self.nodes {
            if nodes_by_id.insert(node.id.clone(), node).is_some() {
                return Err(PipelineValidationError::DuplicateNode(node.id.clone()));
            }

            validate_source_pattern(node)?;
            validate_output(node, &mut output_paths)?;
            if let Some(target) = &node.deployment {
                let validation = if matches!(node.operation, Operation::RenderWebsite { .. }) {
                    target.validate()
                } else {
                    Err("only website outputs support deployment")
                };
                validation.map_err(|message| PipelineValidationError::InvalidDeployment {
                    node: node.id.clone(),
                    message,
                })?;
            }
        }

        Ok(nodes_by_id)
    }

    fn validate_dependencies(
        &self,
        nodes_by_id: &HashMap<NodeId, &PipelineNode>,
    ) -> Result<(), PipelineValidationError> {
        self.nodes
            .iter()
            .try_for_each(|node| validate_node_inputs(node, nodes_by_id))
    }

    /// Orders dependencies before their consumers.
    ///
    /// Rejects duplicate IDs, unknown dependencies, and cycles. Operation signatures
    /// and path constraints are checked separately by `validate`.
    pub fn topological_order(&self) -> Result<Vec<&PipelineNode>, PipelineValidationError> {
        let mut nodes_by_id = HashMap::with_capacity(self.nodes.len());
        for node in &self.nodes {
            if nodes_by_id.insert(node.id.clone(), node).is_some() {
                return Err(PipelineValidationError::DuplicateNode(node.id.clone()));
            }
        }

        TopologicalTraversal::new(nodes_by_id).order(&self.nodes)
    }
}

fn validate_source_pattern(node: &PipelineNode) -> Result<(), PipelineValidationError> {
    if let Some(pattern) = node.operation.source_pattern() {
        let path = Path::new(pattern);
        if !is_confined_relative_path(path) {
            return Err(PipelineValidationError::UnsafeSourcePattern {
                node: node.id.clone(),
                pattern: pattern.into(),
            });
        }
    }
    Ok(())
}

fn validate_output(
    node: &PipelineNode,
    output_paths: &mut HashMap<PathBuf, NodeId>,
) -> Result<(), PipelineValidationError> {
    let writes_output = node.operation.signature().writes_output;
    match (&node.output_path, writes_output) {
        (Some(path), _) => {
            validate_output_path(node, path, output_paths)?;
            if !writes_output {
                return Err(PipelineValidationError::UnexpectedOutputPath(
                    node.id.clone(),
                ));
            }
        }
        (None, true) => {
            return Err(PipelineValidationError::MissingOutputPath(node.id.clone()));
        }
        (None, false) => {}
    }
    Ok(())
}

fn validate_output_path(
    node: &PipelineNode,
    path: &Path,
    output_paths: &mut HashMap<PathBuf, NodeId>,
) -> Result<(), PipelineValidationError> {
    if !is_confined_relative_path(path) {
        return Err(PipelineValidationError::UnsafeOutputPath {
            node: node.id.clone(),
            path: path.to_owned(),
        });
    }
    if let Some(first) = output_paths.insert(path.to_owned(), node.id.clone()) {
        return Err(PipelineValidationError::DuplicateOutput {
            path: path.to_owned(),
            first,
            second: node.id.clone(),
        });
    }
    Ok(())
}

fn validate_node_inputs(
    node: &PipelineNode,
    nodes_by_id: &HashMap<NodeId, &PipelineNode>,
) -> Result<(), PipelineValidationError> {
    let signature = node.operation.signature();
    if !signature.inputs.is_empty() && node.dependencies.is_empty() {
        return Err(PipelineValidationError::MissingInput(node.id.clone()));
    }

    let input_kinds = node
        .dependencies
        .iter()
        .map(|dependency| dependency_output_kind(node, dependency, nodes_by_id))
        .collect::<Result<Vec<_>, _>>()?;

    if input_kinds != signature.inputs {
        return Err(PipelineValidationError::InvalidInputs {
            node: node.id.clone(),
            expected: signature.inputs.to_vec(),
            actual: input_kinds,
        });
    }
    Ok(())
}

fn dependency_output_kind(
    node: &PipelineNode,
    dependency: &NodeId,
    nodes_by_id: &HashMap<NodeId, &PipelineNode>,
) -> Result<ArtifactKind, PipelineValidationError> {
    let Some(input_node) = nodes_by_id.get(dependency) else {
        return Err(PipelineValidationError::UnknownDependency {
            node: node.id.clone(),
            dependency: dependency.clone(),
        });
    };
    let signature = input_node.operation.signature();
    if !signature.produces_value {
        return Err(PipelineValidationError::UnavailableRuntimeOutput {
            node: node.id.clone(),
            dependency: dependency.clone(),
        });
    }
    Ok(signature.output)
}

fn is_confined_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

/// State for one dependency-first depth-first traversal.
struct TopologicalTraversal<'a> {
    nodes_by_id: HashMap<NodeId, &'a PipelineNode>,
    visiting: HashSet<NodeId>,
    visited: HashSet<NodeId>,
    ordered: Vec<&'a PipelineNode>,
}

impl<'a> TopologicalTraversal<'a> {
    fn new(nodes_by_id: HashMap<NodeId, &'a PipelineNode>) -> Self {
        let ordered = Vec::with_capacity(nodes_by_id.len());
        Self {
            nodes_by_id,
            visiting: HashSet::new(),
            visited: HashSet::new(),
            ordered,
        }
    }

    fn order(
        mut self,
        nodes: &'a [PipelineNode],
    ) -> Result<Vec<&'a PipelineNode>, PipelineValidationError> {
        for node in nodes {
            self.visit(node)?;
        }
        Ok(self.ordered)
    }

    fn visit(&mut self, node: &'a PipelineNode) -> Result<(), PipelineValidationError> {
        if self.visited.contains(&node.id) {
            return Ok(());
        }
        if !self.visiting.insert(node.id.clone()) {
            return Err(PipelineValidationError::Cycle(node.id.clone()));
        }

        for dependency in &node.dependencies {
            let Some(&dependency_node) = self.nodes_by_id.get(dependency) else {
                return Err(PipelineValidationError::UnknownDependency {
                    node: node.id.clone(),
                    dependency: dependency.clone(),
                });
            };
            self.visit(dependency_node)?;
        }

        self.visiting.remove(&node.id);
        self.visited.insert(node.id.clone());
        self.ordered.push(node);
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PipelineValidationError {
    InvalidDeployment {
        node: NodeId,
        message: &'static str,
    },
    DuplicateNode(NodeId),
    DuplicateOutput {
        path: PathBuf,
        first: NodeId,
        second: NodeId,
    },
    UnsafeSourcePattern {
        node: NodeId,
        pattern: String,
    },
    UnsafeOutputPath {
        node: NodeId,
        path: PathBuf,
    },
    MissingOutputPath(NodeId),
    UnexpectedOutputPath(NodeId),
    MissingInput(NodeId),
    UnknownDependency {
        node: NodeId,
        dependency: NodeId,
    },
    UnavailableRuntimeOutput {
        node: NodeId,
        dependency: NodeId,
    },
    InvalidInputs {
        node: NodeId,
        expected: Vec<ArtifactKind>,
        actual: Vec<ArtifactKind>,
    },
    Cycle(NodeId),
}

impl fmt::Display for PipelineValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDeployment { node, message } => {
                write!(f, "pipeline deployment '{node}': {message}")
            }
            Self::DuplicateNode(node) => write!(f, "pipeline node '{node}' is defined twice"),
            Self::DuplicateOutput {
                path,
                first,
                second,
            } => write!(
                f,
                "pipeline nodes '{first}' and '{second}' both write '{}'",
                path.display()
            ),
            Self::UnsafeSourcePattern { node, pattern } => write!(
                f,
                "pipeline source '{node}' must stay within the project root: '{pattern}'"
            ),
            Self::UnsafeOutputPath { node, path } => write!(
                f,
                "pipeline output '{node}' must stay within the project root: '{}'",
                path.display()
            ),
            Self::MissingOutputPath(node) => {
                write!(f, "pipeline node '{node}' must declare an output path")
            }
            Self::UnexpectedOutputPath(node) => {
                write!(f, "pipeline node '{node}' cannot declare an output path")
            }
            Self::MissingInput(node) => write!(f, "pipeline node '{node}' requires input"),
            Self::UnknownDependency { node, dependency } => write!(
                f,
                "pipeline node '{node}' depends on unknown node '{dependency}'"
            ),
            Self::UnavailableRuntimeOutput { node, dependency } => write!(
                f,
                "pipeline node '{node}' depends on effect-only node '{dependency}'"
            ),
            Self::InvalidInputs {
                node,
                expected,
                actual,
            } => write!(
                f,
                "pipeline node '{node}' requires inputs {expected:?}, got {actual:?}"
            ),
            Self::Cycle(node) => write!(f, "pipeline contains a cycle at node '{node}'"),
        }
    }
}

impl Error for PipelineValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn org_site_plan() -> PipelinePlan {
        PipelinePlan::new()
            .with_node(PipelineNode::new(
                "org_sources",
                Operation::LoadOrg {
                    pattern: "content/**/*.org".into(),
                },
            ))
            .with_node(
                PipelineNode::new(
                    "markdown",
                    Operation::ExportOrg {
                        backend: "hugo".into(),
                        section: "notes".into(),
                    },
                )
                .depends_on("org_sources")
                .output_to("content/notes"),
            )
            .with_node(
                PipelineNode::new("documents", Operation::ParseMarkdown).depends_on("markdown"),
            )
            .with_node(PipelineNode::new(
                "feed_data",
                Operation::LoadData {
                    pattern: "data/feed.csv".into(),
                },
            ))
            .with_node(PipelineNode::new("feed", Operation::ParseFeed).depends_on("feed_data"))
            .with_node(
                PipelineNode::new("website_assembly", Operation::AssembleWebsite)
                    .depends_on("documents")
                    .depends_on("feed"),
            )
            .with_node(
                PipelineNode::new(
                    "website",
                    Operation::RenderWebsite {
                        config: Box::default(),
                    },
                )
                .depends_on("website_assembly")
                .output_to("_site"),
            )
    }

    #[test]
    fn validates_org_to_website_pipeline() {
        let plan = org_site_plan();

        assert_eq!(plan.validate(), Ok(()));
        assert_eq!(
            plan.topological_order()
                .unwrap()
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "org_sources",
                "markdown",
                "documents",
                "feed_data",
                "feed",
                "website_assembly",
                "website"
            ]
        );
    }

    #[test]
    fn topological_order_rejects_duplicate_ids_without_full_validation() {
        let plan = PipelinePlan::new()
            .with_node(PipelineNode::new("duplicate", Operation::ParseMarkdown))
            .with_node(PipelineNode::new("duplicate", Operation::ParseMarkdown));
        assert_eq!(
            plan.topological_order(),
            Err(PipelineValidationError::DuplicateNode("duplicate".into()))
        );
    }

    #[test]
    fn topological_order_handles_forward_and_shared_dependencies() {
        let plan = PipelinePlan::new()
            .with_node(PipelineNode::new("left", Operation::ParseMarkdown).depends_on("shared"))
            .with_node(PipelineNode::new("right", Operation::ParseMarkdown).depends_on("shared"))
            .with_node(PipelineNode::new("shared", Operation::ParseMarkdown));
        let ids: Vec<_> = plan
            .topological_order()
            .unwrap()
            .iter()
            .map(|node| node.id.clone())
            .collect();
        assert_eq!(
            ids,
            vec![
                NodeId::from("shared"),
                NodeId::from("left"),
                NodeId::from("right")
            ]
        );
    }

    #[test]
    fn rejects_unknown_dependency() {
        let plan = PipelinePlan::new().with_node(
            PipelineNode::new("documents", Operation::ParseMarkdown).depends_on("missing"),
        );

        assert!(matches!(
            plan.validate(),
            Err(PipelineValidationError::UnknownDependency { .. })
        ));
    }

    #[test]
    fn rejects_artifact_type_mismatch() {
        let plan = PipelinePlan::new()
            .with_node(PipelineNode::new(
                "assets",
                Operation::LoadAssets {
                    pattern: "static/**/*".into(),
                },
            ))
            .with_node(
                PipelineNode::new("documents", Operation::ParseMarkdown).depends_on("assets"),
            );

        assert!(matches!(
            plan.validate(),
            Err(PipelineValidationError::InvalidInputs { .. })
        ));
    }

    #[test]
    fn rejects_incomplete_website_assembly() {
        let plan = PipelinePlan::new()
            .with_node(PipelineNode::new(
                "markdown",
                Operation::LoadMarkdown {
                    pattern: "content/*.md".into(),
                },
            ))
            .with_node(
                PipelineNode::new("documents", Operation::ParseMarkdown).depends_on("markdown"),
            )
            .with_node(
                PipelineNode::new("website_assembly", Operation::AssembleWebsite)
                    .depends_on("documents"),
            );

        assert!(matches!(
            plan.validate(),
            Err(PipelineValidationError::InvalidInputs { .. })
        ));
    }

    #[test]
    fn rejects_dependencies_on_effect_only_operations() {
        let plan = PipelinePlan::new()
            .with_node(PipelineNode::new(
                "css_sources",
                Operation::LoadCss {
                    pattern: "css/main.css".into(),
                },
            ))
            .with_node(
                PipelineNode::new("styles", Operation::CompileCss)
                    .depends_on("css_sources")
                    .output_to("_site/main.css"),
            )
            .with_node(
                PipelineNode::new("invalid", Operation::CompileCss)
                    .depends_on("styles")
                    .output_to("_site/other.css"),
            );

        assert!(matches!(
            plan.validate(),
            Err(PipelineValidationError::UnavailableRuntimeOutput { .. })
        ));
    }

    #[test]
    fn rejects_cycles() {
        let plan = PipelinePlan::new()
            .with_node(
                PipelineNode::new(
                    "first",
                    Operation::MapDocuments {
                        mapper: FunctionRef::new("identity"),
                    },
                )
                .depends_on("second"),
            )
            .with_node(
                PipelineNode::new(
                    "second",
                    Operation::MapDocuments {
                        mapper: FunctionRef::new("identity"),
                    },
                )
                .depends_on("first"),
            );

        assert!(matches!(
            plan.validate(),
            Err(PipelineValidationError::Cycle(_))
        ));
    }

    #[test]
    fn rejects_duplicate_output_paths() {
        let plan = org_site_plan().with_node(
            PipelineNode::new(
                "other_website",
                Operation::RenderWebsite {
                    config: Box::default(),
                },
            )
            .depends_on("website_assembly")
            .output_to("_site"),
        );

        assert!(matches!(
            plan.validate(),
            Err(PipelineValidationError::DuplicateOutput { .. })
        ));
    }

    #[test]
    fn rejects_paths_outside_the_project_capability() {
        let unsafe_source = PipelinePlan::new().with_node(PipelineNode::new(
            "documents",
            Operation::LoadMarkdown {
                pattern: "../private/*.md".into(),
            },
        ));
        assert!(matches!(
            unsafe_source.validate(),
            Err(PipelineValidationError::UnsafeSourcePattern { .. })
        ));

        let unsafe_output = PipelinePlan::new().with_node(
            PipelineNode::new(
                "assets",
                Operation::LoadAssets {
                    pattern: "static/**/*".into(),
                },
            )
            .output_to("/tmp/public"),
        );
        assert!(matches!(
            unsafe_output.validate(),
            Err(PipelineValidationError::UnsafeOutputPath { .. })
        ));
    }
}

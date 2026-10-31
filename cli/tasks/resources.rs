//! Strict offline ingestion from already captured bytes. Source-root resolution
//! and physical file reads remain the caller's responsibility.
use std::{collections::BTreeSet, sync::Arc};

use berlin_publication::{
    Code, Diagnostic, ExternalResource, ExternalResourceCollection, PublicationLimits,
    ResourceLimits, ResourceSource, Result, Stage, input_digest,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use yaml_rust::{
    parser::{Event, Parser},
    scanner::{Scanner, TScalarStyle, TokenType},
};

fn error(detail: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Code::InvalidResource, Stage::Resolution, detail)
}
fn limit(detail: &str) -> Diagnostic {
    Diagnostic::new(Code::ResourceLimit, Stage::Resolution, detail)
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ResourceFormat {
    Yaml,
    Csv,
}

#[derive(Clone, Debug)]
pub(super) struct ResourceInput {
    namespace: String,
    logical_name: String,
    sha256: String,
    text: Arc<str>,
    format: ResourceFormat,
}

impl ResourceInput {
    pub(super) fn capture(
        namespace: &str,
        logical_name: &str,
        bytes: &[u8],
        limits: &ResourceLimits,
    ) -> Result<Self> {
        limits.validate()?;
        if bytes.len() > limits.file_bytes {
            return Err(limit("external resource input byte limit exceeded"));
        }
        let format = match logical_name.rsplit('.').next() {
            Some("yaml" | "yml") => ResourceFormat::Yaml,
            Some("csv") => ResourceFormat::Csv,
            _ => {
                return Err(error(
                    "resource format must be selected by .yaml, .yml or .csv extension",
                ));
            }
        };
        let sha256 = input_digest(bytes);
        ResourceSource {
            namespace: namespace.into(),
            logical_name: logical_name.into(),
            sha256: sha256.clone(),
            record: 1,
        }
        .validate()?;
        let text = std::str::from_utf8(bytes).map_err(|_| error("resource input is not UTF-8"))?;
        Ok(Self {
            namespace: namespace.into(),
            logical_name: logical_name.into(),
            sha256,
            text: Arc::from(text),
            format,
        })
    }
    fn evidence(&self, record: usize) -> ResourceSource {
        ResourceSource {
            namespace: self.namespace.clone(),
            logical_name: self.logical_name.clone(),
            sha256: self.sha256.clone(),
            record,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlResources {
    schema_version: u32,
    resources: Vec<Value>,
}

enum Frame {
    Object(Map<String, Value>, Option<String>),
    Array(Vec<Value>, usize),
}

fn attach(stack: &mut [Frame], root: &mut Option<Value>, value: Value) -> Result<()> {
    match stack.last_mut() {
        Some(Frame::Array(values, maximum)) => {
            if values.len() >= *maximum {
                return Err(limit("resource YAML array count exceeded"));
            }
            values.push(value);
        }
        Some(Frame::Object(values, key)) => {
            if let Some(key) = key.take() {
                if values.len() >= 32 {
                    return Err(limit("resource YAML mapping field count exceeded"));
                }
                values.insert(key, value);
            } else {
                let Value::String(key_value) = value else {
                    return Err(error("YAML mapping keys must be strings"));
                };
                if values.contains_key(&key_value) {
                    return Err(error("duplicate YAML mapping key"));
                }
                *key = Some(key_value);
            }
        }
        None => {
            if root.replace(value).is_some() {
                return Err(error("only one YAML document is permitted"));
            }
        }
    }
    Ok(())
}

// Pull parser events explicitly: reject anchors, aliases, tags and excessive
// nesting before building a recursive value or expanding any shared YAML node.
fn yaml_value(
    input: &ResourceInput,
    limits: &ResourceLimits,
    publication: &PublicationLimits,
) -> Result<Value> {
    // Collection tags are not retained in yaml-rust's sequence/mapping events.
    // Reject them at the token boundary too; aliases must never be expanded.
    let mut scanner = Scanner::new(input.text.chars());
    for token in scanner.by_ref() {
        if matches!(
            token.1,
            TokenType::Tag(..) | TokenType::Anchor(_) | TokenType::Alias(_)
        ) {
            return Err(error(
                "YAML anchors, aliases and explicit tags are not supported",
            ));
        }
    }
    if scanner.get_error().is_some() {
        return Err(error("malformed resource YAML"));
    }
    let mut parser = Parser::new(input.text.chars());
    let mut stack = Vec::<Frame>::new();
    let mut root = None;
    let mut documents = 0usize;
    loop {
        let (event, marker) = parser.next().map_err(|e| {
            error(format!(
                "malformed YAML in {}: line {}, column {}",
                input.logical_name,
                e.marker().line() + 1,
                e.marker().col() + 1
            ))
        })?;
        match event {
            Event::StreamEnd => break,
            Event::DocumentStart => {
                documents += 1;
                if documents > 1 {
                    return Err(error("only one YAML document is permitted"));
                }
            }
            Event::Alias(_)
            | Event::Scalar(_, _, 1.., _)
            | Event::SequenceStart(1..)
            | Event::MappingStart(1..) => {
                return Err(error("YAML anchors and aliases are not supported"));
            }
            Event::Scalar(_, _, _, Some(_)) => {
                return Err(error("explicit YAML tags are not supported"));
            }
            Event::SequenceStart(_) | Event::MappingStart(_) => {
                if stack.len() >= publication.depth {
                    return Err(limit("resource YAML nesting limit exceeded"));
                }
                let field = match stack.last() {
                    Some(Frame::Object(_, Some(key))) => Some(key.as_str()),
                    _ => None,
                };
                let frame = if matches!(event, Event::MappingStart(_)) {
                    Frame::Object(Map::new(), None)
                } else {
                    Frame::Array(
                        Vec::new(),
                        match field {
                            Some("resources") => limits.catalog_items,
                            Some("authors") => limits.authors_per_resource,
                            Some("topics") => limits.topics_per_resource,
                            _ => limits.query_terms,
                        },
                    )
                };
                stack.push(frame);
            }
            Event::SequenceEnd | Event::MappingEnd => {
                let frame = stack
                    .pop()
                    .ok_or_else(|| error("unbalanced resource YAML"))?;
                let value = match frame {
                    Frame::Array(values, _) => Value::Array(values),
                    Frame::Object(values, None) => Value::Object(values),
                    Frame::Object(_, Some(_)) => return Err(error("YAML key has no value")),
                };
                attach(&mut stack, &mut root, value)?;
            }
            Event::Scalar(text, style, _, _) => {
                if text.len() > publication.string_bytes {
                    return Err(limit("resource YAML string byte limit exceeded"));
                }
                let key = matches!(stack.last(), Some(Frame::Object(_, None)));
                let value = if style != TScalarStyle::Plain || key {
                    Value::String(text)
                } else {
                    match text.as_str() {
                        "null" | "Null" | "NULL" | "~" | "" => Value::Null,
                        "true" | "True" | "TRUE" => Value::Bool(true),
                        "false" | "False" | "FALSE" => Value::Bool(false),
                        _ => text
                            .parse::<i64>()
                            .map(|v| Value::Number(v.into()))
                            .unwrap_or(Value::String(text)),
                    }
                };
                attach(&mut stack, &mut root, value).map_err(|mut e| {
                    e.detail = format!("{} at line {}", e.detail, marker.line() + 1).into();
                    e
                })?;
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(error("unclosed resource YAML"));
    }
    root.ok_or_else(|| error("resource YAML document is empty"))
}

fn yaml_records(
    input: &ResourceInput,
    limits: &ResourceLimits,
    publication: &PublicationLimits,
) -> Result<Vec<ExternalResource>> {
    let value = yaml_value(input, limits, publication)?;
    // Check envelope version before records, without accepting unknown envelope fields.
    if value.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(Diagnostic::new(
            Code::UnknownVersion,
            Stage::Resolution,
            "unsupported resource file schema version",
        ));
    }
    let file: YamlResources = serde_json::from_value(value).map_err(|_| {
        error(format!(
            "resource schema violation in {}",
            input.logical_name
        ))
    })?;
    if file.schema_version != 1 {
        return Err(error("unsupported resource file version"));
    }
    file.resources
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            serde_json::from_value(value).map_err(|_| {
                error(format!(
                    "resource YAML schema violation in {} record {}",
                    input.logical_name,
                    index + 1
                ))
            })
        })
        .collect()
}

const REQUIRED: &[&str] = &["schema_version", "id", "kind", "url", "title"];
const OPTIONAL: &[&str] = &[
    "description",
    "authors",
    "topics",
    "publisher",
    "published",
    "added",
    "language",
    "preview_url",
    "preview_alt",
    "license_note",
    "attribution",
    "metadata_note",
];

fn string_array(text: &str, maximum: usize) -> Result<Vec<String>> {
    use serde::de::{DeserializeSeed, SeqAccess, Visitor};
    struct Strings(usize);
    impl<'de> Visitor<'de> for Strings {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a bounded array of strings")
        }
        fn visit_seq<A: SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element::<String>()? {
                if values.len() >= self.0 {
                    return Err(serde::de::Error::custom("resource list count exceeded"));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    impl<'de> DeserializeSeed<'de> for Strings {
        type Value = Vec<String>;
        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            d: D,
        ) -> std::result::Result<Self::Value, D::Error> {
            d.deserialize_seq(self)
        }
    }
    let mut parser = serde_json::Deserializer::from_str(text);
    let values = Strings(maximum)
        .deserialize(&mut parser)
        .map_err(|_| error("CSV authors/topics must be bounded JSON string arrays"))?;
    parser
        .end()
        .map_err(|_| error("trailing data after CSV string array"))?;
    Ok(values)
}

fn csv_records(input: &ResourceInput, limits: &ResourceLimits) -> Result<Vec<ExternalResource>> {
    let mut reader = csv::ReaderBuilder::new().from_reader(input.text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|_| error("invalid resource CSV header"))?
        .clone();
    let mut seen = BTreeSet::new();
    for name in &headers {
        if !(REQUIRED.contains(&name) || OPTIONAL.contains(&name)) || !seen.insert(name) {
            return Err(error("unknown or duplicate resource CSV header"));
        }
    }
    if REQUIRED.iter().any(|name| !seen.contains(name)) {
        return Err(error("missing required resource CSV header"));
    }
    let mut resources = Vec::new();
    for (index, row) in reader.records().enumerate() {
        if resources.len() >= limits.catalog_items {
            return Err(limit("resource CSV record count exceeded"));
        }
        let row = row.map_err(|_| {
            error(format!(
                "malformed resource CSV record {} in {}",
                index + 1,
                input.logical_name
            ))
        })?;
        let mut object = Map::new();
        let mut preview = Map::new();
        for (key, value) in headers.iter().zip(&row) {
            if key == "schema_version" {
                if value != "1" {
                    return Err(Diagnostic::new(
                        Code::UnknownVersion,
                        Stage::Resolution,
                        "unsupported resource CSV schema version",
                    ));
                }
                object.insert(key.into(), Value::Number(1.into()));
            } else if matches!(key, "authors" | "topics") {
                let cap = if key == "authors" {
                    limits.authors_per_resource
                } else {
                    limits.topics_per_resource
                };
                let values = if value.is_empty() {
                    Vec::new()
                } else {
                    string_array(value, cap)?
                };
                object.insert(
                    key.into(),
                    serde_json::to_value(values).map_err(|_| error("invalid CSV string array"))?,
                );
            } else if let Some(key) = key.strip_prefix("preview_") {
                if !value.is_empty() {
                    preview.insert(key.into(), Value::String(value.into()));
                }
            } else if REQUIRED.contains(&key) || !value.is_empty() {
                object.insert(key.into(), Value::String(value.into()));
            }
        }
        if !preview.is_empty() {
            object.insert("preview".into(), Value::Object(preview));
        }
        resources.push(serde_json::from_value(Value::Object(object)).map_err(|_| {
            error(format!(
                "resource CSV schema violation in {} record {}",
                input.logical_name,
                index + 1
            ))
        })?);
    }
    Ok(resources)
}

pub(super) fn parse(
    inputs: &[ResourceInput],
    limits: &ResourceLimits,
    publication: &PublicationLimits,
) -> Result<ExternalResourceCollection> {
    limits.validate()?;
    publication.validate()?;
    if inputs.len() > limits.catalog_items {
        return Err(limit("resource source count exceeded"));
    }
    let mut input_bytes = 0usize;
    for input in inputs {
        input_bytes = input_bytes
            .checked_add(input.text.len())
            .ok_or_else(|| limit("resource input size overflow"))?;
        if input_bytes > limits.descriptor_bytes {
            return Err(limit("combined resource input byte limit exceeded"));
        }
    }
    let mut records = Vec::new();
    for input in inputs {
        let resources = match input.format {
            ResourceFormat::Yaml => yaml_records(input, limits, publication)?,
            ResourceFormat::Csv => csv_records(input, limits)?,
        };
        if resources.len() > limits.catalog_items.saturating_sub(records.len()) {
            return Err(limit("combined external resource count exceeded"));
        }
        for (index, resource) in resources.into_iter().enumerate() {
            records.push((resource, input.evidence(index + 1)));
        }
    }
    ExternalResourceCollection::new(records, limits, publication)?.with_input_sources(
        inputs
            .iter()
            .map(|input| input.evidence(1).input())
            .collect(),
        limits,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn capture(name: &str, text: &[u8]) -> ResourceInput {
        ResourceInput::capture("reading", name, text, &ResourceLimits::default()).unwrap()
    }
    fn read(inputs: &[ResourceInput]) -> Result<ExternalResourceCollection> {
        parse(
            inputs,
            &ResourceLimits::default(),
            &PublicationLimits::default(),
        )
    }
    const YAML: &str = "schema_version: 1\nresources:\n  - id: logging\n    kind: paper\n    url: 'https://example.test/Paper?x=1&y=2#Section'\n    title: ' Logging & <notes> '\n    authors: [Second, First]\n    topics: [storage, Durability]\n    published: '2024-02-29'\n    added: '2026-10-30'\n    description: ' Authored prose. '\n    publisher: Example\n    language: en\n    license_note: Link\n    attribution: Authors\n    metadata_note: Curated\n    preview: {url: 'https://example.test/image.png', alt: 'A diagram'}\n";
    #[test]
    fn b02_maintained_fixture_yaml_and_csv_have_identical_record_revisions() {
        let yaml = read(&[capture(
            "resources.yaml",
            include_bytes!("../../support/fixtures/unified-resources/data/resources.yaml"),
        )])
        .unwrap();
        let csv = read(&[capture(
            "resources.csv",
            include_bytes!("../../support/fixtures/unified-resources/data/resources.csv"),
        )])
        .unwrap();
        assert_eq!(yaml.len(), 4);
        for (a, b) in yaml.records().iter().zip(csv.records()) {
            assert_eq!(a.resource, b.resource);
            assert_eq!(a.revision, b.revision);
            assert_ne!(a.source.sha256, b.source.sha256);
        }
    }
    #[test]
    fn b01_b02_b03_yaml_csv_normalize_identically_with_distinct_consumed_evidence() {
        let yaml = read(&[capture("reading.yaml", YAML.as_bytes())]).unwrap();
        let mut writer = csv::Writer::from_writer(Vec::new());
        writer
            .write_record(REQUIRED.iter().chain(OPTIONAL).copied())
            .unwrap();
        writer
            .write_record([
                "1",
                "logging",
                "paper",
                "https://example.test/Paper?x=1&y=2#Section",
                " Logging & <notes> ",
                " Authored prose. ",
                "[\"Second\",\"First\"]",
                "[\"storage\",\"Durability\"]",
                "Example",
                "2024-02-29",
                "2026-10-30",
                "en",
                "https://example.test/image.png",
                "A diagram",
                "Link",
                "Authors",
                "Curated",
            ])
            .unwrap();
        let csv = read(&[capture("reading.csv", &writer.into_inner().unwrap())]).unwrap();
        let a = &yaml.records()[0];
        let b = &csv.records()[0];
        assert_eq!(a.resource, b.resource);
        assert_eq!(a.revision, b.revision);
        assert_ne!(a.source.sha256, b.source.sha256);
        assert_eq!(a.source.record, 1);
        assert_eq!(a.resource.title, " Logging & <notes> ");
    }
    #[test]
    fn b04_strict_csv_headers_rows_and_utf8() {
        for csv in [
            "",
            "Title,URL\n",
            "schema_version,id,kind,url,title,bad\n",
            "schema_version,id,kind,url,title,title\n",
            "schema_version,id,kind,url,title\n2,a,paper,https://example.test/,A\n",
            "schema_version,id,kind,url,title,topics\n1,a,paper,https://example.test/,A,rust\n",
            "schema_version,id,kind,url,title,preview_url\n1,a,paper,https://example.test/,A,https://example.test/image\n",
        ] {
            assert!(
                read(&[capture("reading.csv", csv.as_bytes())]).is_err(),
                "accepted {csv}"
            );
        }
        assert!(
            ResourceInput::capture("reading", "reading.csv", &[255], &ResourceLimits::default())
                .is_err()
        );
        assert!(
            read(&[capture("empty.csv", b"schema_version,id,kind,url,title\n")])
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn b05_b09_yaml_is_strict_bounded_and_all_or_nothing() {
        for text in [
            "",
            "schema_version: 2\nresources: []",
            "schema_version: 1\nbad: true\nresources: []",
            "schema_version: 1\nschema_version: 1\nresources: []",
            "schema_version: 1\nresources: [",
            "schema_version: 1\nresources: &x []",
            "schema_version: 1\nresources: [*x]",
            "schema_version: 1\nresources: !!seq []",
            "---\nschema_version: 1\nresources: []\n---\nschema_version: 1\nresources: []",
        ] {
            assert!(
                read(&[capture("bad.yaml", text.as_bytes())]).is_err(),
                "accepted {text}"
            );
        }
        let bad = YAML.replace("kind: paper", "kind: unknown");
        assert!(
            read(&[
                capture("good.yaml", YAML.as_bytes()),
                capture("bad.yaml", bad.as_bytes())
            ])
            .is_err()
        );
        let nested = format!(
            "schema_version: 1\nresources: {}0{}",
            "[".repeat(40),
            "]".repeat(40)
        );
        assert_eq!(
            read(&[capture("bad.yaml", nested.as_bytes())])
                .unwrap_err()
                .code,
            Code::ResourceLimit
        );
        assert!(
            read(&[capture("empty.yaml", b"schema_version: 1\nresources: []")])
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn b06_b08_captured_bytes_multiple_files_and_duplicates() {
        let mut bytes = YAML.as_bytes().to_vec();
        let first = capture("one.yaml", &bytes);
        let digest = input_digest(&bytes);
        bytes.fill(b'x');
        let second = capture(
            "two.yaml",
            YAML.replace("id: logging", "id: second").as_bytes(),
        );
        let a = read(&[first.clone(), second.clone()]).unwrap();
        let b = read(&[second, first.clone()]).unwrap();
        assert_eq!(a.records()[0].revision, b.records()[0].revision);
        assert_eq!(a.records()[0].source.sha256, digest);
        assert!(a.records()[0].resource.title.contains("Logging"));
        let err = read(&[
            first.clone(),
            ResourceInput {
                logical_name: "duplicate.yaml".into(),
                ..first
            },
        ])
        .unwrap_err();
        assert_eq!(err.code, Code::DuplicateResource);
        assert!(err.detail.contains("duplicate.yaml"));
    }
    #[test]
    fn b07_data_loader_confines_named_and_project_roots_and_bounds_reads() {
        use std::fs;
        let root = tempfile::tempdir().unwrap();
        let site = root.path().join("site");
        let data = root.path().join("data");
        fs::create_dir(&site).unwrap();
        fs::create_dir(&data).unwrap();
        fs::write(
            site.join("berlin.pipeline.rhai"),
            "const SOURCE_ROOTS = #{reading: \"../data\"};",
        )
        .unwrap();
        fs::write(data.join("reading.yaml"), YAML).unwrap();
        let project = crate::project::Project::new(site.clone(), vec![]);
        let sources = super::super::load_runtime_data(&project, "@reading/*.yaml").unwrap();
        assert_eq!(sources.len(), 1);
        let reference = crate::util::fs::source_reference(&project, &sources[0].path).unwrap();
        assert_eq!(reference.to_str().unwrap(), "@reading/reading.yaml");
        let input = capture("reading/reading.yaml", sources[0].text.as_bytes());
        fs::write(data.join("reading.yaml"), "changed after capture").unwrap();
        assert_eq!(read(&[input]).unwrap().records()[0].resource.id, "logging");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(data.join("reading.yaml"), site.join("escape.yaml"))
                .unwrap();
            assert!(super::super::load_runtime_data(&project, "escape.yaml").is_err());
            fs::write(root.path().join("outside.yaml"), YAML).unwrap();
            std::os::unix::fs::symlink(root.path().join("outside.yaml"), data.join("escape.yaml"))
                .unwrap();
            assert!(super::super::load_runtime_data(&project, "@reading/*.yaml").is_err());
        }
        let file = fs::File::create(site.join("large.yaml")).unwrap();
        file.set_len(ResourceLimits::default().file_bytes as u64 + 1)
            .unwrap();
        assert!(super::super::load_runtime_data(&project, "large.yaml").is_err());
    }

    #[test]
    fn p02_typed_resource_ingestion_executes_from_captured_data() {
        use super::super::{ExecutionOptions, RuntimeArtifact, SourceFile, executor::NodeExecutor};
        use berlin_core::{ArtifactKind, Operation, PipelineNode};
        use std::collections::HashMap;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("reading.yaml");
        let project = crate::project::Project::new(root.path().to_owned(), vec![]);
        let program = berlin_pipeline_dsl::RhaiPipelineLoader::new("").unwrap();
        let node = PipelineNode::new(
            "resources",
            Operation::ParseResources {
                namespace: "reading".into(),
            },
        )
        .depends_on("data");
        let sources = vec![SourceFile {
            path,
            uri: "file:///captured/reading.yaml".into(),
            text: YAML.into(),
        }];
        let artifacts = HashMap::from([("data".into(), RuntimeArtifact::DataSources(sources))]);
        let mut consumed = Vec::new();
        let mut diagnostics = Vec::new();
        let result = NodeExecutor {
            consumed_inputs: &mut consumed,
            project: &project,
            artifacts: &artifacts,
            program: &program,
            options: ExecutionOptions::default(),
            output_root: root.path(),
            diagnostics: &mut diagnostics,
        }
        .execute(&node)
        .unwrap()
        .unwrap();
        assert_eq!(result.kind(), ArtifactKind::ExternalResources);
        let RuntimeArtifact::ExternalResources(collection) = result else {
            panic!("wrong runtime type")
        };
        assert_eq!(collection.records()[0].source.logical_name, "reading.yaml");
        assert_eq!(
            collection.records()[0].source.sha256,
            input_digest(YAML.as_bytes())
        );
        assert_eq!(collection.records()[0].source.namespace, "reading");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn b05_input_and_record_boundaries_are_exact() {
        let mut limits = ResourceLimits {
            file_bytes: YAML.len(),
            catalog_items: 1,
            ..Default::default()
        };
        let input =
            ResourceInput::capture("reading", "reading.yaml", YAML.as_bytes(), &limits).unwrap();
        assert!(
            parse(
                std::slice::from_ref(&input),
                &limits,
                &PublicationLimits::default()
            )
            .is_ok()
        );
        assert!(
            parse(
                &[input.clone(), input],
                &limits,
                &PublicationLimits::default()
            )
            .is_err()
        );
        limits.file_bytes -= 1;
        assert!(
            ResourceInput::capture("reading", "reading.yaml", YAML.as_bytes(), &limits).is_err()
        );
        assert!(
            ResourceInput::capture(
                "reading",
                "reading.txt",
                YAML.as_bytes(),
                &ResourceLimits::default()
            )
            .is_err()
        );
        let empty = capture(
            "empty.yaml",
            format!(
                "schema_version: 1\nresources: []\n{}",
                "# padding\n".repeat(20)
            )
            .as_bytes(),
        );
        let mut limits = ResourceLimits {
            descriptor_bytes: empty.text.len() * 2,
            ..Default::default()
        };
        assert!(
            parse(
                &[empty.clone(), empty.clone()],
                &limits,
                &PublicationLimits::default()
            )
            .is_ok()
        );
        limits.descriptor_bytes -= 1;
        assert!(
            parse(
                &[empty.clone(), empty],
                &limits,
                &PublicationLimits::default()
            )
            .is_err()
        );
    }
}

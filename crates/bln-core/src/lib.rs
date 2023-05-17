pub mod error;
mod graph;
mod media_type;
mod module_specifier;
mod normalize_path;
mod parsed_source;

// Re-exports
pub use anyhow;
pub use parking_lot;
pub use serde;
pub use toml;
pub use url;

pub use module_specifier::resolve_import;
pub use module_specifier::resolve_path;
pub use module_specifier::resolve_url;
pub use module_specifier::resolve_url_or_path;
pub use module_specifier::ModuleResolutionError;
pub use module_specifier::ModuleSpecifier;
pub use module_specifier::DUMMY_SPECIFIER;
pub use normalize_path::normalize_path;

pub use media_type::MediaType;

pub use parsed_source::FrontMatter;
pub use parsed_source::ParsedSource;

pub use graph::Resolutions;
pub use graph::ResolutionsBuilder;

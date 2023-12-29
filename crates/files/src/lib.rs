mod file_info;
mod media_type;
mod module_specifier;
mod normalize_path;
mod path;

pub use module_specifier::resolve_import;
pub use module_specifier::resolve_path;
pub use module_specifier::resolve_url;
pub use module_specifier::resolve_url_or_path;
pub use module_specifier::ModuleResolutionError;
pub use module_specifier::ModuleSpecifier;
pub use module_specifier::DUMMY_SPECIFIER;

pub use media_type::MediaType;

pub use normalize_path::normalize_path;

pub use file_info::find_content_components;
pub use file_info::FileInfo;

pub mod fs;

pub use path::specifier::to_file_path;

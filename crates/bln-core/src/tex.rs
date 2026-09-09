//! Explicit compiler selection; no arbitrary command strings in a pipeline.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TexEngine {
    PdfLatex,
    LuaLatex,
}
impl TexEngine {
    pub fn program(self) -> &'static str {
        match self {
            Self::PdfLatex => "pdflatex",
            Self::LuaLatex => "lualatex",
        }
    }
}
impl std::str::FromStr for TexEngine {
    type Err = &'static str;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pdflatex" => Ok(Self::PdfLatex),
            "lualatex" => Ok(Self::LuaLatex),
            _ => Err("graphics engine must be pdflatex or lualatex"),
        }
    }
}

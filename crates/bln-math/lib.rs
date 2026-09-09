//! A bounded, offline equation renderer, not a general LaTeX document processor.
use pulldown_latex::{Parser, RenderConfig, Storage};

pub use pulldown_latex::config::DisplayMode;

/// Shared presentation rules for native MathML. No scripts, network or web fonts.
pub const STYLESHEET: &str = include_str!("math.css");

const MAX_SOURCE_BYTES: usize = 16 * 1024;
const MAX_EVENTS: usize = 65_536;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("math expression must not be empty")]
    Empty,
    #[error("math expression exceeds Berlin's size or complexity limit")]
    Limit,
    #[error("invalid or unsupported TeX equation: {0}")]
    Invalid(String),
}

/// Rendered equation markup. Only this module constructs it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MathMl(String);

impl MathMl {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

/// Keeps exact source in an escaped annotation; parser errors never become a
/// successful page containing an merror placeholder. Macros are expression-local.
pub fn render(tex: &str, display_mode: DisplayMode) -> Result<MathMl, Error> {
    if tex.trim().is_empty() {
        return Err(Error::Empty);
    }
    if tex.len() > MAX_SOURCE_BYTES {
        return Err(Error::Limit);
    }
    let storage = Storage::new();
    let events = Parser::new(tex, &storage)
        .take(MAX_EVENTS + 1)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| Error::Invalid(error.to_string()))?;
    if events.len() > MAX_EVENTS {
        return Err(Error::Limit);
    }
    // pulldown-latex expects the annotation to be escaped by its caller.
    let annotation = tex
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let mut output = String::new();
    pulldown_latex::push_mathml(
        &mut output,
        events.into_iter().map(Ok::<_, std::convert::Infallible>),
        RenderConfig {
            display_mode,
            annotation: Some(&annotation),
            ..RenderConfig::default()
        },
    )
    .expect("writing MathML to a String cannot fail");
    Ok(MathMl(output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_semantic_mathml_and_preserves_escaped_source() {
        let tex = r"x_i < \frac{a & b}{2}";
        // Ampersands outside an alignment are invalid; source annotation must
        // also be escaped when valid TeX contains literal HTML metacharacters.
        assert!(render(tex, DisplayMode::Inline).is_err());
        let tex = r"x_i < \frac{a}{2}";
        let result = render(tex, DisplayMode::Inline).unwrap();
        assert!(result.as_str().contains("display=\"inline\""));
        assert!(result.as_str().contains("<mfrac>"));
        assert!(
            result
                .as_str()
                .contains("<annotation encoding=\"application/x-tex\">x_i &lt;")
        );
        assert!(
            render(r"\sum_{i=1}^n i", DisplayMode::Block)
                .unwrap()
                .as_str()
                .contains("display=\"block\"")
        );
    }

    #[test]
    fn errors_are_explicit_and_macros_do_not_leak_between_expressions() {
        for tex in [
            "",
            r"\unknownBerlinCommand{x}",
            r"\frac{1}",
            r"\input{secret}",
            r"\label{eq:x}",
        ] {
            assert!(render(tex, DisplayMode::Inline).is_err(), "{tex}");
        }
        assert!(render(&"x".repeat(MAX_SOURCE_BYTES + 1), DisplayMode::Inline).is_err());
        render(r"\def\RR{\mathbb{R}} \RR", DisplayMode::Inline).unwrap();
        render(r"\newcommand{\RR}[0]{\mathbb{R}} \RR", DisplayMode::Inline).unwrap();
        assert!(render(r"\RR", DisplayMode::Inline).is_err());
        assert!(render(r"\def\loop{\loop}\loop", DisplayMode::Inline).is_err());
    }

    #[test]
    fn supports_common_environments_and_escapes_text_and_annotations() {
        for tex in [
            r"\begin{equation}E=mc^2\end{equation}",
            r"\begin{align*}a &= b+c \\ d &= e\end{align*}",
            r"\begin{bmatrix}1 & 2 \\ 3 & 4\end{bmatrix}",
            r"\text{<script>alert(1)</script>} + a \& b",
        ] {
            let html = render(tex, DisplayMode::Block).unwrap();
            assert!(!html.as_str().contains("<script>"));
            assert!(!html.as_str().contains("<merror"));
        }
    }
}

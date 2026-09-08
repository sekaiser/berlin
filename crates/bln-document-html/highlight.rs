//! Stateful syntax highlighting with independently balanced HTML for each line.

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::html::{IncludeBackground, styled_line_to_highlighted_html};
use syntect::parsing::SyntaxSet;

pub(super) struct LineHighlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
}

impl Default for LineHighlighter {
    fn default() -> Self {
        Self {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            theme: ThemeSet::load_defaults()
                .themes
                .remove("InspiredGitHub")
                .expect("bundled theme exists"),
        }
    }
}

impl LineHighlighter {
    pub(super) fn lines(&self, language: Option<&str>, source: &str) -> Vec<String> {
        let syntax = language
            .and_then(|name| self.syntaxes.find_syntax_by_token(name))
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        source
            .split_inclusive('\n')
            .map(|line| {
                highlighter
                    .highlight_line(line, &self.syntaxes)
                    .ok()
                    .and_then(|ranges| {
                        styled_line_to_highlighted_html(&ranges, IncludeBackground::No).ok()
                    })
                    .unwrap_or_else(|| super::code::escape(line))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::LineHighlighter;

    #[test]
    fn multiline_tokens_keep_their_state_and_each_line_has_balanced_markup() {
        let lines = LineHighlighter::default().lines(
            Some("rust"),
            "/* first\nstill a comment */\nlet x = \"<&\";\n",
        );
        assert_eq!(lines.len(), 3);
        assert!(lines[1].contains("font-style:italic"));
        assert!(lines[2].contains("&lt;&amp;"));
        for line in lines {
            assert_eq!(
                line.matches("<span").count(),
                line.matches("</span>").count()
            );
        }
    }
}

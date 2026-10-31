//! Fallible payload sink used by opt-in fragment rendering. Legacy rendering has
//! no byte ceiling; bounded callers observe failure without a String-write panic.
use std::fmt;

#[derive(Default)]
pub(super) struct Output {
    text: String,
    limit: Option<usize>,
    pub failed: bool,
}
impl Output {
    pub fn bounded(limit: usize) -> Self {
        Self {
            limit: Some(limit),
            ..Self::default()
        }
    }
    pub fn fork(&self) -> Self {
        Self {
            limit: self
                .limit
                .map(|limit| limit.saturating_sub(self.text.len())),
            ..Self::default()
        }
    }
    pub fn push_str(&mut self, text: &str) {
        if self.failed
            || self
                .limit
                .is_some_and(|limit| text.len() > limit.saturating_sub(self.text.len()))
        {
            self.failed = true;
            return;
        }
        self.text.push_str(text);
    }
    pub fn push(&mut self, character: char) {
        self.push_str(character.encode_utf8(&mut [0; 4]));
    }
    pub fn into_string(self) -> String {
        self.text
    }
}
impl fmt::Write for Output {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.push_str(text);
        if self.failed { Err(fmt::Error) } else { Ok(()) }
    }
}

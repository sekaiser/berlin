// Portions adapted from Deno.
// Copyright 2018-2023 the Deno authors. All rights reserved. MIT license.
// See cli/THIRD_PARTY_NOTICES.txt in the repository (THIRD_PARTY_NOTICES.txt
// in the CLI package) for the applicable permission and copyright notice.

use std::fmt;
use std::io::Write;
use std::sync::LazyLock;

use termcolor::Ansi;
use termcolor::Color::Ansi256;
use termcolor::Color::Blue;
use termcolor::Color::Red;
use termcolor::ColorSpec;
use termcolor::WriteColor;

static NO_COLOR: LazyLock<bool> = LazyLock::new(|| std::env::var_os("NO_COLOR").is_some());

fn style(text: impl AsRef<str>, spec: ColorSpec) -> impl fmt::Display {
    if *NO_COLOR {
        return text.as_ref().to_owned();
    }

    let mut output = Vec::new();
    let mut writer = Ansi::new(&mut output);
    writer
        .set_color(&spec)
        .expect("writing ANSI color cannot fail");
    writer
        .write_all(text.as_ref().as_bytes())
        .expect("writing to memory cannot fail");
    writer.reset().expect("writing ANSI reset cannot fail");
    String::from_utf8_lossy(&output).into_owned()
}

pub fn red_bold(text: impl AsRef<str>) -> impl fmt::Display {
    let mut spec = ColorSpec::new();
    spec.set_fg(Some(Red)).set_bold(true);
    style(text, spec)
}

pub fn gray(text: impl AsRef<str>) -> impl fmt::Display {
    let mut spec = ColorSpec::new();
    spec.set_fg(Some(Ansi256(245)));
    style(text, spec)
}

pub fn intense_blue(text: impl AsRef<str>) -> impl fmt::Display {
    let mut spec = ColorSpec::new();
    spec.set_fg(Some(Blue)).set_intense(true);
    style(text, spec)
}

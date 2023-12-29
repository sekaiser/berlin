use std::collections::HashMap;

use parser::ParsedSource;

use super::input::Input;

pub type AggregatedSources = HashMap<String, Vec<ParsedSource>>;

pub type SortFn = Box<dyn Fn(&ParsedSource, &ParsedSource) -> std::cmp::Ordering>;

pub type InputAggregate<'a> =
    &'a dyn Fn(&str, &[ParsedSource], Option<SortFn>) -> AggregatedSources;

pub type Inputs<'a> = Vec<Input<'a>>;

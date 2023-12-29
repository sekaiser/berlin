use libs::tera;
use parser::ParsedSource;

use super::{types::AggregatedSources, EventHandler};

#[allow(unused_variables)]
pub enum Param<'a> {
    Single2(&'a str, &'a dyn EventHandler),
    Static(&'static dyn Fn() -> tera::Context),
    Single(&'a str, &'static dyn Fn(&AggregatedSources) -> tera::Value),
    Multiple(Vec<(&'a str, &'static dyn Fn(&AggregatedSources) -> tera::Value)>),
    Custom(&'static dyn Fn(&AggregatedSources) -> tera::Context),
    Bind(&'static dyn Fn(&ParsedSource) -> tera::Context),
}

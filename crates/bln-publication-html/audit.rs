//! Narrow audit of Tera's parsed AST. Tera treats `safe` specially, bypassing a
//! registered filter, so replacing that filter alone cannot enforce escaping.
//! This checks only the selected template dependency closure, not unused legacy
//! templates. It is not a Tera type checker or a second interpreter.
use std::collections::BTreeSet;
use tera::{Tera, ast::*};

pub(super) fn audit(tera: &Tera, root_name: &str) -> std::result::Result<(), String> {
    fn expression(expr: &Expr) -> bool {
        expr.filters
            .iter()
            .any(|f| f.name == "safe" || f.args.values().any(expression))
            || value(&expr.val)
    }
    fn value(expr_value: &ExprVal) -> bool {
        match expr_value {
            ExprVal::Math(e) => expression(&e.lhs) || expression(&e.rhs),
            ExprVal::Logic(e) => expression(&e.lhs) || expression(&e.rhs),
            ExprVal::In(e) => expression(&e.lhs) || expression(&e.rhs),
            ExprVal::Array(v) => v.iter().any(expression),
            ExprVal::FunctionCall(f) => f.args.values().any(expression),
            ExprVal::MacroCall(f) => f.args.values().any(expression),
            ExprVal::Test(t) => t.args.iter().any(expression),
            ExprVal::StringConcat(c) => c.values.iter().any(value),
            _ => false,
        }
    }
    fn nodes(
        tera: &Tera,
        body: &[Node],
        seen: &mut BTreeSet<String>,
    ) -> std::result::Result<(), String> {
        fn reject(unsafe_filter: bool) -> std::result::Result<(), String> {
            if unsafe_filter {
                Err("safe is forbidden in publication templates; use renderer-owned slots".into())
            } else {
                Ok(())
            }
        }
        for node in body {
            match node {
                Node::VariableBlock(_, expr) => reject(expression(expr))?,
                Node::Set(_, set) => reject(expression(&set.value))?,
                Node::MacroDefinition(_, m, _) => {
                    for expr in m.args.values().flatten() {
                        reject(expression(expr))?;
                    }
                    nodes(tera, &m.body, seen)?;
                }
                Node::FilterSection(_, f, _) => {
                    reject(f.filter.name == "safe" || f.filter.args.values().any(expression))?;
                    nodes(tera, &f.body, seen)?;
                }
                Node::Block(_, b, _) => nodes(tera, &b.body, seen)?,
                Node::Forloop(_, f, _) => {
                    reject(expression(&f.container))?;
                    nodes(tera, &f.body, seen)?;
                    if let Some(empty) = &f.empty_body {
                        nodes(tera, empty, seen)?;
                    }
                }
                Node::If(i, _) => {
                    for (_, condition, body) in &i.conditions {
                        reject(expression(condition))?;
                        nodes(tera, body, seen)?;
                    }
                    if let Some((_, body)) = &i.otherwise {
                        nodes(tera, body, seen)?;
                    }
                }
                Node::Extends(_, name) | Node::ImportMacro(_, name, _) => {
                    template(tera, name, seen)?
                }
                Node::Include(_, names, ignore_missing) => {
                    for name in names {
                        if tera.templates.contains_key(name) || !ignore_missing {
                            template(tera, name, seen)?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn template(
        tera: &Tera,
        name: &str,
        seen: &mut BTreeSet<String>,
    ) -> std::result::Result<(), String> {
        if !seen.insert(name.into()) {
            return Ok(());
        }
        let template = tera
            .templates
            .get(name)
            .ok_or_else(|| format!("missing template '{name}'"))?;
        nodes(tera, &template.ast, seen)
    }
    template(tera, root_name, &mut BTreeSet::new())
}

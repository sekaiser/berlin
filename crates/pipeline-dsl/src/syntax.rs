//! Stateful implementation of Berlin's pipeline declaration syntax.
//!
//! This module collects publishable artifacts emitted by `output`, assembles
//! them into plans when a `pipeline` block completes, and validates the plans
//! captured during script evaluation. Operation construction remains in the
//! sibling `operations` module.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use berlin_core::PipelinePlan;
use rhai::AST;
use rhai::Dynamic;
use rhai::Engine;
use rhai::EvalAltResult;
use rhai::EvalContext;
use rhai::Expression;

use super::CompileError;
use super::operations::PipelineTarget;
use super::validate_document_mappers;

#[derive(Clone, Default)]
pub(super) struct PipelineRegistry {
    plans: Rc<RefCell<HashMap<String, PipelinePlan>>>,
    active_targets: Rc<RefCell<Option<Vec<PipelineTarget>>>>,
}

impl PipelineRegistry {
    pub(super) fn validated_plans(
        &self,
        ast: &AST,
    ) -> Result<HashMap<String, PipelinePlan>, CompileError> {
        let plans = self
            .plans
            .try_borrow()
            .map_err(|_| CompileError::RegistryBorrowed)?
            .clone();
        for (name, plan) in &plans {
            plan.validate()
                .map_err(|source| CompileError::InvalidPlan {
                    name: name.clone(),
                    source,
                })?;
            validate_document_mappers(plan, ast)?;
        }
        Ok(plans)
    }

    fn collect_output(
        &self,
        context: &mut EvalContext<'_, '_, '_, '_, '_, '_>,
        inputs: &[Expression<'_>],
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let value = context.eval_expression_tree(&inputs[0])?;
        let target = PipelineTarget::from_dynamic(value).ok_or_else(|| {
            Box::<EvalAltResult>::from("output requires a publishable pipeline artifact")
        })?;
        self.active_targets
            .try_borrow_mut()
            .map_err(|_| {
                Box::<EvalAltResult>::from("pipeline target registry is already borrowed")
            })?
            .as_mut()
            .ok_or_else(|| Box::<EvalAltResult>::from("output can only be used inside a pipeline"))?
            .push(target);
        Ok(Dynamic::UNIT)
    }

    fn define_pipeline(
        &self,
        context: &mut EvalContext<'_, '_, '_, '_, '_, '_>,
        inputs: &[Expression<'_>],
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let name = pipeline_name(&inputs[0]);
        self.begin_pipeline()?;

        if let Err(error) = context.eval_expression_tree(&inputs[1]) {
            self.discard_active_targets()?;
            return Err(error);
        }

        let plan = assemble_plan(self.take_active_targets()?);
        self.insert_plan(name, plan)?;
        Ok(Dynamic::UNIT)
    }

    fn begin_pipeline(&self) -> Result<(), Box<EvalAltResult>> {
        let mut targets = self.active_targets.try_borrow_mut().map_err(|_| {
            Box::<EvalAltResult>::from("pipeline target registry is already borrowed")
        })?;
        if targets.is_some() {
            return Err(Box::<EvalAltResult>::from(
                "pipeline definitions cannot be nested",
            ));
        }
        *targets = Some(Vec::new());
        Ok(())
    }

    fn discard_active_targets(&self) -> Result<(), Box<EvalAltResult>> {
        self.active_targets
            .try_borrow_mut()
            .map_err(|_| {
                Box::<EvalAltResult>::from("pipeline target registry is already borrowed")
            })?
            .take();
        Ok(())
    }

    fn take_active_targets(&self) -> Result<Vec<PipelineTarget>, Box<EvalAltResult>> {
        self.active_targets
            .try_borrow_mut()
            .map_err(|_| {
                Box::<EvalAltResult>::from("pipeline target registry is already borrowed")
            })?
            .take()
            .ok_or_else(|| {
                Box::<EvalAltResult>::from("pipeline target collection was not initialized")
            })
    }

    fn insert_plan(&self, name: String, plan: PipelinePlan) -> Result<(), Box<EvalAltResult>> {
        let previous = self
            .plans
            .try_borrow_mut()
            .map_err(|_| Box::<EvalAltResult>::from("pipeline registry is already borrowed"))?
            .insert(name.clone(), plan);
        if previous.is_some() {
            return Err(Box::<EvalAltResult>::from(format!(
                "pipeline '{name}' is defined twice"
            )));
        }
        Ok(())
    }
}

pub(super) fn register_pipeline_syntax(engine: &mut Engine, registry: &PipelineRegistry) {
    let output_registry = registry.clone();
    engine
        .register_custom_syntax(["output", "$expr$"], false, move |context, inputs| {
            output_registry.collect_output(context, inputs)
        })
        .expect("output custom syntax is valid");

    let pipeline_registry = registry.clone();
    engine
        .register_custom_syntax(
            ["pipeline", "$ident$", "$block$"],
            false,
            move |context, inputs| pipeline_registry.define_pipeline(context, inputs),
        )
        .expect("pipeline custom syntax is valid");
}

fn pipeline_name(expression: &Expression<'_>) -> String {
    expression
        .get_string_value()
        .expect("$ident$ always contains a name")
        .to_string()
}

fn assemble_plan(targets: Vec<PipelineTarget>) -> PipelinePlan {
    let nodes = targets.into_iter().flat_map(PipelineTarget::into_nodes);

    let mut seen = HashMap::new();
    let mut plan = PipelinePlan::new();

    for node in nodes {
        match seen.get(&node.id) {
            None => {
                seen.insert(node.id.clone(), node.clone());
                plan = plan.with_node(node);
            }
            Some(existing) if existing == &node => {}
            Some(_) => {
                plan = plan.with_node(node);
            }
        }
    }

    plan
}

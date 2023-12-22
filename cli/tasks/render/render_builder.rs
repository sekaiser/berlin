use crate::tasks::Input;
use libs::tera;

//  #[derive(Default)]
//  pub struct RenderBuilder {
//     name: &'static str,
//     inputs: &'static [Input<'static>],
//     template: &'static str,
//     maybe_aggregator: Option<Aggregator<'static>>,
//     add_to_context: Vec<&'static dyn Fn() -> tera::Context>,
//     output: &'static str,
// }

// #[allow(dead_code)]
// impl RenderBuilder {
//     pub fn new(name: &'static str, template: &'static str, output: &'static str) -> RenderBuilder {
//         RenderBuilder {
//             name,
//             inputs: &[],
//             template,
//             maybe_aggregator: None,
//             add_to_context: Vec::new(),
//             output,
//         }
//     }

//     pub fn input(mut self, inputs: &'static [Input<'static>]) -> Self {
//         self.inputs = inputs;
//         self
//     }

//     pub fn template_vars(mut self, inputs: Aggregator<'static>) -> Self {
//         self.maybe_aggregator = Some(inputs);
//         self
//     }

//     pub fn add_to_context<I: Fn() -> tera::Context>(mut self, input: &'static I) -> Self {
//         self.add_to_context.push(input);
//         self
//     }

//     pub fn build(self) -> Render<'static> {
//         Render {
//             name: self.name,
//             // inputs: self.inputs.to_vec(),
//             inputs: vec![],
//             template: self.template,
//             maybe_aggregator: self.maybe_aggregator,
//             add_to_context: self.add_to_context,
//             output: self.output,
//         }
//     }
// }

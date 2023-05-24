use errors::anyhow::Error;
use errors::error::generic_error;
use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::path::{Path, PathBuf};

use petgraph::graph::NodeIndex;
use petgraph::visit::Dfs;
use petgraph::Graph;

pub struct Resolutions(Inner);

pub struct Inner {
    graph: Graph<PathBuf, (), petgraph::Directed>,
    root_nodes: Vec<NodeIndex<u32>>,
    node_ids: HashMap<PathBuf, NodeIndex<u32>>,
}

impl Deref for Resolutions {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Resolutions {
    pub fn is_empty(&self) -> bool {
        self.graph.capacity().0 == 0
    }

    pub fn get_root(&self, path: PathBuf) -> Vec<PathBuf> {
        let mut found = HashSet::new();
        let idx = self.node_ids.get(&path).map(|v| *v).unwrap();
        for node in self.root_nodes.clone().into_iter() {
            let mut dfs = Dfs::new(&self.graph, node);

            while let Some(nx) = dfs.next(&self.graph) {
                if self.graph[nx] == self.graph[idx] {
                    found.insert(self.graph[node].clone());
                }
            }
        }
        Vec::from_iter(found)
    }
}

pub struct ResolutionsBuilder {
    //edges: Vec<(PathBuf, Vec<PathBuf>, Box<dyn Fn(&Path, &[&Path]) -> Result<(), String>>)>,
    edges: Vec<(PathBuf, Vec<PathBuf>)>,
}

impl ResolutionsBuilder {
    pub fn new() -> Self {
        Self { edges: Vec::new() }
    }

    pub fn add_rule<P1, P2>(mut self, path: P1, dependencies: &[P2]) -> ResolutionsBuilder
    where
        //F: Fn(&Path, &[&Path]) -> Result<(), String> + 'static,
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        self.edges.push((
            path.as_ref().to_path_buf(),
            dependencies
                .iter()
                .map(|s| s.as_ref().to_path_buf())
                .collect(),
            //Box::new(build_fn),
        ));
        self
    }

    pub fn build(self) -> Result<Resolutions, Error> {
        let mut node_ids = HashMap::new();
        let mut edges_after_node = Vec::with_capacity(self.edges.len());
        let mut graph = Graph::new();

        for edge in self.edges.into_iter() {
            let (path, imports) = edge;

            if node_ids.contains_key(&path) {
                return Err(generic_error("Path already added."));
            }

            let idx = graph.add_node(path.clone());
            node_ids.insert(path, idx);
            edges_after_node.push((idx, imports));
        }

        for edge in edges_after_node.into_iter() {
            let (idx, imports) = edge;

            for import in imports {
                let maybe_import = node_ids.get(&import).map(|v| *v);
                if let Some(idx2) = maybe_import {
                    graph.add_edge(idx, idx2, ());
                } else {
                    let idx2 = graph.add_node(import.clone());
                    node_ids.insert(import, idx2);
                    graph.add_edge(idx, idx2, ());
                }
            }
        }

        if petgraph::algo::is_cyclic_directed(&graph) {
            return Err(generic_error("Cannot construct graph: Cycle detected!"));
        }

        let root_nodes = graph
            .node_indices()
            .filter(|n| graph.neighbors_directed(*n, petgraph::Incoming).count() == 0)
            .collect::<Vec<_>>();

        Ok(Resolutions(Inner {
            graph,
            root_nodes,
            node_ids,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_graph() {
        let resolutions = ResolutionsBuilder::new()
            .add_rule(Path::new("main.css"), &vec![Path::new("a.css")])
            .add_rule(Path::new("style.css"), &vec![Path::new("a.css")])
            .build()
            .unwrap();

        println!("found: {:?}", resolutions.get_root(PathBuf::from("a.css")));
    }
}

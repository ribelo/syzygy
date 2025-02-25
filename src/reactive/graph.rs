use std::{cell::RefCell, fmt, rc::Rc};

use thiserror::Error;

use super::{
    BoxableValue,
    handler::{FromGraph, Reactive, ReactiveHandler, ReactiveWrapper},
    port_id::PortId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    Clean,
    Check,
    Dirty,
    Error,
}

#[derive(Clone)]
pub enum NodeValue {
    Uninitialized,
    Void,
    Memoized(Rc<dyn BoxableValue>),
}

#[derive(Debug)]
pub struct Node<T>(pub Rc<T>);

impl<T: BoxableValue> FromGraph for Node<T> {
    fn from_graph(graph: &Graph) -> Self {
        Self(graph.get_node_value::<T>().unwrap())
    }
    fn deps_ids() -> Vec<PortId> {
        vec![PortId::new::<T>()]
    }
}

#[derive(Clone)]
pub struct GraphNode {
    pub port_id: PortId,
    pub state: NodeState,
    pub sources: Vec<PortId>,
    pub subscribers: Vec<PortId>,
    pub reactive: Option<Rc<dyn Reactive>>,
    pub value: Option<Rc<dyn BoxableValue>>,
}

impl fmt::Debug for GraphNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphNode")
            .field("port_id", &self.port_id)
            .field("state", &self.state)
            .field("sources", &self.sources)
            .field("subscribers", &self.subscribers)
            .field("reactive", &"<reactive>")
            .field("value", &"<value>")
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("Dependency cycle detected in graph")]
    Cycle,
    #[error("Source node not found for port id {0:?}")]
    SourceNotFound(PortId),
    #[error("Port id {0:?} already exists in graph")]
    PortIdExists(PortId),
}

#[derive(Clone, Default)]
pub struct Graph {
    nodes: Rc<RefCell<Vec<GraphNode>>>,
}

impl Graph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn find_node(&self, id: PortId) -> Option<GraphNode> {
        self.nodes
            .borrow()
            .iter()
            .find(|node| node.port_id == id)
            .cloned()
    }

    #[must_use]
    pub fn get_node_state(&self, id: PortId) -> Option<NodeState> {
        self.find_node(id).map(|node| node.state)
    }

    pub fn set_node_state(&self, id: PortId, new_state: NodeState) {
        let mut nodes = self.nodes.borrow_mut();
        if let Some(node) = nodes.iter_mut().find(|node| node.port_id == id) {
            node.state = new_state;
        }
    }

    #[must_use]
    pub fn get_node<T: 'static>(&self) -> Option<Rc<T>>
    where
        T: BoxableValue + Clone,
    {
        let id = PortId::new::<T>();
        self.update_node_if_necessary(id);
        self.get_node_value::<T>()
    }

    fn get_node_value<T>(&self) -> Option<Rc<T>>
    where
        T: BoxableValue + 'static,
    {
        let id = PortId::new::<T>();
        let nodes = self.nodes.borrow();

        nodes
            .iter()
            .find(|node| node.port_id == id)
            .and_then(|node| node.value.as_ref().map(Rc::clone))
            .and_then(|val| val.downcast_rc::<T>().ok())
    }

    pub(crate) fn set_node_value_by_id(&self, port_id: PortId, value: impl BoxableValue + 'static) {
        let mut nodes = self.nodes.borrow_mut();
        if let Some(node) = nodes.iter_mut().find(|node| node.port_id == port_id) {
            node.value = Some(Rc::new(value));
        }
    }

    pub fn set_node_value<T>(&self, value: T)
    where
        T: BoxableValue + 'static,
    {
        let id = PortId::new::<T>();
        let mut nodes = self.nodes.borrow_mut();
        if let Some(node) = nodes.iter_mut().find(|node| node.port_id == id) {
            node.value = Some(Rc::new(value));
        } else {
            // In this design we insert a new node with no dependencies.
            nodes.push(GraphNode {
                port_id: id,
                state: NodeState::Dirty,
                sources: Vec::new(),
                subscribers: Vec::new(),
                reactive: None,
                value: Some(Rc::new(value)),
            });
        }
        drop(nodes);
        self.mark_subs_as_dirty(id);
    }

    pub fn update_node_if_necessary(&self, root_id: PortId) {
        // Return immediately if the root is already clean.
        if self.get_node_state(root_id) == Some(NodeState::Clean) {
            return;
        }

        // Each index in `levels` represents a "distance" (or level) from the root.
        let mut levels: Vec<Vec<PortId>> = Vec::new();
        // Use a stack (LIFO) for traversing the dependency tree.
        let mut queue: Vec<(usize, PortId)> = vec![(0, root_id)];
        // Use a Vec for visited nodes to improve cache locality.
        let mut visited: Vec<PortId> = vec![root_id];

        while let Some((level, id)) = queue.pop() {
            // Ensure our levels vector is large enough.
            if levels.len() <= level {
                levels.resize(level + 1, Vec::new());
            }
            levels[level].push(id);

            // Get the node's sources (dependencies) if available.
            if let Some(node) = self.find_node(id) {
                let next_level = level + 1;
                for source_id in node.sources {
                    // Only process if the source is dirty and not already visited.
                    if self.get_node_state(source_id) == Some(NodeState::Dirty)
                        && !visited.contains(&source_id)
                    {
                        visited.push(source_id);
                        queue.push((next_level, source_id));
                    }
                }
            }
        }

        // Process nodes from the deepest level to the shallowest.
        for level_nodes in levels.into_iter().rev() {
            for id in level_nodes {
                if let Some(node) = self.find_node(id) {
                    if let Some(reactive) = node.reactive {
                        reactive.run(self);
                    }
                }
            }
        }
    }

    pub fn mark_subs_as_dirty(&self, root_id: PortId) {
        let mut stack = Vec::new();
        {
            let nodes = self.nodes.borrow();
            if let Some(node) = nodes.iter().find(|node| node.port_id == root_id) {
                stack.extend(node.subscribers.iter().copied());
            }
        }

        while let Some(id) = stack.pop() {
            {
                let mut nodes = self.nodes.borrow_mut();
                if let Some(node) = nodes.iter_mut().find(|node| node.port_id == id) {
                    node.state = NodeState::Dirty;
                    stack.extend(node.subscribers.iter().copied());
                }
            }
        }
    }

    /// Helper method to check if starting from `start`, we can reach `target`
    /// by traversing upward through the sources.
    fn depends_on(&self, start: PortId, target: PortId) -> bool {
        let mut stack = vec![start];
        let mut visited = vec![start];

        while let Some(current) = stack.pop() {
            if current == target {
                return true;
            }
            // Skip already visited nodes.
            if visited.contains(&current) {
                continue;
            }
            visited.push(current);
            // Traverse the sources of the current node.
            if let Some(node) = self.find_node(current) {
                for s in node.sources {
                    stack.push(s);
                }
            }
        }
        false
    }

    fn reg_reactive(
        &self,
        port_id: PortId,
        sources: Vec<PortId>,
        value: Option<Rc<dyn BoxableValue>>,
        reactive: Rc<dyn Reactive>,
    ) -> Result<(), GraphError> {
        // Check for duplicate port_id.
        {
            let nodes = self.nodes.borrow();
            if nodes.iter().any(|node| node.port_id == port_id) {
                return Err(GraphError::PortIdExists(port_id));
            }
        }

        // Verify that each source node exists.
        for &source_id in &sources {
            let exists = {
                let nodes = self.nodes.borrow();
                nodes.iter().any(|node| node.port_id == source_id)
            };
            if !exists {
                return Err(GraphError::SourceNotFound(source_id));
            }
        }

        // Check for potential dependency cycle.
        // For each source, verify that it does not (directly or indirectly) depend on the new node.
        for &source_id in &sources {
            if self.depends_on(source_id, port_id) {
                return Err(GraphError::Cycle);
            }
        }

        // Create the new node.
        let node = GraphNode {
            port_id,
            state: NodeState::Dirty,
            sources,
            subscribers: Vec::new(),
            reactive: Some(reactive),
            value,
        };

        let mut nodes = self.nodes.borrow_mut();

        // Add subscriber relationships.
        for source_id in &node.sources {
            if let Some(source_node) = nodes.iter_mut().find(|n| n.port_id == *source_id) {
                source_node.subscribers.push(port_id);
            }
        }

        // Register the new node.
        nodes.push(node);
        Ok(())
    }

    pub fn reg_node<T, H, R>(&self, handler: H) -> Result<(), GraphError>
    where
        T: 'static,
        H: ReactiveHandler<T, R> + 'static,
        R: 'static,
    {
        let port_id = PortId::new::<R>();

        // Early check if the port_id already exists
        {
            let nodes = self.nodes.borrow();
            if nodes.iter().any(|node| node.port_id == port_id) {
                return Err(GraphError::PortIdExists(port_id));
            }
        }

        let sources = handler.deps_ids();
        let reactive = Rc::new(ReactiveWrapper::new(handler));
        self.reg_reactive(port_id, sources, None, reactive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_handler_deps() {
        dbg!(PortId::new::<String>());
        dbg!(PortId::new::<Node<String>>());
        let a = |x: Node<String>| -> String { format!("x = {}", x.0) };
        dbg!(a.deps_ids());
        let b = |x: Node<String>| -> String { format!("x = {}", x.0) };
        dbg!(a.deps_ids());
        // dbg!(b.deps_ids());
    }

    #[test]
    fn test_source() {
        let mut graph = Graph::new();
        graph.reg_node(|| 2).unwrap();
        graph.reg_node(|x: Node<i32>| -> String { format!("x = {}", x.0) }).unwrap();
        graph.reg_node(|x: Node<String>| -> String { format!("foobarbaz: {}", x.0) }).unwrap();
        let x = graph.get_node::<i32>();
        let y = graph.get_node::<String>();
        let z = graph.get_node::<String>();
        dbg!(x);
        dbg!(y);
        dbg!(z);
        // dbg!(graph.find_node(PortId::new::<i32>()));
        // dbg!(graph.find_node(PortId::new::<String>()));
    }
}

//! Generic tree-building blocks for Ember+ providers.

use crate::glow::{Access, GlowValue, ParameterInfo as GlowParameterInfo, ParameterType};
use crate::handler::Handler;

/// A parameter in the Ember+ tree.
#[derive(Debug, Clone)]
pub struct Parameter {
    /// The parameter's number within its parent.
    pub number: u32,
    /// The parameter's identifier.
    pub identifier: String,
    /// Optional description.
    pub description: Option<String>,
    /// The parameter's current value.
    pub value: GlowValue,
    /// Whether the parameter is writable.
    pub access: Access,
    /// Parameter type hint.
    pub parameter_type: ParameterType,
}

impl Parameter {
    /// Convert to a Glow parameter info for encoding.
    pub fn to_glow_info(&self) -> GlowParameterInfo {
        GlowParameterInfo {
            identifier: self.identifier.clone(),
            description: self.description.clone(),
            value: self.value.clone(),
            access: self.access,
            parameter_type: self.parameter_type,
        }
    }
}

/// A node in the Ember+ tree.
#[derive(Debug, Clone)]
pub struct Node {
    /// The node's number within its parent.
    pub number: u32,
    /// The node's identifier.
    pub identifier: String,
    /// Optional description.
    pub description: Option<String>,
    /// Child elements.
    pub children: Vec<TreeElement>,
}

/// An element in the Ember+ tree.
#[derive(Debug, Clone)]
pub enum TreeElement {
    /// A node.
    Node(Node),
    /// A parameter.
    Parameter(Parameter),
}

impl TreeElement {
    /// Get the number of this element within its parent.
    pub fn number(&self) -> u32 {
        match self {
            TreeElement::Node(n) => n.number,
            TreeElement::Parameter(p) => p.number,
        }
    }

    /// Get the identifier of this element.
    pub fn identifier(&self) -> &str {
        match self {
            TreeElement::Node(n) => &n.identifier,
            TreeElement::Parameter(p) => &p.identifier,
        }
    }
}

/// A fixed Ember+ tree definition.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    /// Root elements of the tree.
    pub root: Vec<TreeElement>,
}

impl Tree {
    /// Create a new, empty tree.
    pub fn new() -> Self {
        Self { root: Vec::new() }
    }

    /// Add a root element.
    pub fn add(&mut self, element: TreeElement) {
        self.root.push(element);
    }

    /// Find an element by its path.
    pub fn find(&self, path: &[u32]) -> Option<&TreeElement> {
        if path.is_empty() {
            return None;
        }

        let mut current = &self.root;
        let mut element: Option<&TreeElement> = None;

        for (depth, &number) in path.iter().enumerate() {
            element = current.iter().find(|e| e.number() == number);
            if depth + 1 < path.len() {
                match element {
                    Some(TreeElement::Node(node)) => current = &node.children,
                    _ => return None,
                }
            }
        }

        element
    }

    /// Find a mutable parameter by path.
    pub fn find_parameter_mut(&mut self, path: &[u32]) -> Option<&mut Parameter> {
        let element = self.find_mut(path)?;
        match element {
            TreeElement::Parameter(param) => Some(param),
            _ => None,
        }
    }

    fn find_mut(&mut self, path: &[u32]) -> Option<&mut TreeElement> {
        if path.is_empty() {
            return None;
        }

        let mut current: *mut Vec<TreeElement> = &mut self.root;
        let mut element: Option<&mut TreeElement> = None;

        for (depth, &number) in path.iter().enumerate() {
            element = unsafe { (&mut *current).iter_mut().find(|e| e.number() == number) };
            if depth + 1 < path.len() {
                match element {
                    Some(TreeElement::Node(node)) => current = &mut node.children,
                    _ => return None,
                }
            }
        }

        element
    }
}

/// Helper to build a cart-style provider tree.
pub struct CartTreeBuilder {
    root_node: Node,
}

impl CartTreeBuilder {
    /// Create a new cart tree builder with the given root identifier and number.
    pub fn new(root_identifier: impl Into<String>, root_number: u32) -> Self {
        Self {
            root_node: Node {
                number: root_number,
                identifier: root_identifier.into(),
                description: None,
                children: Vec::new(),
            },
        }
    }

    /// Add a cart slot with trigger and status parameters.
    pub fn add_cart(
        &mut self,
        number: u32,
        identifier: impl Into<String>,
        name: impl Into<String>,
    ) {
        let mut cart = Node {
            number,
            identifier: identifier.into(),
            description: None,
            children: Vec::new(),
        };

        cart.children.push(TreeElement::Parameter(Parameter {
            number: 1,
            identifier: "Name".to_string(),
            description: Some("Cart name".to_string()),
            value: GlowValue::String(name.into()),
            access: Access::Read,
            parameter_type: ParameterType::String,
        }));

        cart.children.push(TreeElement::Parameter(Parameter {
            number: 2,
            identifier: "Trigger".to_string(),
            description: Some("Pulse to trigger".to_string()),
            value: GlowValue::Boolean(false),
            access: Access::ReadWrite,
            parameter_type: ParameterType::Boolean,
        }));

        cart.children.push(TreeElement::Parameter(Parameter {
            number: 3,
            identifier: "Status".to_string(),
            description: Some("Playback status".to_string()),
            value: GlowValue::Integer(0),
            access: Access::Read,
            parameter_type: ParameterType::Integer,
        }));

        self.root_node.children.push(TreeElement::Node(cart));
    }

    /// Add a global stop-all parameter.
    pub fn add_global_stop(
        &mut self,
        number: u32,
    ) {
        self.root_node.children.push(TreeElement::Parameter(Parameter {
            number,
            identifier: "StopAll".to_string(),
            description: Some("Pulse to stop all".to_string()),
            value: GlowValue::Boolean(false),
            access: Access::ReadWrite,
            parameter_type: ParameterType::Boolean,
        }));
    }

    /// Add a global now-playing parameter.
    pub fn add_global_now_playing(
        &mut self,
        number: u32,
    ) {
        self.root_node.children.push(TreeElement::Parameter(Parameter {
            number,
            identifier: "NowPlaying".to_string(),
            description: Some("Currently playing cart".to_string()),
            value: GlowValue::String(String::new()),
            access: Access::Read,
            parameter_type: ParameterType::String,
        }));
    }

    /// Build the tree.
    pub fn build(self) -> Tree {
        Tree {
            root: vec![TreeElement::Node(self.root_node)],
        }
    }
}

/// Update parameter values in a tree from a handler.
pub fn refresh_tree_values(tree: &mut Tree, handler: &dyn Handler) {
    refresh_elements(&mut tree.root, &[], handler);
}

fn refresh_elements(elements: &mut [TreeElement], path: &[u32], handler: &dyn Handler) {
    for element in elements {
        match element {
            TreeElement::Node(node) => {
                let mut child_path = path.to_vec();
                child_path.push(node.number);
                refresh_elements(&mut node.children, &child_path, handler);
            }
            TreeElement::Parameter(param) => {
                let mut param_path = path.to_vec();
                param_path.push(param.number);
                if let Some(value) = handler.get_value(&param_path) {
                    param.value = value;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_element_by_path() {
        let mut tree = Tree::new();
        let mut root = Node {
            number: 1,
            identifier: "Root".to_string(),
            description: None,
            children: Vec::new(),
        };
        root.children.push(TreeElement::Parameter(Parameter {
            number: 2,
            identifier: "Param".to_string(),
            description: None,
            value: GlowValue::Integer(42),
            access: Access::Read,
            parameter_type: ParameterType::Integer,
        }));
        tree.add(TreeElement::Node(root));

        assert!(tree.find(&[1]).is_some());
        assert!(tree.find(&[1, 2]).is_some());
        assert!(tree.find(&[1, 3]).is_none());
    }

    #[test]
    fn cart_tree_builder() {
        let mut builder = CartTreeBuilder::new("Callie", 1);
        builder.add_cart(10, "Cart 1", "Intro");
        builder.add_global_stop(100);
        let tree = builder.build();

        assert!(tree.find(&[1, 10, 2]).is_some());
        assert!(tree.find(&[1, 100]).is_some());
    }
}

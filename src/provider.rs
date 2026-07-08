//! Ember+ provider state machine.

use crate::glow::{self, Access, Command as GlowCommand, GlowValue, NodeInfo};
use crate::handler::Handler;
use crate::tree::{Tree, TreeElement};
use crate::{Error, Result};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// Shared provider state across all connections.
pub struct Provider {
    tree: Mutex<Tree>,
    handler: Arc<dyn Handler>,
}

impl Provider {
    /// Create a new provider with the given tree and handler.
    pub fn new(tree: Tree, handler: Arc<dyn Handler>) -> Self {
        Self {
            tree: Mutex::new(tree),
            handler,
        }
    }

    /// Create a new session for an individual consumer connection.
    pub fn session(self: &Arc<Self>) -> ProviderSession {
        ProviderSession {
            provider: Arc::clone(self),
            subscriptions: HashSet::new(),
        }
    }

    /// Update tree values from the handler and broadcast changes to all sessions.
    ///
    /// In a real implementation this would be hooked into the application event loop.
    /// For now, sessions are responsible for re-reading values when needed.
    pub fn refresh(&self) -> Result<()> {
        let mut tree = self.tree.lock().map_err(|_| Error::Glow("tree mutex poisoned".into()))?;
        crate::tree::refresh_tree_values(&mut tree, self.handler.as_ref());
        Ok(())
    }
}

/// A single consumer connection session.
pub struct ProviderSession {
    provider: Arc<Provider>,
    subscriptions: HashSet<Vec<u32>>,
}

impl ProviderSession {
    /// Handle a decoded Glow command and return zero or more S101-framed responses.
    pub fn handle_command(&mut self,
        command: &GlowCommand,
    ) -> Result<Vec<Vec<u8>>> {
        match command {
            GlowCommand::GetDirectory { path, dir_field_mask } => {
                self.handle_get_directory(path, *dir_field_mask)
            }
            GlowCommand::Subscribe { path } => self.handle_subscribe(path),
            GlowCommand::Unsubscribe { path } => self.handle_unsubscribe(path),
            GlowCommand::SetValue { path, value } => self.handle_set_value(path, value),
            GlowCommand::Invoke { path, .. } => self.handle_invoke(path),
            GlowCommand::Other => Ok(Vec::new()),
        }
    }

    fn handle_get_directory(
        &mut self,
        path: &[u32],
        _dir_field_mask: Option<i32>,
    ) -> Result<Vec<Vec<u8>>> {
        let tree = self.provider.tree.lock().map_err(|_| Error::Glow("tree mutex poisoned".into()))?;
        let mut responses = Vec::new();

        if path.is_empty() {
            // Return root node.
            if let Some(TreeElement::Node(root)) = tree.root.first() {
                responses.push(glow::encode_qualified_node(
                    path,
                    &NodeInfo {
                        identifier: root.identifier.clone(),
                        description: root.description.clone(),
                    },
                )?);
            }
        }

        let children = match tree.find(path) {
            Some(TreeElement::Node(node)) => &node.children,
            Some(TreeElement::Parameter(_)) => {
                // Single parameter: send it back.
                return self.encode_element(path);
            }
            None => return Ok(responses),
        };

        for child in children {
            let mut child_path = path.to_vec();
            child_path.push(child.number());
            responses.extend(self.encode_element(&child_path)?);
        }

        Ok(responses)
    }

    fn handle_subscribe(&mut self,
        path: &[u32],
    ) -> Result<Vec<Vec<u8>>> {
        self.subscriptions.insert(path.to_vec());
        self.encode_element(path)
    }

    fn handle_unsubscribe(&mut self,
        path: &[u32],
    ) -> Result<Vec<Vec<u8>>> {
        self.subscriptions.remove(path);
        Ok(Vec::new())
    }

    fn handle_set_value(
        &mut self,
        path: &[u32],
        value: &GlowValue,
    ) -> Result<Vec<Vec<u8>>> {
        // Update the handler.
        self.provider.handler.on_value_change(path, value)?;

        // If this is a pulse parameter (boolean write), reset it to false.
        let mut responses = Vec::new();
        {
            let tree = self.provider.tree.lock().map_err(|_| Error::Glow("tree mutex poisoned".into()))?;
            if let Some(TreeElement::Parameter(param)) = tree.find(path) {
                if param.access == Access::ReadWrite && matches!(value, GlowValue::Boolean(true)) {
                    // Notify that the parameter is back to false.
                    drop(tree);
                    responses.extend(self.encode_element(path)?);
                }
            }
        }

        Ok(responses)
    }

    fn handle_invoke(
        &mut self,
        path: &[u32],
    ) -> Result<Vec<Vec<u8>>> {
        self.provider.handler.on_invoke(path, &[])?;
        Ok(Vec::new())
    }

    fn encode_element(&self,
        path: &[u32],
    ) -> Result<Vec<Vec<u8>>> {
        let tree = self.provider.tree.lock().map_err(|_| Error::Glow("tree mutex poisoned".into()))?;
        let mut responses = Vec::new();

        match tree.find(path) {
            Some(TreeElement::Node(node)) => {
                responses.push(glow::encode_qualified_node(
                    path,
                    &NodeInfo {
                        identifier: node.identifier.clone(),
                        description: node.description.clone(),
                    },
                )?);
            }
            Some(TreeElement::Parameter(param)) => {
                responses.push(glow::encode_qualified_parameter(path, &param.to_glow_info())?);
            }
            None => {}
        }

        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glow::{encode_get_directory_command, GlowValue};
    use crate::handler::Handler;
    use crate::s101::FrameDecoder;
    use crate::tree::CartTreeBuilder;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestHandler {
        triggers: AtomicUsize,
    }

    impl Handler for TestHandler {
        fn on_value_change(
            &self,
            _path: &[u32],
            value: &GlowValue,
        ) -> crate::Result<()> {
            if matches!(value, GlowValue::Boolean(true)) {
                self.triggers.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }
    }

    #[test]
    fn get_directory_returns_qualified_elements() {
        let mut builder = CartTreeBuilder::new("Test", 1);
        builder.add_cart(10, "Cart 1", "Intro");
        let tree = builder.build();

        let handler = Arc::new(TestHandler {
            triggers: AtomicUsize::new(0),
        });
        let provider = Arc::new(Provider::new(tree, handler));
        let mut session = provider.session();

        // Decode a GetDirectory command for the root node.
        let command_bytes = encode_get_directory_command(&[]).unwrap();
        let mut decoder = FrameDecoder::new();
        decoder.feed(&command_bytes);
        let frame = decoder.decode_next().unwrap().unwrap();

        let commands = glow::decode_glow_payload(&frame.payload).unwrap();
        assert_eq!(commands.len(), 1);

        let responses = session.handle_command(&commands[0]).unwrap();
        assert!(!responses.is_empty(), "GetDirectory should return responses");

        // Each response should be a valid S101 frame starting with BOF.
        for response in &responses {
            assert_eq!(response[0], crate::s101::BOF);
        }
    }
}


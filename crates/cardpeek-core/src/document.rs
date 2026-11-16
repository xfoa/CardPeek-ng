use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ByteString, CoreError, DocumentId, NodeId, RunId, TraceRecordId};

/// Current native result document schema version.
pub const DOCUMENT_SCHEMA_VERSION: u32 = 1;

/// A value stored in a result node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum NodeValue {
    /// Text remains distinct from binary data.
    Text(String),
    /// Binary data retains its element width.
    Bytes(ByteString),
}

/// A typed link from a result node to a source trace record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Referenced trace record.
    pub record: TraceRecordId,
    /// Optional human-readable source operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
}

/// One ordered node in a result document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultNode {
    /// Stable run-scoped identifier.
    pub id: NodeId,
    /// Parent identifier, absent for top-level nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<NodeId>,
    /// Analyser-defined class.
    pub class: String,
    /// Display label.
    pub label: String,
    /// Analyser-defined identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    /// Optional exact size in bits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bits: Option<u64>,
    /// Primary value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<NodeValue>,
    /// Alternative display value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alternative: Option<String>,
    /// MIME type of the primary value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Optional source exchange or reset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    /// Ordered child nodes.
    #[serde(default)]
    pub children: Vec<ResultNode>,
    /// Extension attributes in deterministic name order.
    #[serde(default)]
    pub extensions: BTreeMap<String, NodeValue>,
}

impl ResultNode {
    /// Creates a node with no value or children.
    #[must_use]
    pub fn new(id: NodeId, class: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id,
            parent: None,
            class: class.into(),
            label: label.into(),
            identifier: None,
            size_bits: None,
            value: None,
            alternative: None,
            mime_type: None,
            provenance: None,
            children: Vec::new(),
            extensions: BTreeMap::new(),
        }
    }

    /// Sets the primary value and derives its bit width.
    pub fn set_value(&mut self, value: NodeValue) {
        self.size_bits = match &value {
            NodeValue::Text(text) => Some((text.len() * 8) as u64),
            NodeValue::Bytes(bytes) => Some(bytes.bit_len() as u64),
        };
        self.value = Some(value);
    }

    /// Appends a child and sets its parent relationship.
    pub fn append(&mut self, mut child: Self) {
        child.parent = Some(self.id.clone());
        self.children.push(child);
    }
}

/// A UI-independent result document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultDocument {
    /// Native schema version.
    pub schema_version: u32,
    /// Stable document identifier.
    pub id: DocumentId,
    /// Originating analyser run.
    pub run_id: RunId,
    /// Monotonic document revision.
    pub revision: u64,
    /// Ordered top-level nodes.
    pub roots: Vec<ResultNode>,
}

impl ResultDocument {
    /// Creates an empty document at revision zero.
    #[must_use]
    pub const fn new(id: DocumentId, run_id: RunId) -> Self {
        Self {
            schema_version: DOCUMENT_SCHEMA_VERSION,
            id,
            run_id,
            revision: 0,
            roots: Vec::new(),
        }
    }

    /// Increments the revision after a document mutation.
    pub const fn publish(&mut self) {
        self.revision += 1;
    }

    /// Returns the number of nodes, including descendants.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.roots.iter().map(count_nodes).sum()
    }

    /// Finds a node by its stable identifier.
    #[must_use]
    pub fn find(&self, id: &NodeId) -> Option<&ResultNode> {
        self.roots.iter().find_map(|node| find_node(node, id))
    }

    /// Validates schema and tree invariants.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported schema or invalid node relationships.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.schema_version != DOCUMENT_SCHEMA_VERSION {
            return Err(CoreError::InvalidDocument(format!(
                "unsupported schema version {}",
                self.schema_version
            )));
        }
        let mut ids = Vec::new();
        for root in &self.roots {
            validate_node(root, None, &mut ids)?;
        }
        Ok(())
    }

    /// Encodes the document using the versioned native JSON schema.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid document or a serialisation failure.
    pub fn to_json(&self) -> Result<String, CoreError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Decodes and validates a native JSON document.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed JSON or an invalid document.
    pub fn from_json(input: &str) -> Result<Self, CoreError> {
        let document: Self = serde_json::from_str(input)?;
        document.validate()?;
        Ok(document)
    }
}

fn count_nodes(node: &ResultNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

fn find_node<'a>(node: &'a ResultNode, id: &NodeId) -> Option<&'a ResultNode> {
    if &node.id == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| find_node(child, id))
}

fn validate_node(
    node: &ResultNode,
    parent: Option<&NodeId>,
    ids: &mut Vec<NodeId>,
) -> Result<(), CoreError> {
    if ids.contains(&node.id) {
        return Err(CoreError::InvalidDocument(format!(
            "duplicate node identifier {}",
            node.id
        )));
    }
    if node.parent != parent.cloned() {
        return Err(CoreError::InvalidDocument(format!(
            "incorrect parent for node {}",
            node.id
        )));
    }
    ids.push(node.id.clone());
    for child in &node.children {
        validate_node(child, Some(&node.id), ids)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_preserves_order_and_round_trips() {
        let mut document = ResultDocument::new(
            DocumentId::new("doc-1").unwrap(),
            RunId::new("run-1").unwrap(),
        );
        let mut root = ResultNode::new(NodeId::new("node-1").unwrap(), "card", "Card");
        let mut child = ResultNode::new(NodeId::new("node-2").unwrap(), "atr", "ATR");
        child.set_value(NodeValue::Bytes(ByteString::from_bytes(vec![0x3b, 0])));
        root.append(child);
        document.roots.push(root);
        document.publish();
        let encoded = document.to_json().unwrap();
        let decoded = ResultDocument::from_json(&encoded).unwrap();
        assert_eq!(decoded, document);
        assert_eq!(decoded.node_count(), 2);
    }
}

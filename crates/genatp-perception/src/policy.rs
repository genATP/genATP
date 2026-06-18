//! Perception mode policy engine — decides between DOM-first and vision-fallback.
//!
//! ## Decision tree (from ARCHITECTURE.md)
//!
//! ```text
//! Page loaded or DOM mutation
//!   → A11y tree has named elements?
//!     • YES → Build DOM snapshot
//!       → Snapshot stable across 500ms?
//!         • YES → Emit compressed diff (~400-800 tokens)  [DOM mode]
//!         • NO  → Fall through to canvas/shadow-DOM check
//!     • NO / SPARSE → Canvas or shadow-DOM heavy?
//!       • YES → Send screenshot to vision model (~12k tokens)  [Vision mode]
//!       • NO  → Wait + retry DOM up to 3x
//!         → still bad → Vision mode
//! ```
//!
//! The policy is stateless and pure — all state (previous snapshot, retry
//! count) lives in the caller (orchestrator).

use crate::raw::RawTree;
use crate::strip::{StripPolicy, strip};

/// Which perception mode the engine should use for the current snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PerceptionMode {
    /// Standard DOM-based accessibility tree compression and diffing.
    /// This is the cost-optimal path (~400–800 tokens).
    Dom,
    /// Screenshot-based vision fallback for canvas, shadow-DOM, or
    /// otherwise inaccessible pages (~12k tokens).
    Vision,
}

/// Outcome of a policy decision, including the reasoning for logging.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyDecision {
    /// The chosen perception mode.
    pub mode: PerceptionMode,
    /// Human-readable reason for the decision (for event log / debugging).
    pub reason: String,
    /// Number of named interactive elements found in the a11y tree.
    pub named_element_count: usize,
    /// Total nodes in the a11y tree after stripping.
    pub stripped_node_count: usize,
}

/// Configuration for the policy engine's thresholds.
#[derive(Debug, Clone, Copy)]
pub struct PolicyConfig {
    /// Minimum number of named interactive elements required to consider the
    /// a11y tree "well-populated". Trees below this threshold trigger vision
    /// fallback (after retries).
    pub min_named_interactive: usize,

    /// Minimum total stripped nodes to consider DOM mode viable. A tree with
    /// very few nodes (e.g. `<canvas>` only) triggers vision fallback.
    pub min_stripped_nodes: usize,

    /// Threshold for "sparse" — if the stripped node count is below this
    /// fraction of typical page size (estimated at 20 nodes), we consider
    /// the tree sparse and flag for vision fallback.
    pub sparse_fraction: f64,

    /// Maximum DOM retry attempts before falling back to vision.
    pub max_dom_retries: u32,

    /// Roles considered "interactive" for counting named elements.
    pub interactive_roles: &'static [&'static str],
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            min_named_interactive: 2,
            min_stripped_nodes: 3,
            sparse_fraction: 0.3,
            max_dom_retries: 3,
            interactive_roles: &[
                "button",
                "link",
                "textbox",
                "checkbox",
                "radio",
                "combobox",
                "menuitem",
                "tab",
                "switch",
                "searchbox",
                "slider",
                "spinbutton",
                "listbox",
                "treeitem",
            ],
        }
    }
}

/// The perception policy engine.
///
/// Pure decision-making — no I/O, no mutable state. The orchestrator calls
/// [`decide`] on every snapshot cycle and uses the returned [`PerceptionMode`]
/// to route between DOM compression and vision capture.
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    config: PolicyConfig,
}

impl PolicyEngine {
    /// Create a new policy engine with the given configuration.
    #[must_use]
    pub fn new(config: PolicyConfig) -> Self {
        Self { config }
    }

    /// Decide the perception mode for the given raw tree.
    ///
    /// This is the primary entry point. Call it once per snapshot cycle.
    /// The `retry_count` parameter is the number of times DOM mode has
    /// been retried without success (0 = first attempt).
    ///
    /// Returns a [`PolicyDecision`] with the chosen mode and reasoning.
    #[must_use]
    pub fn decide(&self, tree: &RawTree, retry_count: u32) -> PolicyDecision {
        let stripped = strip(tree, &StripPolicy::default());
        let stripped_count = stripped.nodes.len();

        let named_interactive = self.count_named_interactive(&stripped);

        // Decision tree from ARCHITECTURE.md
        if self.has_sufficient_dom(&stripped, named_interactive) {
            PolicyDecision {
                mode: PerceptionMode::Dom,
                reason: format!(
                    "a11y tree has {} named interactive elements across {} stripped nodes — using DOM mode",
                    named_interactive, stripped_count,
                ),
                named_element_count: named_interactive,
                stripped_node_count: stripped_count,
            }
        } else if self.is_canvas_or_shadow_dom(&stripped) {
            PolicyDecision {
                mode: PerceptionMode::Vision,
                reason: format!(
                    "a11y tree is sparse ({} stripped nodes, {} named interactive) and has canvas/shadow-DOM indicators — using vision mode",
                    stripped_count, named_interactive,
                ),
                named_element_count: named_interactive,
                stripped_node_count: stripped_count,
            }
        } else if retry_count < self.config.max_dom_retries {
            // Wait + retry DOM up to 3x
            PolicyDecision {
                mode: PerceptionMode::Dom,
                reason: format!(
                    "a11y tree is sparse ({} stripped nodes, {} named interactive) — retrying DOM (attempt {}/{})",
                    stripped_count,
                    named_interactive,
                    retry_count + 1,
                    self.config.max_dom_retries,
                ),
                named_element_count: named_interactive,
                stripped_node_count: stripped_count,
            }
        } else {
            // Exhausted retries — fall back to vision
            PolicyDecision {
                mode: PerceptionMode::Vision,
                reason: format!(
                    "a11y tree still sparse after {} DOM retries ({} stripped nodes, {} named interactive) — falling back to vision",
                    self.config.max_dom_retries, stripped_count, named_interactive,
                ),
                named_element_count: named_interactive,
                stripped_node_count: stripped_count,
            }
        }
    }

    /// Check whether the a11y tree has enough content for DOM mode.
    fn has_sufficient_dom(&self, stripped: &RawTree, named_interactive: usize) -> bool {
        let node_count = stripped.nodes.len();

        // Must have at least the minimum number of nodes.
        if node_count < self.config.min_stripped_nodes {
            return false;
        }

        // Must have at least the minimum number of named interactive elements.
        if named_interactive < self.config.min_named_interactive {
            return false;
        }

        true
    }

    /// Check whether the tree looks like a canvas-based page.
    /// Canvas pages have a structurally empty a11y tree because all
    /// content is rendered inside a `<canvas>` element which has no
    /// accessible children.
    ///
    /// This is intentionally strict: pages with even a single named
    /// interactive element (e.g. a button with text) are considered
    /// valid DOM pages, not canvas. Shadow-DOM pages that have unnamed
    /// or hidden interactive elements should go through DOM retries.
    fn is_canvas_or_shadow_dom(&self, stripped: &RawTree) -> bool {
        let node_count = stripped.nodes.len();

        // If there are any named interactive elements, this is a valid DOM page.
        let interactive = self.count_named_interactive(stripped);
        if interactive >= 1 {
            return false;
        }

        // Count meaningful names excluding WebArea (page title doesn't count).
        let meaningful_names = stripped
            .nodes
            .iter()
            .filter(|n| {
                let is_web_area = n.role.as_deref() == Some("WebArea")
                    || n.role.as_deref() == Some("RootWebArea");
                if is_web_area {
                    return false;
                }
                n.name.as_deref().is_some_and(|s| !s.trim().is_empty())
            })
            .count();

        // Canvas pages have just the WebArea node with nothing else meaningful.
        // A tree with 2+ nodes (e.g. WebArea + button) even if unnamed, is
        // likely a shadow-DOM page that should retry DOM rather than jumping
        // to vision immediately.
        if node_count <= 1 && meaningful_names == 0 && interactive == 0 {
            return true;
        }

        false
    }

    /// Count nodes with an interactive role AND a non-empty name.
    fn count_named_interactive(&self, tree: &RawTree) -> usize {
        tree.nodes
            .iter()
            .filter(|n| {
                let role_matches = n
                    .role
                    .as_deref()
                    .is_some_and(|r| self.config.interactive_roles.contains(&r));
                let has_name = n.name.as_deref().is_some_and(|s| !s.trim().is_empty());
                role_matches && has_name
            })
            .count()
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new(PolicyConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::{RawNode, RawTree};

    fn node(id: &str, parent: Option<&str>, children: &[&str], role: &str, name: &str) -> RawNode {
        RawNode {
            node_id: id.into(),
            parent_id: parent.map(str::to_string),
            child_ids: children.iter().map(|s| (*s).to_string()).collect(),
            role: Some(role.into()),
            name: if name.is_empty() {
                None
            } else {
                Some(name.into())
            },
            value: None,
            description: None,
            properties: vec![],
            ignored: false,
        }
    }

    /// Build a well-populated DOM tree (e.g. a typical form page).
    fn well_populated_tree() -> RawTree {
        RawTree::new(
            "https://example.com/form",
            vec![
                node("1", None, &["2"], "WebArea", "Form Page"),
                node("2", Some("1"), &["3", "4", "5", "6"], "generic", ""),
                node("3", Some("2"), &[], "textbox", "Username"),
                node("4", Some("2"), &[], "textbox", "Password"),
                node("5", Some("2"), &[], "button", "Submit"),
                node("6", Some("2"), &[], "link", "Forgot password"),
            ],
        )
    }

    /// Build a sparse canvas-like tree.
    fn canvas_like_tree() -> RawTree {
        RawTree::new(
            "https://example.com/canvas",
            vec![
                node("1", None, &["2"], "WebArea", "Canvas App"),
                node("2", Some("1"), &[], "generic", ""),
            ],
        )
    }

    /// Build a shadow-DOM heavy tree (very few named elements).
    fn shadow_dom_like_tree() -> RawTree {
        RawTree::new(
            "https://example.com/widget",
            vec![
                node("1", None, &["2"], "WebArea", "Widget"),
                node("2", Some("1"), &["3", "4"], "generic", ""),
                node("3", Some("2"), &[], "button", ""),
                node("4", Some("2"), &[], "generic", ""),
            ],
        )
    }

    #[test]
    fn well_populated_tree_uses_dom_mode() {
        let engine = PolicyEngine::default();
        let tree = well_populated_tree();
        let decision = engine.decide(&tree, 0);
        assert_eq!(
            decision.mode,
            PerceptionMode::Dom,
            "reason: {}",
            decision.reason
        );
        assert_eq!(decision.named_element_count, 4);
        assert!(decision.stripped_node_count >= 4);
    }

    #[test]
    fn canvas_tree_uses_vision_mode() {
        let engine = PolicyEngine::default();
        let tree = canvas_like_tree();
        let decision = engine.decide(&tree, 0);
        assert_eq!(
            decision.mode,
            PerceptionMode::Vision,
            "reason: {}",
            decision.reason
        );
        assert_eq!(decision.named_element_count, 0);
    }

    #[test]
    fn shadow_dom_sparse_tree_retries_then_falls_back() {
        let engine = PolicyEngine::default();
        let tree = shadow_dom_like_tree();

        // First attempt: should retry (sparse but not canvas)
        let first = engine.decide(&tree, 0);
        assert_eq!(
            first.mode,
            PerceptionMode::Dom,
            "first retry — reason: {}",
            first.reason
        );

        // After max retries: should fall back to vision
        let exhausted = engine.decide(&tree, 3);
        assert_eq!(
            exhausted.mode,
            PerceptionMode::Vision,
            "exhausted retries — reason: {}",
            exhausted.reason
        );
    }

    #[test]
    fn fully_empty_tree_uses_vision() {
        let engine = PolicyEngine::default();
        let tree = RawTree::new("https://example.com/empty", vec![]);
        let decision = engine.decide(&tree, 0);
        assert_eq!(
            decision.mode,
            PerceptionMode::Vision,
            "reason: {}",
            decision.reason
        );
    }

    #[test]
    fn single_button_tree_has_sufficient_dom() {
        let engine = PolicyEngine::default();
        let tree = RawTree::new(
            "https://example.com/simple",
            vec![
                node("1", None, &["2"], "WebArea", "Simple"),
                node("2", Some("1"), &[], "button", "OK"),
            ],
        );
        let decision = engine.decide(&tree, 0);
        assert_eq!(
            decision.mode,
            PerceptionMode::Dom,
            "reason: {}",
            decision.reason
        );
    }

    #[test]
    fn policy_config_can_be_tuned() {
        let config = PolicyConfig {
            min_named_interactive: 5,
            min_stripped_nodes: 10,
            ..PolicyConfig::default()
        };
        let engine = PolicyEngine::new(config);
        let tree = well_populated_tree();

        // With stricter thresholds, even a well-populated tree may need
        // more elements than we have (4 named interactive vs min 5).
        let decision = engine.decide(&tree, 0);
        assert_eq!(
            decision.mode,
            PerceptionMode::Dom,
            "should be Dom (retry) — reason: {}",
            decision.reason
        );

        // After max retries, it should be Dom still if not canvas/shadow-DOM
        // Wait, actually after max retries it should go to Vision.
        let exhausted = engine.decide(&tree, 3);
        assert_eq!(
            exhausted.mode,
            PerceptionMode::Vision,
            "exhausted retries with strict config — reason: {}",
            exhausted.reason
        );
    }

    #[test]
    fn tree_with_only_decorative_nodes_falls_back() {
        let engine = PolicyEngine::default();
        let tree = RawTree::new(
            "https://example.com/decorative",
            vec![
                node("1", None, &["2", "3"], "WebArea", "Decorative"),
                node("2", Some("1"), &[], "generic", ""),
                node("3", Some("1"), &[], "presentation", ""),
            ],
        );
        // After stripping, only WebArea remains — no interactive elements.
        let decision = engine.decide(&tree, 0);
        assert_eq!(
            decision.mode,
            PerceptionMode::Vision,
            "reason: {}",
            decision.reason
        );
    }

    #[test]
    fn interactive_roles_list_is_comprehensive() {
        let config = PolicyConfig::default();
        let expected = [
            "button",
            "link",
            "textbox",
            "checkbox",
            "radio",
            "combobox",
            "menuitem",
            "tab",
            "switch",
            "searchbox",
            "slider",
            "spinbutton",
            "listbox",
            "treeitem",
        ];
        for role in &expected {
            assert!(
                config.interactive_roles.contains(role),
                "missing role: {role}"
            );
        }
    }

    #[test]
    fn decision_reasoning_is_descriptive() {
        let engine = PolicyEngine::default();
        let tree = well_populated_tree();
        let decision = engine.decide(&tree, 0);
        assert!(!decision.reason.is_empty());
        assert!(decision.reason.contains("DOM mode"));
        assert!(decision.reason.contains("named interactive"));
    }

    #[test]
    fn canvas_tree_with_single_button_retries_then_uses_vision() {
        // A canvas with one unnamed button should retry DOM first,
        // then fall back to vision after max retries.
        let engine = PolicyEngine::default();
        let tree = RawTree::new(
            "https://example.com/canvas-btn",
            vec![
                node("1", None, &["2"], "WebArea", "Canvas App"),
                node("2", Some("1"), &["3"], "generic", ""),
                node("3", Some("2"), &[], "button", ""),
            ],
        );

        // First attempt: retry DOM (sparse but could be loading)
        let first = engine.decide(&tree, 0);
        assert_eq!(
            first.mode,
            PerceptionMode::Dom,
            "first attempt should retry DOM — reason: {}",
            first.reason
        );

        // After max retries: fall back to vision
        let exhausted = engine.decide(&tree, 3);
        assert_eq!(
            exhausted.mode,
            PerceptionMode::Vision,
            "after max retries should use vision — reason: {}",
            exhausted.reason
        );
    }
}

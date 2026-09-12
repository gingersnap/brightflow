//! Who said it: the layer and producer behind every semantic fact.
//!
//! The store keeps one row per (object, layer) and resolves per field across
//! layers in [`Layer::PRECEDENCE`] order. That is what lets a connector
//! re-declare on every sync — it only ever rewrites its own `Declared` rows —
//! while a person's edit stays on top, and what lets the UI say "from GitHub
//! connector 0.2.0" or "edited by you" next to a value. Provenance is a fact
//! about a row, never a template: the store still holds no opinion of its own.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Which kind of author wrote a semantic row. Lower layers are defaults;
/// higher layers are edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    /// The engine's own guess from dtypes and cardinality, written when a
    /// table has no declaration (uploads, events).
    Detected,
    /// A producer's declaration: a connector, an enrichment function.
    Declared,
    /// An LLM agent's edit, through the action bus.
    Agent,
    /// A person's edit, through the action bus.
    User,
}

impl Layer {
    /// Highest precedence first — the order the resolved view coalesces in.
    pub const PRECEDENCE: [Self; 4] = [Self::User, Self::Agent, Self::Declared, Self::Detected];

    /// The stored spelling — the inverse of `parse`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Detected => "detected",
            Self::Declared => "declared",
            Self::Agent => "agent",
            Self::User => "user",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::PRECEDENCE.into_iter().find(|l| l.as_str() == s)
    }

    /// `true` when `self` wins over `other` in the resolved view.
    pub fn outranks(self, other: Self) -> bool {
        let rank = |l: Self| {
            Self::PRECEDENCE
                .iter()
                .position(|p| *p == l)
                .unwrap_or(usize::MAX)
        };
        rank(self) < rank(other)
    }
}

/// The author of a set of semantic rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct Provenance {
    pub layer: Layer,
    /// Who, in a stable spelling: `connector:github`, `enrichment:ticket_classify`,
    /// `detector`, `user:<id>`, `agent:<run id>`.
    pub producer: String,
    /// The producer's version, when it has one (a connector's frontmatter
    /// version, an enrichment function version).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub version: Option<String>,
    /// A content hash of the producer, when it has one (a connector's source
    /// hash), so an unchanged producer can be told from a re-release.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hash: Option<String>,
}

impl Provenance {
    pub fn declared(producer: impl Into<String>) -> Self {
        Self {
            layer: Layer::Declared,
            producer: producer.into(),
            version: None,
            hash: None,
        }
    }

    pub fn detected() -> Self {
        Self {
            layer: Layer::Detected,
            producer: "detector".to_string(),
            version: None,
            hash: None,
        }
    }

    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    #[must_use]
    pub fn with_hash(mut self, hash: impl Into<String>) -> Self {
        self.hash = Some(hash.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_is_user_over_agent_over_declared_over_detected() {
        assert!(Layer::User.outranks(Layer::Agent));
        assert!(Layer::Agent.outranks(Layer::Declared));
        assert!(Layer::Declared.outranks(Layer::Detected));
        assert!(!Layer::Detected.outranks(Layer::User));
        assert!(!Layer::User.outranks(Layer::User));
    }

    #[test]
    fn stored_names_round_trip() {
        for layer in Layer::PRECEDENCE {
            assert_eq!(Layer::parse(layer.as_str()), Some(layer));
            let json = serde_json::to_string(&layer).unwrap();
            assert_eq!(json, format!("\"{}\"", layer.as_str()));
        }
        assert_eq!(Layer::parse("seed"), None);
    }

    #[test]
    fn provenance_builders() {
        let p = Provenance::declared("connector:github")
            .with_version("0.2.0")
            .with_hash("abc");
        assert_eq!(p.layer, Layer::Declared);
        assert_eq!(p.version.as_deref(), Some("0.2.0"));
        let json = serde_json::to_value(Provenance::detected()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"layer": "detected", "producer": "detector"})
        );
    }
}

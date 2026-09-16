use serde::{Deserialize, Serialize};

/// Observed state of an engine or catalog instance. One type for both so the
/// controller and the API never need parallel mappings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Health {
    Healthy,
    Degraded,
    Unavailable,
}

impl Health {
    pub fn as_str(&self) -> &'static str {
        match self {
            Health::Healthy => "healthy",
            Health::Degraded => "degraded",
            Health::Unavailable => "unavailable",
        }
    }
}

impl std::fmt::Display for Health {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Eligibility status of a candidate database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CandidateStatus {
    /// Database evaluation is valid and eligible for ranking.
    Eligible,
    /// Database was excluded from recommendations, along with the explanation.
    Excluded(String),
}

pub type EligibilityStatus = CandidateStatus;

impl CandidateStatus {
    /// Returns true if the candidate is eligible.
    pub fn is_eligible(&self) -> bool {
        matches!(self, Self::Eligible)
    }

    /// Returns true if the candidate is excluded.
    pub fn is_excluded(&self) -> bool {
        !self.is_eligible()
    }

    /// Returns the exclusion reason if excluded.
    pub fn exclusion_reason(&self) -> Option<&str> {
        match self {
            Self::Eligible => None,
            Self::Excluded(reason) => Some(reason.as_str()),
        }
    }
}

/// A recommendation result for a single database evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecommendationResult {
    /// Database identifier.
    pub database: String,
    /// Final normalized composite score (0.0 to 1.0).
    pub score: f64,
    /// Rank of the database (1 = best eligible candidate, 0 for excluded candidates).
    pub rank: usize,
    /// Contributing metrics (metric dimension -> normalized score in [0.0, 1.0]).
    pub contributing_metrics: HashMap<String, f64>,
    /// Concise, evidence-based explanation for the score or exclusion.
    pub explanation: String,
    /// Eligibility status information.
    pub status: CandidateStatus,
}

impl RecommendationResult {
    /// Returns the database identifier.
    pub fn database_name(&self) -> &str {
        &self.database
    }

    /// Returns the final score.
    pub fn final_score(&self) -> f64 {
        self.score
    }

    /// Returns the rank.
    pub fn rank(&self) -> usize {
        self.rank
    }

    /// Returns whether the candidate is eligible.
    pub fn is_eligible(&self) -> bool {
        self.status.is_eligible()
    }

    /// Returns whether the candidate is excluded.
    pub fn is_excluded(&self) -> bool {
        self.status.is_excluded()
    }
}

/// Final report produced by the Recommendation Engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecommendationReport {
    /// Workload identifier.
    pub workload_name: String,
    /// Ranked eligible database candidates, sorted by rank ascending (best first).
    pub ranked_candidates: Vec<RecommendationResult>,
    /// Best eligible candidate database identifier, if any candidate is eligible.
    pub best_candidate: Option<String>,
    /// Database evaluations excluded from recommendations, with reasons.
    pub excluded_candidates: Vec<RecommendationResult>,
    /// Human-readable summary of the recommendation outcome.
    pub summary: String,
}

impl RecommendationReport {
    /// Returns the top-ranked candidate result, if one exists.
    pub fn best_result(&self) -> Option<&RecommendationResult> {
        self.ranked_candidates.first()
    }

    /// Returns the database identifier of the top candidate, if any.
    pub fn best(&self) -> Option<&str> {
        self.best_candidate.as_deref()
    }

    /// Returns whether any eligible candidates were ranked.
    pub fn has_eligible_candidates(&self) -> bool {
        !self.ranked_candidates.is_empty()
    }

    /// Returns true if no candidates were evaluated or ranked.
    pub fn is_empty(&self) -> bool {
        self.ranked_candidates.is_empty() && self.excluded_candidates.is_empty()
    }

    /// Number of eligible candidates.
    pub fn eligible_count(&self) -> usize {
        self.ranked_candidates.len()
    }

    /// Number of excluded candidates.
    pub fn excluded_count(&self) -> usize {
        self.excluded_candidates.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidate_status_methods() {
        let eligible = CandidateStatus::Eligible;
        assert!(eligible.is_eligible());
        assert!(!eligible.is_excluded());
        assert_eq!(eligible.exclusion_reason(), None);

        let excluded = CandidateStatus::Excluded("Validation failed".to_string());
        assert!(!excluded.is_eligible());
        assert!(excluded.is_excluded());
        assert_eq!(excluded.exclusion_reason(), Some("Validation failed"));
    }

    #[test]
    fn test_models_serialization() {
        let mut contributing = HashMap::new();
        contributing.insert("latency".to_string(), 0.95);
        contributing.insert("throughput".to_string(), 0.85);

        let result = RecommendationResult {
            database: "qdrant".to_string(),
            score: 0.90,
            rank: 1,
            contributing_metrics: contributing,
            explanation: "Ranked #1 with score 0.90".to_string(),
            status: CandidateStatus::Eligible,
        };

        let report = RecommendationReport {
            workload_name: "test_wl".to_string(),
            ranked_candidates: vec![result.clone()],
            best_candidate: Some("qdrant".to_string()),
            excluded_candidates: vec![],
            summary: "Recommended qdrant".to_string(),
        };

        let json = serde_json::to_string(&report).unwrap();
        let deserialized: RecommendationReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, deserialized);
        assert_eq!(deserialized.best(), Some("qdrant"));
        assert_eq!(deserialized.best_result().unwrap().database, "qdrant");
    }
}

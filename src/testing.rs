//! Fixtures shared by unit tests across modules.

use std::path::PathBuf;

use crate::scenario::{Budget, Family, Probe, Scenario, Target, TaskKind, Truth, TruthMode};

/// A minimal retrieval scenario carrying the given truth.
pub fn scenario_with_truth(truth: Truth) -> Scenario {
    Scenario {
        id: "test/fixture".into(),
        family: Family::Retrieval,
        task: TaskKind::Callers,
        question: "who calls this?".into(),
        target: Target {
            ecosystem: "crates.io".into(),
            package: "memchr".into(),
            version: "2.8.3".into(),
            to_version: None,
            root: None,
        },
        probe: Probe {
            name: Some("memchr3".into()),
            ..Default::default()
        },
        truth,
        budget: Budget::default(),
        notes: None,
        source_path: PathBuf::new(),
    }
}

/// A diff scenario, for adapters that must decline it.
pub fn diff_scenario() -> Scenario {
    let mut scenario = scenario_with_truth(Truth {
        mode: TruthMode::SymbolSet,
        all_of: vec!["memchr::memchr3".into()],
        any_of: vec![],
        none_of: vec![],
        oracle: None,
        sources: vec![],
    });
    scenario.task = TaskKind::Diff;
    scenario.target.to_version = Some("2.8.0".into());
    scenario
}

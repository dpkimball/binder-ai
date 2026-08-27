use crate::runtime::GitDiff;
use crate::workflows::hygiene::finding::FindingKind;
use crate::workflows::hygiene::remediation::RemediationType;

/// One finding after dispatch + optional repair/workflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingOutcome {
    pub repository: String,
    pub finding: FindingKind,
    pub remediation: RemediationType,
    pub success: bool,
    pub diff_generated: bool,
    pub tests_passed: Option<bool>,
    pub summary: String,
    pub git_diff: Option<GitDiff>,
}

/// Full hygiene remediation pass. Inspection is not included.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HygieneReport {
    pub outcomes: Vec<FindingOutcome>,
}

impl HygieneReport {
    pub fn push(&mut self, outcome: FindingOutcome) {
        self.outcomes.push(outcome);
    }

    pub fn render(&self) -> String {
        let mut out = String::from(
            "Repository\tFinding\tRemediation Type\tSuccess\tDiff Generated\tTests Passed\n",
        );
        for row in &self.outcomes {
            let tests = match row.tests_passed {
                Some(true) => "true",
                Some(false) => "false",
                None => "n/a",
            };
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\n",
                row.repository,
                row.finding,
                row.remediation,
                row.success,
                row.diff_generated,
                tests
            ));
        }
        out
    }
}

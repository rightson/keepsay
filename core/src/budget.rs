//! Token usage and monthly budget (FR-6, PROPOSAL §6.8). Non-secret.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BudgetStatus {
    pub month_usage: Usage,
    pub estimated_usd: f64,
    pub soft_limit_usd: Option<f64>,
    pub hard_limit_usd: Option<f64>,
}

impl BudgetStatus {
    /// At or past the hard limit, the app switches to raw mode.
    pub fn hard_limited(&self) -> bool {
        self.hard_limit_usd.is_some_and(|h| self.estimated_usd >= h)
    }
}

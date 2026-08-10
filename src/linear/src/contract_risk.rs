//! Per-contract dynamic risk factor tracking — fee-spec.md §14.7.
//!
//! Each contract earns its own risk factor through observed behavior.
//! Risk factors are stored in a chain-state sled tree (`contract_risk`),
//! updated by the miner at window boundaries, and read by `compute_total_fee()`.
//!
//! [1:1] Python model: `ContractRiskTracker` in `contrib/model/fee_window_model.py`.

use std::collections::HashMap;

use dwow_sdk::blockchain::RiskFactor;
use dwow_sdk::crypto::ContractId;

/// Re-exported from nominal type for backward compatibility.
/// Prefer `RiskFactor::SCALE` in new code.
pub const RISK_FACTOR_SCALE: u64 = RiskFactor::SCALE;

/// Internal key type: ContractId bytes. ContractId doesn't implement Hash,
/// so we use the raw bytes as the map key.
type CidKey = [u8; 32];

fn cid_key(cid: &ContractId) -> CidKey {
    let mut key = [0u8; 32];
    key.copy_from_slice(cid.to_bytes().as_slice());
    key
}

/// System parameters for the ContractRiskTracker.
/// These are genesis-initialized per FI-GEN-1. The values here are the
/// initial defaults used until genesis ceremony stores real values.
#[derive(Debug, Clone)]
pub struct RiskTrackerParams {
    /// Additive step per window above tolerance. Default: +0.25×.
    pub escalation_step: RiskFactor,
    /// Subtractive step per N conforming windows. Default: -0.05×.
    /// FI-RISK-2: de-escalation SHALL be slower than escalation.
    pub deescalation_step: RiskFactor,
    /// Maximum risk factor cap. Default: 200_000 = 2.0×.
    pub max_risk_factor: RiskFactor,
    /// Floor for risk factor. Default: 100_000 = 1.0×.
    /// FI-RISK-4: new contracts start here.
    pub baseline_risk_factor: RiskFactor,
    /// Allowed deviation ratio (±50% = 0.50). Deviations within this
    /// range do not count toward escalation.
    pub tolerance: f64,
    /// Number of consecutive conforming windows required for one
    /// de-escalation step. Default: 4.
    pub conforming_windows_for_deescalation: u32,
}

impl Default for RiskTrackerParams {
    fn default() -> Self {
        Self {
            escalation_step: RiskFactor::new(25_000),
            deescalation_step: RiskFactor::new(5_000),
            max_risk_factor: RiskFactor::MAX,
            baseline_risk_factor: RiskFactor::BASELINE,
            tolerance: 0.50,
            conforming_windows_for_deescalation: 4,
        }
    }
}

/// A single cost deviation observation for one contract in one window.
#[derive(Debug, Clone)]
pub struct CostDeviation {
    pub contract_id: ContractId,
    pub function: String,
    pub declared_cost: u64,
    pub observed_cost: u64,
    pub window_id: u64,
}

impl CostDeviation {
    /// True if the deviation is within the tolerance threshold.
    pub fn within_tolerance(&self, tolerance: f64) -> bool {
        if self.declared_cost == 0 {
            return false; // zero declared cost with non-zero observed is always a deviation
        }
        let ratio = self.observed_cost as f64 / self.declared_cost as f64;
        (ratio - 1.0).abs() <= tolerance
    }
}

/// Per-contract dynamic risk factor tracker.
///
/// FI-RISK-3: Risk factors are stored per contract_id. No global
/// classification table exists. Risk is earned through under-declaration.
///
/// FI-RISK-4: New contracts start at baseline_risk_factor.
///
/// FI-RISK-5: Any node can read a contract's current risk factor.
pub struct ContractRiskTracker {
    params: RiskTrackerParams,
    /// Per-contract risk factors. Key: ContractId bytes, Value: RiskFactor.
    /// In production this is a sled tree; in tests an in-memory HashMap.
    contract_risk: HashMap<CidKey, RiskFactor>,
    /// Pending deviations for the current window. Cleared after evaluate_window().
    deviations: HashMap<CidKey, Vec<CostDeviation>>,
    /// Consecutive conforming windows per contract.
    conforming_windows: HashMap<CidKey, u32>,
}

impl ContractRiskTracker {
    /// Create a new tracker with the given system parameters.
    pub fn new(params: RiskTrackerParams) -> Self {
        Self {
            params,
            contract_risk: HashMap::new(),
            deviations: HashMap::new(),
            conforming_windows: HashMap::new(),
        }
    }

    /// Read a contract's current risk factor. FI-RISK-4: new contracts
    /// return baseline. FI-RISK-5: any node can call this.
    pub fn get_risk_factor(&self, contract_id: &ContractId) -> RiskFactor {
        self.contract_risk
            .get(&cid_key(contract_id))
            .copied()
            .unwrap_or(self.params.baseline_risk_factor)
    }

    /// Record a cost deviation for a contract in the current window.
    pub fn record(
        &mut self,
        contract_id: ContractId,
        function: String,
        declared_cost: u64,
        observed_cost: u64,
        window_id: u64,
    ) {
        let dev = CostDeviation { contract_id, function, declared_cost, observed_cost, window_id };
        let key = cid_key(&dev.contract_id);
        self.deviations.entry(key).or_default().push(dev);
    }

    /// Evaluate a contract's deviations for the current window and update
    /// its risk factor. Returns the new risk factor.
    ///
    /// FI-RISK-2: Escalation for under-declaration, de-escalation for
    /// sustained accuracy. De-escalation is slower than escalation.
    pub fn evaluate_window(&mut self, contract_id: &ContractId) -> RiskFactor {
        let key = cid_key(contract_id);
        let current = self.get_risk_factor(contract_id);
        let devs = self.deviations.remove(&key).unwrap_or_default();

        if devs.is_empty() {
            return current;
        }

        let above_tolerance = devs.iter()
            .filter(|d| !d.within_tolerance(self.params.tolerance))
            .count();

        let new_risk = if above_tolerance > 0 {
            let escalated = RiskFactor::new(
                current.get().saturating_add(self.params.escalation_step.get())
            );
            let capped = if escalated.get() > self.params.max_risk_factor.get() {
                self.params.max_risk_factor
            } else {
                escalated
            };
            self.conforming_windows.insert(key, 0);
            capped
        } else {
            let consecutive = self.conforming_windows.get(&key).copied().unwrap_or(0) + 1;
            if consecutive >= self.params.conforming_windows_for_deescalation {
                let deescalated = RiskFactor::new(
                    current.get().saturating_sub(self.params.deescalation_step.get())
                );
                let floored = if deescalated.get() < self.params.baseline_risk_factor.get() {
                    self.params.baseline_risk_factor
                } else {
                    deescalated
                };
                self.conforming_windows.insert(key, 0);
                floored
            } else {
                self.conforming_windows.insert(key, consecutive);
                current
            }
        };

        self.contract_risk.insert(key, new_risk);
        new_risk
    }

    /// Persist the contract_risk map to a sled tree.
    pub fn save_to_tree(&self, tree: &sled::Tree) -> Result<(), sled::Error> {
        for (key, risk) in &self.contract_risk {
            tree.insert(key.as_slice(), &risk.get().to_le_bytes())?;
        }
        Ok(())
    }

    /// Load the contract_risk map from a sled tree.
    pub fn load_from_tree(&mut self, tree: &sled::Tree) -> Result<(), sled::Error> {
        for entry in tree.iter() {
            let (key, value) = entry?;
            if key.len() == 32 && value.len() == 8 {
                let mut key_arr = [0u8; 32];
                key_arr.copy_from_slice(&key);
                let mut risk_bytes = [0u8; 8];
                risk_bytes.copy_from_slice(&value);
                let risk = RiskFactor::from_le_bytes(risk_bytes);
                self.contract_risk.insert(key_arr, risk);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cid(id: u8) -> ContractId {
        let mut bytes = [0u8; 32];
        bytes[0] = id;
        ContractId::from_bytes(bytes).unwrap()
    }

    #[test]
    fn test_new_contract_baseline() {
        let tracker = ContractRiskTracker::new(RiskTrackerParams::default());
        assert_eq!(tracker.get_risk_factor(&test_cid(1)), RiskFactor::BASELINE);
    }

    #[test]
    fn test_deviation_within_tolerance() {
        let d = CostDeviation {
            contract_id: test_cid(1), function: "f".into(),
            declared_cost: 1000, observed_cost: 1400, window_id: 0,
        };
        assert!(d.within_tolerance(0.50));
    }

    #[test]
    fn test_deviation_above_tolerance() {
        let d = CostDeviation {
            contract_id: test_cid(1), function: "f".into(),
            declared_cost: 1000, observed_cost: 1600, window_id: 0,
        };
        assert!(!d.within_tolerance(0.50));
    }

    #[test]
    fn test_escalation_one_window() {
        let params = RiskTrackerParams::default();
        let mut tracker = ContractRiskTracker::new(params.clone());
        tracker.record(test_cid(1), "f".into(), 1000, 2000, 0);
        let new_risk = tracker.evaluate_window(&test_cid(1));
        assert_eq!(new_risk.get(), RiskFactor::BASELINE.get() + params.escalation_step.get());
    }

    #[test]
    fn test_escalation_two_windows() {
        let params = RiskTrackerParams::default();
        let mut tracker = ContractRiskTracker::new(params.clone());
        tracker.record(test_cid(1), "f".into(), 1000, 2000, 0);
        tracker.evaluate_window(&test_cid(1));
        tracker.record(test_cid(1), "f".into(), 1000, 2000, 1);
        let new_risk = tracker.evaluate_window(&test_cid(1));
        assert_eq!(new_risk.get(), RiskFactor::BASELINE.get() + params.escalation_step.get() * 2);
    }

    #[test]
    fn test_escalation_capped() {
        let params = RiskTrackerParams::default();
        let mut tracker = ContractRiskTracker::new(params.clone());
        for w in 0..20 {
            tracker.record(test_cid(1), "f".into(), 1000, 2000, w);
            tracker.evaluate_window(&test_cid(1));
        }
        assert_eq!(tracker.get_risk_factor(&test_cid(1)), params.max_risk_factor);
    }

    #[test]
    fn test_per_contract_independence() {
        let mut tracker = ContractRiskTracker::new(RiskTrackerParams::default());
        tracker.record(test_cid(1), "f".into(), 1000, 2000, 0);
        tracker.evaluate_window(&test_cid(1));
        assert_eq!(tracker.get_risk_factor(&test_cid(2)), RiskFactor::BASELINE);
    }

    #[test]
    fn test_risk_emerges_from_observation() {
        let mut tracker = ContractRiskTracker::new(RiskTrackerParams::default());
        for w in 0..8 {
            tracker.record(test_cid(1), "f".into(), 1000, 1100, w);
            tracker.evaluate_window(&test_cid(1));
        }
        assert_eq!(tracker.get_risk_factor(&test_cid(1)), RiskFactor::BASELINE,
            "accurate contract stays at baseline");
        for w in 0..8 {
            tracker.record(test_cid(2), "f".into(), 1000, 2000, w);
            tracker.evaluate_window(&test_cid(2));
        }
        let params = RiskTrackerParams::default();
        assert_eq!(tracker.get_risk_factor(&test_cid(2)), params.max_risk_factor,
            "inaccurate contract escalates");
    }
}

/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

#![allow(dead_code)]

//! Capital deployer management for external backing
//!
//! This module provides functionality for external capital providers ("backers")
//! to deploy capital to relayers in exchange for a share of the relayer's fees.

use super::config::CapitalDeployerConfig;
use super::error::{RelayerError, Result};

/// Represents a capital deployment
#[derive(Debug, Clone)]
pub struct Deployment {
    /// Backer address
    pub backer: String,
    /// Amount deployed
    pub amount: u64,
    /// Relayer receiving the deployment
    pub relayer: String,
    /// Cut percentage for backer (in basis points)
    pub backer_cut_bp: u32,
    /// Block when deployment was made
    pub deployed_at_block: u64,
}

/// Capital deployer manager
pub struct CapitalDeployer {
    config: CapitalDeployerConfig,
    /// Active deployments
    deployments: Vec<Deployment>,
    /// Total deployed amount
    total_deployed: u64,
}

impl CapitalDeployer {
    /// Create a new capital deployer manager
    pub fn new(config: CapitalDeployerConfig) -> Self {
        Self { config, deployments: Vec::new(), total_deployed: 0 }
    }

    /// Check if deployer is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Add a new deployment
    pub fn add_deployment(&mut self, deployment: Deployment) -> Result<()> {
        if !self.config.enabled {
            return Err(RelayerError::DeployerError("Deployer not enabled".to_string()));
        }

        if deployment.amount < self.config.min_deploy {
            return Err(RelayerError::DeployerError(format!(
                "Deployment {} below minimum {}",
                deployment.amount, self.config.min_deploy
            )));
        }

        if deployment.amount > self.config.max_deploy {
            return Err(RelayerError::DeployerError(format!(
                "Deployment {} above maximum {}",
                deployment.amount, self.config.max_deploy
            )));
        }

        self.total_deployed += deployment.amount;
        self.deployments.push(deployment);
        Ok(())
    }

    /// Remove a deployment
    pub fn remove_deployment(&mut self, backer: &str, relayer: &str) -> Result<u64> {
        let pos = self
            .deployments
            .iter()
            .position(|d| d.backer == backer && d.relayer == relayer)
            .ok_or_else(|| RelayerError::DeployerError("Deployment not found".to_string()))?;

        let deployment = self.deployments.remove(pos);
        self.total_deployed -= deployment.amount;
        Ok(deployment.amount)
    }

    /// Get total deployed amount
    pub fn total_deployed(&self) -> u64 {
        self.total_deployed
    }

    /// Get deployment count
    pub fn deployment_count(&self) -> usize {
        self.deployments.len()
    }

    /// Calculate backer's share of fees
    pub fn calculate_backer_share(&self, total_fees: u64) -> u64 {
        (total_fees * self.config.deployer_cut_bp as u64) / 10000
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CapitalDeployerConfig;

    fn enabled_config() -> CapitalDeployerConfig {
        CapitalDeployerConfig {
            enabled: true,
            min_deploy: 100,
            max_deploy: 10000,
            deployer_cut_bp: 1500,
            ..Default::default()
        }
    }

    fn test_deployment(backer: &str, amount: u64) -> Deployment {
        Deployment {
            backer: backer.to_string(),
            amount,
            relayer: "relayer1".to_string(),
            backer_cut_bp: 1500,
            deployed_at_block: 1,
        }
    }

    #[test]
    fn test_new() {
        let deployer = CapitalDeployer::new(enabled_config());
        assert!(deployer.is_enabled());
        assert_eq!(deployer.total_deployed(), 0);
        assert_eq!(deployer.deployment_count(), 0);
    }

    #[test]
    fn test_new_disabled() {
        let cfg = CapitalDeployerConfig { enabled: false, ..Default::default() };
        let deployer = CapitalDeployer::new(cfg);
        assert!(!deployer.is_enabled());
    }

    #[test]
    fn test_add_deployment_success() {
        let mut deployer = CapitalDeployer::new(enabled_config());
        deployer.add_deployment(test_deployment("backer1", 500)).unwrap();
        assert_eq!(deployer.total_deployed(), 500);
        assert_eq!(deployer.deployment_count(), 1);
    }

    #[test]
    fn test_add_deployment_below_min() {
        let mut deployer = CapitalDeployer::new(enabled_config());
        let err = deployer.add_deployment(test_deployment("backer1", 50)).unwrap_err();
        assert!(matches!(err, RelayerError::DeployerError(_)));
    }

    #[test]
    fn test_add_deployment_above_max() {
        let mut deployer = CapitalDeployer::new(enabled_config());
        let err = deployer.add_deployment(test_deployment("backer1", 20000)).unwrap_err();
        assert!(matches!(err, RelayerError::DeployerError(_)));
    }

    #[test]
    fn test_add_deployment_disabled() {
        let cfg = CapitalDeployerConfig { enabled: false, ..Default::default() };
        let mut deployer = CapitalDeployer::new(cfg);
        let err = deployer.add_deployment(test_deployment("backer1", 500)).unwrap_err();
        assert!(matches!(err, RelayerError::DeployerError(_)));
    }

    #[test]
    fn test_add_deployment_at_min_boundary() {
        let mut deployer = CapitalDeployer::new(enabled_config());
        deployer.add_deployment(test_deployment("backer1", 100)).unwrap();
        assert_eq!(deployer.total_deployed(), 100);
    }

    #[test]
    fn test_add_deployment_at_max_boundary() {
        let mut deployer = CapitalDeployer::new(enabled_config());
        deployer.add_deployment(test_deployment("backer1", 10000)).unwrap();
        assert_eq!(deployer.total_deployed(), 10000);
    }

    #[test]
    fn test_add_multiple_deployments() {
        let mut deployer = CapitalDeployer::new(enabled_config());
        deployer.add_deployment(test_deployment("backer1", 200)).unwrap();
        deployer.add_deployment(test_deployment("backer2", 300)).unwrap();
        assert_eq!(deployer.total_deployed(), 500);
        assert_eq!(deployer.deployment_count(), 2);
    }

    #[test]
    fn test_remove_deployment_success() {
        let mut deployer = CapitalDeployer::new(enabled_config());
        deployer.add_deployment(test_deployment("backer1", 500)).unwrap();
        let removed = deployer.remove_deployment("backer1", "relayer1").unwrap();
        assert_eq!(removed, 500);
        assert_eq!(deployer.total_deployed(), 0);
        assert_eq!(deployer.deployment_count(), 0);
    }

    #[test]
    fn test_remove_deployment_not_found() {
        let mut deployer = CapitalDeployer::new(enabled_config());
        let err = deployer.remove_deployment("nobody", "relayer1").unwrap_err();
        assert!(matches!(err, RelayerError::DeployerError(_)));
    }

    #[test]
    fn test_remove_deployment_wrong_relayer() {
        let mut deployer = CapitalDeployer::new(enabled_config());
        deployer.add_deployment(test_deployment("backer1", 500)).unwrap();
        let err = deployer.remove_deployment("backer1", "other_relayer").unwrap_err();
        assert!(matches!(err, RelayerError::DeployerError(_)));
    }

    #[test]
    fn test_calculate_backer_share() {
        let deployer = CapitalDeployer::new(enabled_config());
        // deployer_cut_bp = 1500 = 15%
        assert_eq!(deployer.calculate_backer_share(10000), 1500);
        assert_eq!(deployer.calculate_backer_share(0), 0);
        assert_eq!(deployer.calculate_backer_share(100), 15);
    }

    #[test]
    fn test_calculate_backer_share_rounding() {
        let cfg = CapitalDeployerConfig { deployer_cut_bp: 1, ..enabled_config() };
        let deployer = CapitalDeployer::new(cfg);
        // 1 bp = 0.01%, so 500 * 1 / 10000 = 0
        assert_eq!(deployer.calculate_backer_share(500), 0);
    }
}

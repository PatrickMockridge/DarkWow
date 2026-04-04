/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

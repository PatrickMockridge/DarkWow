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

//! Insurance Market Contract Client API
//!
//! This module provides the client-side API for building Insurance Market contract calls.

pub mod underwrite_with_capability_v1;
pub mod purchase_coverage_with_capability_v1;

use dwow_sdk::{
    crypto::{
        pasta_prelude::{Curve, CurveAffine, Group},
        poseidon_hash,
        schnorr::{SchnorrSecret, Signature},
        PublicKey, SecretKey,
    },
    pasta::pallas,
};
use dwow_serial::serialize;

use crate::model::{
    RegisterRiskTypeParamsV1,
    CreateMarketParamsV1,
    UnderwriteParamsV1,
    PurchaseCoverageParamsV1,
    PurchaseCoverageWithDAGParamsV1,
    FileClaimParamsV1,
    DeactivateUnderwriterParamsV1,
    CloseMarketParamsV1,
    RetireRiskTypeParamsV1,
};

/// Builder for registering a risk type
pub struct RegisterRiskTypeV1Builder {
    category: u8,
    description: Vec<u8>,
    base_premium_rate: u32,
    min_bond_rate: u32,
    oracle_pubkey: PublicKey,
}

impl RegisterRiskTypeV1Builder {
    /// Create a new risk type builder
    pub fn new(category: u8, description: String, oracle_pubkey: PublicKey) -> Self {
        Self {
            category,
            description: description.into_bytes(),
            base_premium_rate: 500, // 5% default
            min_bond_rate: 1000,    // 10% default
            oracle_pubkey,
        }
    }

    /// Set base premium rate (basis points)
    pub fn base_premium_rate(mut self, rate: u32) -> Self {
        self.base_premium_rate = rate;
        self
    }

    /// Set minimum bond rate (basis points)
    pub fn min_bond_rate(mut self, rate: u32) -> Self {
        self.min_bond_rate = rate;
        self
    }

    /// Build the params
    pub fn build(self) -> RegisterRiskTypeParamsV1 {
        use crate::model::RiskCategory;
        RegisterRiskTypeParamsV1 {
            category: RiskCategory::try_from(self.category).unwrap_or(RiskCategory::Custom),
            description: self.description,
            base_premium_rate: self.base_premium_rate,
            min_bond_rate: self.min_bond_rate,
            oracle_pubkey: self.oracle_pubkey,
        }
    }
}

/// Builder for creating an insurance market
pub struct CreateMarketV1Builder {
    risk_type_id: pallas::Base,
    initial_premium_rate: u32,
    total_coverage: u64,
    coverage_period: u64,
    deductible: u64,
    max_coverage_per_buyer: u64,
    closes_at: u64,
}

impl CreateMarketV1Builder {
    /// Create a new market builder
    pub fn new(risk_type_id: pallas::Base, total_coverage: u64, coverage_period: u64) -> Self {
        Self {
            risk_type_id,
            initial_premium_rate: 0, // Use risk type's base rate
            total_coverage,
            coverage_period,
            deductible: 0,
            max_coverage_per_buyer: total_coverage,
            closes_at: 0,
        }
    }

    /// Set initial premium rate (basis points, 0 = use risk type default)
    pub fn initial_premium_rate(mut self, rate: u32) -> Self {
        self.initial_premium_rate = rate;
        self
    }

    /// Set deductible amount
    pub fn deductible(mut self, amount: u64) -> Self {
        self.deductible = amount;
        self
    }

    /// Set max coverage per buyer
    pub fn max_coverage_per_buyer(mut self, amount: u64) -> Self {
        self.max_coverage_per_buyer = amount;
        self
    }

    /// Set market close block height
    pub fn closes_at(mut self, block: u64) -> Self {
        self.closes_at = block;
        self
    }

    /// Build the params
    pub fn build(self) -> CreateMarketParamsV1 {
        CreateMarketParamsV1 {
            risk_type_id: self.risk_type_id,
            initial_premium_rate: self.initial_premium_rate,
            total_coverage: self.total_coverage,
            coverage_period: self.coverage_period,
            deductible: self.deductible,
            max_coverage_per_buyer: self.max_coverage_per_buyer,
            closes_at: self.closes_at,
            required_underwriter_capability: None,
            required_buyer_capability: None,
            required_dag_id: None,
        }
    }
}

/// Builder for underwriting a risk
pub struct UnderwriteV1Builder {
    market_id: pallas::Base,
    bond_amount: u64,
    coverage_limit: u64,
    underwriter: PublicKey,
}

impl UnderwriteV1Builder {
    /// Create a new underwrite builder
    pub fn new(market_id: pallas::Base, underwriter: PublicKey) -> Self {
        Self {
            market_id,
            bond_amount: 0,
            coverage_limit: 0,
            underwriter,
        }
    }

    /// Set bond amount
    pub fn bond_amount(mut self, amount: u64) -> Self {
        self.bond_amount = amount;
        self
    }

    /// Set coverage limit
    pub fn coverage_limit(mut self, amount: u64) -> Self {
        self.coverage_limit = amount;
        self
    }

    /// Build the params
    pub fn build(self) -> UnderwriteParamsV1 {
        UnderwriteParamsV1 {
            market_id: self.market_id,
            bond_amount: self.bond_amount,
            coverage_limit: self.coverage_limit,
            underwriter: self.underwriter,
        }
    }
}

/// Builder for purchasing coverage
pub struct PurchaseCoverageV1Builder {
    market_id: pallas::Base,
    underwriter_id: pallas::Base,
    buyer: PublicKey,
    buyer_secret: SecretKey,
    coverage_amount: u64,
    value_commit: pallas::Point,
    premium_rate: u32,
}

impl PurchaseCoverageV1Builder {
    /// Create a new purchase coverage builder
    pub fn new(
        market_id: pallas::Base,
        underwriter_id: pallas::Base,
        buyer: PublicKey,
        buyer_secret: SecretKey,
        value_commit: pallas::Point,
    ) -> Self {
        Self {
            market_id,
            underwriter_id,
            buyer,
            buyer_secret,
            coverage_amount: 0,
            value_commit,
            premium_rate: 500, // 5% default
        }
    }

    /// Set coverage amount
    pub fn coverage_amount(mut self, amount: u64) -> Self {
        self.coverage_amount = amount;
        self
    }

    /// Set premium rate (basis points, 500 = 5%)
    pub fn premium_rate(mut self, rate: u32) -> Self {
        self.premium_rate = rate;
        self
    }

    /// Build the params with a Schnorr signature binding (buyer, value_commit, premium)
    pub fn build(self) -> PurchaseCoverageParamsV1 {
        let premium = crate::model::calculate_premium(self.coverage_amount, self.premium_rate)
            .unwrap_or(0);
        let vc_coords = self.value_commit.to_affine().coordinates().unwrap();
        let signature_msg = serialize(&poseidon_hash([
            self.buyer.x(),
            self.buyer.y(),
            *vc_coords.x(),
            *vc_coords.y(),
            pallas::Base::from(premium),
        ]));
        let signature = self.buyer_secret.sign(&signature_msg);

        PurchaseCoverageParamsV1 {
            market_id: self.market_id,
            underwriter_id: self.underwriter_id,
            buyer: self.buyer,
            coverage_amount: self.coverage_amount,
            value_commit: self.value_commit,
            signature,
        }
    }
}

/// Builder for purchasing coverage with DAG qualification
pub struct PurchaseCoverageWithDAGV1Builder {
    market_id: pallas::Base,
    underwriter_id: pallas::Base,
    buyer: PublicKey,
    buyer_secret: SecretKey,
    coverage_amount: u64,
    value_commit: pallas::Point,
    premium_rate: u32,
    dag_proof: Vec<u8>,
    dag_path_index: u32,
    required_dag_id: [u8; 32],
}

impl PurchaseCoverageWithDAGV1Builder {
    pub fn new(
        market_id: pallas::Base,
        underwriter_id: pallas::Base,
        buyer: PublicKey,
        buyer_secret: SecretKey,
        value_commit: pallas::Point,
        dag_proof: Vec<u8>,
        dag_path_index: u32,
        required_dag_id: [u8; 32],
    ) -> Self {
        Self {
            market_id,
            underwriter_id,
            buyer,
            buyer_secret,
            coverage_amount: 0,
            value_commit,
            premium_rate: 500,
            dag_proof,
            dag_path_index,
            required_dag_id,
        }
    }

    pub fn coverage_amount(mut self, amount: u64) -> Self {
        self.coverage_amount = amount;
        self
    }

    pub fn premium_rate(mut self, rate: u32) -> Self {
        self.premium_rate = rate;
        self
    }

    pub fn build(self) -> crate::model::PurchaseCoverageWithDAGParamsV1 {
        let premium = crate::model::calculate_premium(self.coverage_amount, self.premium_rate)
            .unwrap_or(0);
        let vc_coords = self.value_commit.to_affine().coordinates().unwrap();
        let signature_msg = serialize(&poseidon_hash([
            self.buyer.x(),
            self.buyer.y(),
            *vc_coords.x(),
            *vc_coords.y(),
            pallas::Base::from(premium),
        ]));
        let signature = self.buyer_secret.sign(&signature_msg);

        crate::model::PurchaseCoverageWithDAGParamsV1 {
            market_id: self.market_id,
            underwriter_id: self.underwriter_id,
            buyer: self.buyer,
            coverage_amount: self.coverage_amount,
            value_commit: self.value_commit,
            signature,
            dag_proof: self.dag_proof,
            dag_path_index: self.dag_path_index,
            required_dag_id: self.required_dag_id,
        }
    }
}

/// Builder for filing a claim
pub struct FileClaimV1Builder {
    coverage_id: pallas::Base,
    market_id: pallas::Base,
    buyer: PublicKey,
    amount: u64,
    evidence: Vec<u8>,
    oracle_signature: pallas::Base,
}

impl FileClaimV1Builder {
    /// Create a new file claim builder
    pub fn new(coverage_id: pallas::Base, market_id: pallas::Base, buyer: PublicKey, amount: u64) -> Self {
        Self {
            coverage_id,
            market_id,
            buyer,
            amount,
            evidence: vec![],
            oracle_signature: pallas::Base::zero(),
        }
    }

    /// Set evidence/description
    pub fn evidence(mut self, evidence: Vec<u8>) -> Self {
        self.evidence = evidence;
        self
    }

    /// Build the params
    pub fn build(self) -> FileClaimParamsV1 {
        FileClaimParamsV1 {
            coverage_id: self.coverage_id,
            market_id: self.market_id,
            buyer: self.buyer,
            amount: self.amount,
            evidence: self.evidence,
            oracle_signature: self.oracle_signature,
        }
    }
}

/// Builder for deactivating an underwriter
pub struct DeactivateUnderwriterV1Builder {
    underwriter_id: pallas::Base,
    owner: PublicKey,
}

impl DeactivateUnderwriterV1Builder {
    pub fn new(underwriter_id: pallas::Base, owner: PublicKey) -> Self {
        Self { underwriter_id, owner }
    }

    pub fn build(self) -> DeactivateUnderwriterParamsV1 {
        DeactivateUnderwriterParamsV1 {
            underwriter_id: self.underwriter_id,
            owner: self.owner,
        }
    }
}

/// Builder for closing an insurance market
pub struct CloseMarketV1Builder {
    market_id: pallas::Base,
}

impl CloseMarketV1Builder {
    pub fn new(market_id: pallas::Base) -> Self {
        Self { market_id }
    }

    pub fn build(self) -> CloseMarketParamsV1 {
        CloseMarketParamsV1 {
            market_id: self.market_id,
        }
    }
}

/// Builder for retiring a risk type
pub struct RetireRiskTypeV1Builder {
    risk_type_id: pallas::Base,
}

impl RetireRiskTypeV1Builder {
    pub fn new(risk_type_id: pallas::Base) -> Self {
        Self { risk_type_id }
    }

    pub fn build(self) -> RetireRiskTypeParamsV1 {
        RetireRiskTypeParamsV1 {
            risk_type_id: self.risk_type_id,
        }
    }
}
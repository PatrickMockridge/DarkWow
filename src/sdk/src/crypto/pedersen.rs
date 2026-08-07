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

use pasta_curves::{arithmetic::CurveExt, pallas};

use super::{
    blind::ScalarBlind,
    constants::{
        fixed_bases::{
            VALUE_COMMITMENT_PERSONALIZATION, VALUE_COMMITMENT_R_BYTES, VALUE_COMMITMENT_V_BYTES,
        },
    },
    util::fp_mod_fv,
};

/// Pedersen commitment for a 64-bit value, in the base field.
#[allow(non_snake_case)]
pub fn pedersen_commitment_u64(value: u64, blind: ScalarBlind) -> pallas::Point {
    let hasher = pallas::Point::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
    let V = hasher(&VALUE_COMMITMENT_V_BYTES);
    let R = hasher(&VALUE_COMMITMENT_R_BYTES);

    // DISPENSATION: base field < scalar field — conversion guaranteed valid.
    let scalar_val = fp_mod_fv(pallas::Base::from(value))
        .expect("u64 to Base to Scalar: mathematically guaranteed valid");
    V * scalar_val + R * blind.inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pedersen_commitment() {
        let a_value: u64 = 10;
        let a_blind = ScalarBlind::from(11);
        let b_value: u64 = 20;
        let b_blind = ScalarBlind::from(21);

        assert_eq!(
            pedersen_commitment_u64(a_value, a_blind.clone()) + pedersen_commitment_u64(b_value, b_blind.clone()),
            pedersen_commitment_u64(a_value + b_value, &a_blind + &b_blind)
        );

        let a_value = 10;
        let b_value = 20;

        assert_eq!(
            pedersen_commitment_u64(a_value, a_blind.clone()) + pedersen_commitment_u64(b_value, b_blind.clone()),
            pedersen_commitment_u64(a_value + b_value, &a_blind + &b_blind)
        );
    }
}

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

//! Lottery Contract Integration Tests

#[cfg(test)]
mod tests {
    use darkfi_lottery_contract::model::{LotteryConfig, PrizeTierConfig};
    use darkfi_lottery_contract::LotteryFunction;

    #[test]
    fn test_lottery_function_enum_valid() {
        assert!(LotteryFunction::try_from(0x00).is_ok()); // InitializeV1
        assert!(LotteryFunction::try_from(0x01).is_ok()); // BuyTicketV1
        assert!(LotteryFunction::try_from(0x02).is_ok()); // DrawWinnersV1
        assert!(LotteryFunction::try_from(0x03).is_ok()); // RevealTicketV1
        assert!(LotteryFunction::try_from(0x04).is_ok()); // ClaimPrizeV1
        assert!(LotteryFunction::try_from(0x05).is_ok()); // ExpireLotteryV1
    }

    #[test]
    fn test_lottery_function_enum_invalid() {
        assert!(LotteryFunction::try_from(0xFF).is_err());
        assert!(LotteryFunction::try_from(0x06).is_err());
    }

    #[test]
    fn test_lottery_config_validation() {
        // Valid config
        let valid_config = LotteryConfig {
            num_picks: 5,
            number_range: 90,
            house_edge_bp: 2000,
            ticket_price: 100,
            prize_tiers: vec![
                PrizeTierConfig { matches_needed: 5, payout_percent: 5000, roll_to_next: true },
                PrizeTierConfig { matches_needed: 4, payout_percent: 2000, roll_to_next: false },
                PrizeTierConfig { matches_needed: 3, payout_percent: 1000, roll_to_next: false },
            ],
        };
        assert!(valid_config.validate().is_ok());

        // Invalid: num_picks > MAX_NUM_PICKS
        let invalid_picks = LotteryConfig {
            num_picks: 15, // MAX is 10
            number_range: 90,
            house_edge_bp: 2000,
            ticket_price: 100,
            prize_tiers: vec![],
        };
        assert!(invalid_picks.validate().is_err());

        // Invalid: num_picks > number_range
        let invalid_range = LotteryConfig {
            num_picks: 10,
            number_range: 5, // Can't pick 10 from 5
            house_edge_bp: 2000,
            ticket_price: 100,
            prize_tiers: vec![],
        };
        assert!(invalid_range.validate().is_err());
    }

    #[test]
    fn test_uk_lottery_config() {
        let uk_config = darkfi_lottery_contract::UK_LOTTERY_CONFIG();
        assert_eq!(uk_config.num_picks, 6);
        assert_eq!(uk_config.number_range, 59);
        assert_eq!(uk_config.house_edge_bp, 2500);
        assert_eq!(uk_config.ticket_price, 200);
        assert_eq!(uk_config.prize_tiers.len(), 4);
    }

    #[test]
    fn test_neighborhood_config() {
        let neighborhood = darkfi_lottery_contract::NEIGHBORHOOD_CONFIG();
        assert_eq!(neighborhood.num_picks, 3);
        assert_eq!(neighborhood.number_range, 10);
        assert_eq!(neighborhood.house_edge_bp, 1000);
        assert_eq!(neighborhood.ticket_price, 10);
        assert_eq!(neighborhood.prize_tiers.len(), 3);
    }

    #[test]
    fn test_min_matches() {
        let config = darkfi_lottery_contract::SIMPLE_690_CONFIG();
        assert_eq!(config.min_matches(), 3); // Last tier is 3 matches
        assert_eq!(config.max_matches(), 6); // num_picks
    }
}

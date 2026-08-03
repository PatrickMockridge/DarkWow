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

//! Escrow contract client module

/// ZK circuit binary constants (client-side proof generation)
pub mod zkbins;

pub mod create_escrow;
pub mod fund;
pub mod claim;
pub mod refund;
pub mod cancel;

// EscrowClient REMOVED (T2a — phantom-code-removed-first, 2026-07-15).
// The `impl ContractClient` stub was wallet grammar (WalletStateProvider) in a
// contract crate with zero consumers. Per wallet.md §6.4 / type-system.md §13,
// escrow is exercised through the generic manifest path — no per-contract
// wallet client. There is native token and there are capabilities; that is it.

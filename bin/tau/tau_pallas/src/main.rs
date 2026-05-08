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

//! Tau_Pallas entry point
//!
//! This is a Pallas-native variant of tau with DarkWow on-chain integration.
//! It uses dwow_sdk crypto (Pallas curve) throughout, enabling direct
//! transaction signing and darkfid RPC integration for on-chain capability
//! verification.

fn main() {
    // Print version on startup
    println!("tau_pallas v{}", env!("CARGO_PKG_VERSION"));
    println!("Pallas-native tau with DarkFi on-chain integration");
    println!();
    println!("Note: Full daemon not yet implemented.");
    println!("RPC client module available for darkfid integration.");
    println!();
    println!("Modules available:");
    println!("  - tau_pallas::rpc_client::DarkfidClient");
    println!("  - tau_pallas::capability (O-Cap verification)");
    println!("  - tau_pallas::task_info (Task management with Pallas keys)");
}

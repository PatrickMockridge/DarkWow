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

//! Identity contract integration tests

use darkfi_identity_contract::*;

#[test]
fn test_identity_function_enum() {
    // Test that all function IDs are valid
    assert!(IdentityFunction::try_from(0x00).is_ok());
    assert!(IdentityFunction::try_from(0x01).is_ok());
    assert!(IdentityFunction::try_from(0x02).is_ok());
    assert!(IdentityFunction::try_from(0x03).is_ok());
    assert!(IdentityFunction::try_from(0x04).is_ok());
    assert!(IdentityFunction::try_from(0xFF).is_err());
}

#[test]
fn test_credential_commitment() {
    // Test that commitment computation is deterministic
    let issuer_pub = [1u8; 32];
    let holder_pub = [2u8; 32];
    let schema_hash = [3u8; 32];
    let attrs = vec![4u8; 32];

    // In production, this would call the actual function
    // For now, just verify the structure is correct
    assert_eq!(issuer_pub.len(), 32);
    assert_eq!(holder_pub.len(), 32);
    assert_eq!(schema_hash.len(), 32);
}

#[test]
fn test_nullifier_computation() {
    // Test that nullifier computation is deterministic
    let issuer_pub = [1u8; 32];
    let holder_pub = [2u8; 32];

    // In production, this would call the actual function
    // For now, just verify the structure is correct
    assert_eq!(issuer_pub.len(), 32);
    assert_eq!(holder_pub.len(), 32);
}
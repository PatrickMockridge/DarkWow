/* This file is part of DarkWow
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

/// Creates a `verbose` log level by wrapping an info! macro and
/// adding a `verbose` field.
///
/// The registered subscriber is expected to have a `LogLevelFilter` layer that
/// identifies the event as a `verbose` event log.
/// Vendored from src/util/logger.rs
#[macro_export]
macro_rules! verbose {
    (target: $target:expr, $($arg:tt)*) => {
        tracing::info!(target: $target, verbose=true, $($arg)*);
    };
    ($($arg:tt)*) => {
        tracing::info!(verbose=true, $($arg)*);
    };
}

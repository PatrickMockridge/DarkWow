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

//! Test failure types — the mechanism for the Test Outcome Taxonomy.
//!
//! `doc/src/dev/testing/production-test-standard.md` defines a normative taxonomy:
//! **INFRA-FAIL** for a failure in shared infrastructure (which affects every test
//! equally), **TEST-FAIL** for a failure specific to the contract under test, and
//! it requires a prefix that MUST name the module, or the contract and endpoint.
//! A `.unwrap()` in a fixture cannot satisfy that: it renders as
//! `called Result::unwrap() on an Err value` at a file and line, naming neither.
//!
//! This module is that missing mechanism. A test or helper returns [`TestResult`]
//! and propagates with `?`; the failure arrives at libtest carrying its prefix, its
//! stage and its cause.
//!
//! Enabled by the `test-support` feature, which only *dev*-dependencies turn on.
//! Contracts build `dwow-sdk` with `wasm` alone, so this module never reaches a
//! contract artifact and cannot move the genesis hash.

use std::error::Error as StdError;
use std::fmt;
use std::panic::Location;
use std::result::Result as ResultGeneric;

/// `Result` returned by test helpers and test bodies.
pub type TestResult<T> = ResultGeneric<T, TestError>;

/// A test failure that names its cause, its stage, and the class of the taxonomy it
/// belongs to.
///
/// `Debug` is implemented by hand to delegate to `Display`, because libtest reports a
/// failing `Result`-returning test as `Error: {:?}` — a derived `Debug` would print a
/// struct dump where the prefixed message belongs.
pub enum TestError {
    /// **INFRA-FAIL** — a step in a shared infrastructure module failed. Affects every
    /// test equally, so it `SHALL` name the module that failed.
    Infra {
        /// The shared module that failed, e.g. `GenesisHarness`, `accept_block`.
        module: &'static str,
        /// What it was doing, e.g. `reading keys.toml`, `verifying the hash chain`.
        stage: &'static str,
        /// The underlying failure, preserved.
        cause: Box<dyn StdError>,
    },

    /// **TEST-FAIL** — a failure specific to the contract under test.
    Test {
        /// The contract under test.
        contract: &'static str,
        /// The endpoint being exercised.
        endpoint: &'static str,
        /// The underlying failure, preserved.
        cause: Box<dyn StdError>,
    },

    /// A propagated failure carrying the stage that failed. Produced by [`Context::at`].
    At {
        /// The stage that failed, named by the caller that knew it.
        stage: &'static str,
        /// Where the failure was propagated from.
        at: &'static Location<'static>,
        /// The underlying failure, preserved.
        cause: Box<dyn StdError>,
    },

    /// A value did not match what the test required.
    Check {
        /// Where the check was made.
        at: &'static Location<'static>,
        /// What was required.
        expected: String,
        /// What was found.
        actual: String,
        /// What the check was for, when the author said. Carries over the message that
        /// `assert_eq!` would have taken — including one that interpolates values — so
        /// converting an assertion never loses what it was reporting.
        note: Option<String>,
    },

    /// A boolean condition did not hold.
    Failed {
        /// Where the condition was checked.
        at: &'static Location<'static>,
        /// The condition, as written.
        cond: &'static str,
        /// What the check was for, when the author said.
        note: Option<String>,
    },
}

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Infra { module, stage, cause } => {
                write!(f, "INFRA-FAIL [{module}]: {stage}: {cause}")
            }
            Self::Test { contract, endpoint, cause } => {
                write!(f, "TEST-FAIL [{contract}::{endpoint}]: {cause}")
            }
            Self::At { stage, at, cause } => {
                write!(f, "{stage} at {}:{}: {cause}", at.file(), at.line())
            }
            Self::Check { at, expected, actual, note } => {
                write!(f, "{}:{}: expected {expected}, got {actual}", at.file(), at.line())?;
                if let Some(note) = note {
                    write!(f, " ({note})")?;
                }
                Ok(())
            }
            Self::Failed { at, cond, note } => {
                write!(f, "{}:{}: assertion failed: {cond}", at.file(), at.line())?;
                if let Some(note) = note {
                    write!(f, " ({note})")?;
                }
                Ok(())
            }
        }
    }
}

impl fmt::Debug for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl TestError {
    /// **INFRA-FAIL** — a step in a shared infrastructure module failed. Every test
    /// that goes through that module sees this, so it names the module, not the caller.
    pub fn infra(
        module: &'static str,
        stage: &'static str,
        cause: impl Into<Box<dyn StdError>>,
    ) -> Self {
        Self::Infra { module, stage, cause: cause.into() }
    }

    /// **TEST-FAIL** — a step specific to the contract under test failed.
    pub fn test(
        contract: &'static str,
        endpoint: &'static str,
        cause: impl Into<Box<dyn StdError>>,
    ) -> Self {
        Self::Test { contract, endpoint, cause: cause.into() }
    }
}

impl StdError for TestError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Infra { cause, .. } | Self::Test { cause, .. } | Self::At { cause, .. } => {
                Some(cause.as_ref())
            }
            Self::Check { .. } | Self::Failed { .. } => None,
        }
    }
}

/// Adds the stage name to a failure while propagating it.
///
/// There is deliberately no blanket `From` conversion into [`TestError`]: attribution is
/// a decision only the call site can make, so the stage is named explicitly at each
/// propagation point rather than defaulted.
pub trait Context<T> {
    /// Propagate `self`, naming `stage` as the step that failed.
    fn at(self, stage: &'static str) -> TestResult<T>;
}

impl<T, E: StdError + 'static> Context<T> for ResultGeneric<T, E> {
    #[track_caller]
    fn at(self, stage: &'static str) -> TestResult<T> {
        self.map_err(|e| TestError::At { stage, at: Location::caller(), cause: Box::new(e) })
    }
}

/// Declare this test module's INFRA-FAIL shorthand: `result.infra("stage")?`.
///
/// The module name is stated once here rather than at every propagation point, so a
/// failing shared step names both the module and the stage without the call site
/// repeating either. `Option` is covered too, because a `None` from a store lookup is
/// the same kind of failure as an `Err`.
#[macro_export]
macro_rules! infra_context {
    ($module:literal) => {
        /// Propagates a failure as this module's INFRA-FAIL, naming the stage.
        trait InfraContext<T> {
            /// INFRA-FAIL: the named stage in this module failed.
            fn infra(self, stage: &'static str) -> $crate::test_support::TestResult<T>;
        }

        // The bound is `Into<Box<dyn Error>>` rather than `Error`, so this also covers
        // the `Result<_, String>` and `Result<_, Box<dyn Error>>` returns that appear
        // in this codebase — `String` is deliberately *not* an `Error`, but it does
        // convert into one.
        impl<T, E: ::std::convert::Into<::std::boxed::Box<dyn ::std::error::Error>>>
            InfraContext<T> for ::std::result::Result<T, E>
        {
            #[track_caller]
            fn infra(self, stage: &'static str) -> $crate::test_support::TestResult<T> {
                self.map_err(|e| $crate::test_support::TestError::infra($module, stage, e))
            }
        }

        impl<T> InfraContext<T> for ::std::option::Option<T> {
            #[track_caller]
            fn infra(self, stage: &'static str) -> $crate::test_support::TestResult<T> {
                self.ok_or_else(|| {
                    $crate::test_support::TestError::infra($module, stage, "value absent")
                })
            }
        }
    };
}

/// Return `Err(TestError::Failed { .. })` naming the condition, unless it holds.
#[macro_export]
macro_rules! ensure {
    ($cond:expr $(,)?) => {
        if !($cond) {
            return ::std::result::Result::Err($crate::test_support::TestError::Failed {
                at: ::std::panic::Location::caller(),
                cond: ::std::stringify!($cond),
                note: ::std::option::Option::None,
            });
        }
    };
    ($cond:expr, $note:expr $(,)?) => {
        if !($cond) {
            return ::std::result::Result::Err($crate::test_support::TestError::Failed {
                at: ::std::panic::Location::caller(),
                cond: ::std::stringify!($cond),
                note: ::std::option::Option::Some(::std::string::ToString::to_string(&$note)),
            });
        }
    };
}

/// Return `Err(TestError::Check { .. })` naming both values, unless they are equal.
#[macro_export]
macro_rules! ensure_eq {
    ($left:expr, $right:expr $(,)?) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    return ::std::result::Result::Err($crate::test_support::TestError::Check {
                        at: ::std::panic::Location::caller(),
                        expected: ::std::format!("{:?}", right_val),
                        actual: ::std::format!("{:?}", left_val),
                        note: ::std::option::Option::None,
                    });
                }
            }
        }
    };
    ($left:expr, $right:expr, $note:expr $(,)?) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    return ::std::result::Result::Err($crate::test_support::TestError::Check {
                        at: ::std::panic::Location::caller(),
                        expected: ::std::format!("{:?}", right_val),
                        actual: ::std::format!("{:?}", left_val),
                        note: ::std::option::Option::Some(::std::string::ToString::to_string(&$note)),
                    });
                }
            }
        }
    };
}

/// Return `Err(TestError::Check { .. })` naming both values, unless they differ.
#[macro_export]
macro_rules! ensure_ne {
    ($left:expr, $right:expr $(,)?) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if *left_val == *right_val {
                    return ::std::result::Result::Err($crate::test_support::TestError::Check {
                        at: ::std::panic::Location::caller(),
                        expected: ::std::format!("anything but {:?}", right_val),
                        actual: ::std::format!("{:?}", left_val),
                        note: ::std::option::Option::None,
                    });
                }
            }
        }
    };
    ($left:expr, $right:expr, $note:expr $(,)?) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if *left_val == *right_val {
                    return ::std::result::Result::Err($crate::test_support::TestError::Check {
                        at: ::std::panic::Location::caller(),
                        expected: ::std::format!("anything but {:?}", right_val),
                        actual: ::std::format!("{:?}", left_val),
                        note: ::std::option::Option::Some(::std::string::ToString::to_string(&$note)),
                    });
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rendered message is the deliverable: libtest prints `Debug`, so these assert
    /// on the exact string a failing test shows.
    #[test]
    fn infra_fail_names_its_module_and_stage() {
        let e = TestError::Infra {
            module: "accept_block",
            stage: "verifying the hash chain",
            cause: "discontinuous".into(),
        };
        assert_eq!(
            format!("{e}"),
            "INFRA-FAIL [accept_block]: verifying the hash chain: discontinuous"
        );
        assert_eq!(format!("{e:?}"), format!("{e}"));
    }

    #[test]
    fn test_fail_names_its_contract_and_endpoint() {
        let e = TestError::Test {
            contract: "native_token",
            endpoint: "TransferV1",
            cause: "empty call_data".into(),
        };
        assert_eq!(
            format!("{e}"),
            "TEST-FAIL [native_token::TransferV1]: empty call_data"
        );
    }

    #[test]
    fn check_reports_both_values() {
        let e = TestError::Check {
            at: Location::caller(),
            expected: "20u64".to_string(),
            actual: "19u64".to_string(),
            note: None,
        };
        let rendered = format!("{e}");
        assert!(
            rendered.ends_with(": expected 20u64, got 19u64"),
            "unexpected rendering: {rendered}"
        );
        assert!(rendered.contains("test_support.rs"), "no location: {rendered}");
    }

    /// A message the author attached must survive the conversion from `assert!`.
    #[test]
    fn a_note_survives_from_assert_style_checks() {
        fn subject() -> TestResult<()> {
            ensure_eq!(1u64, 2u64, "coinbase + uncle note both persisted");
            Ok(())
        }
        let rendered = format!("{}", subject().unwrap_err());
        assert!(
            rendered.ends_with("expected 2, got 1 (coinbase + uncle note both persisted)"),
            "unexpected rendering: {rendered}"
        );
    }

    /// The per-module shorthand, which is what every converted call site uses.
    mod with_infra_context {
        use super::*;
        use crate::infra_context;

        infra_context!("tests::test_support::with_infra_context");

        fn stage_fails() -> TestResult<()> {
            let r: Result<(), std::io::Error> =
                Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such key"));
            r.infra("reading the fees tree")?;
            Ok(())
        }

        #[test]
        fn names_the_module_and_the_stage() {
            let rendered = format!("{}", stage_fails().unwrap_err());
            assert!(
                rendered.starts_with(
                    "INFRA-FAIL [tests::test_support::with_infra_context]: \
                     reading the fees tree: "
                ),
                "unexpected rendering: {rendered}"
            );
            assert!(rendered.ends_with("no such key"), "cause lost: {rendered}");
        }

        /// A `None` from a store lookup is the same kind of failure as an `Err`.
        #[test]
        fn covers_option() {
            let absent: Option<u8> = None;
            let rendered = format!("{}", absent.infra("looking up fees_db[2]").unwrap_err());
            assert!(
                rendered.starts_with(
                    "INFRA-FAIL [tests::test_support::with_infra_context]: \
                     looking up fees_db[2]: value absent"
                ),
                "unexpected rendering: {rendered}"
            );
        }
    }

    /// `?` on a foreign error names the stage, and keeps the cause.
    fn stage_propagation() -> TestResult<()> {
        let outcome: Result<(), std::io::Error> =
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"));
        outcome.at("reading keys.toml")?;
        Ok(())
    }

    #[test]
    fn at_names_the_stage_and_preserves_the_cause() {
        let e = stage_propagation().unwrap_err();
        let rendered = format!("{e}");
        assert!(
            rendered.starts_with("reading keys.toml at "),
            "unexpected rendering: {rendered}"
        );
        assert!(rendered.ends_with("no such file"), "cause lost: {rendered}");
        assert!(StdError::source(&e).is_some(), "source() must expose the cause");
    }

    #[test]
    fn ensure_eq_returns_a_typed_error_naming_both_sides() {
        fn subject() -> TestResult<()> {
            let left = 19u64;
            ensure_eq!(left, 20u64);
            Ok(())
        }
        let e = subject().unwrap_err();
        let rendered = format!("{e}");
        assert!(rendered.contains("expected 20, got 19"), "unexpected: {rendered}");
    }

    #[test]
    fn ensure_and_ensure_ne_pass_on_their_conditions() {
        fn ok() -> TestResult<()> {
            ensure!(1 + 1 == 2);
            ensure_ne!(1u64, 2u64);
            Ok(())
        }
        assert!(ok().is_ok());
    }

    #[test]
    fn ensure_names_the_condition() {
        fn subject() -> TestResult<()> {
            ensure!(1 + 1 == 3);
            Ok(())
        }
        let rendered = format!("{}", subject().unwrap_err());
        assert!(rendered.contains("assertion failed: 1 + 1 == 3"), "unexpected: {rendered}");
    }
}

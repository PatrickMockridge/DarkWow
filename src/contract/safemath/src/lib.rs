#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

/// Whether a template is executable on stock official DarkWow today.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateTrack {
    /// Uses only the stock official DarkWow `range_check(64, ...)` profile.
    Stock,
    /// Requires widened DarkWow-side range profiles beyond current stock support.
    Experimental,
}

/// Whether a template is part of the flagship public surface or a narrower helper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateSurface {
    /// Part of the primary public surface for downstream consumers.
    Public,
    /// Exposed as a helper for internal composition or advanced callers.
    Helper,
}

/// Metadata for one reusable safemath template.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Template {
    /// File name under `templates/safemath/`.
    pub file_name: &'static str,
    /// Declared `zkas` namespace / constant block name.
    pub namespace: &'static str,
    /// Embedded template source.
    pub source: &'static str,
    /// Current compatibility track for this template.
    pub track: TemplateTrack,
    /// Current public-vs-helper surface for this template.
    pub surface: TemplateSurface,
}

/// Embedded safe integer arithmetic templates encoded over `Base` witnesses.
pub mod safemath {
    use super::{Template, TemplateSurface, TemplateTrack};

    /// Stock-compatible templates that compile on official DarkWow without widened range profiles.
    ///
    /// Most downstream users targeting stock official DarkWow today should start
    /// with this module's public `CATALOG` and `template(...)` lookup.
    pub mod stock {
        use super::{Template, TemplateSurface, TemplateTrack};

        pub const ASSERT_U64_V1_ZK: &str = include_str!("../templates/safemath/assert_u64_v1.zk");
        pub const ASSERT_NONZERO_U64_V1_ZK: &str =
            include_str!("../templates/safemath/assert_nonzero_u64_v1.zk");
        pub const ASSERT_LT_U64_V1_ZK: &str =
            include_str!("../templates/safemath/assert_lt_u64_v1.zk");
        pub const ASSERT_LTE_U64_V1_ZK: &str =
            include_str!("../templates/safemath/assert_lte_u64_v1.zk");
        pub const DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK: &str =
            include_str!("../templates/safemath/div_floor_u128_by_u64_to_u64_v1.zk");
        pub const SQRT_FLOOR_U128_V1_ZK: &str =
            include_str!("../templates/safemath/sqrt_floor_u128_v1.zk");
        pub const CROSS_MUL_LTE_U64_V1_ZK: &str =
            include_str!("../templates/safemath/cross_mul_lte_u64_v1.zk");
        pub const CROSS_MUL_GTE_U64_V1_ZK: &str =
            include_str!("../templates/safemath/cross_mul_gte_u64_v1.zk");

        /// Narrow public stock v0 surface for AMM-style `u64` state and `u128` intermediates.
        pub const CATALOG: &[Template] = &[
            Template {
                file_name: "assert_u64_v1.zk",
                namespace: "SafeMath_AssertU64_V1",
                source: ASSERT_U64_V1_ZK,
                track: TemplateTrack::Stock,
                surface: TemplateSurface::Public,
            },
            Template {
                file_name: "assert_nonzero_u64_v1.zk",
                namespace: "SafeMath_AssertNonZeroU64_V1",
                source: ASSERT_NONZERO_U64_V1_ZK,
                track: TemplateTrack::Stock,
                surface: TemplateSurface::Public,
            },
            Template {
                file_name: "assert_lt_u64_v1.zk",
                namespace: "SafeMath_AssertLTU64_V1",
                source: ASSERT_LT_U64_V1_ZK,
                track: TemplateTrack::Stock,
                surface: TemplateSurface::Public,
            },
            Template {
                file_name: "assert_lte_u64_v1.zk",
                namespace: "SafeMath_AssertLTEU64_V1",
                source: ASSERT_LTE_U64_V1_ZK,
                track: TemplateTrack::Stock,
                surface: TemplateSurface::Public,
            },
            Template {
                file_name: "div_floor_u128_by_u64_to_u64_v1.zk",
                namespace: "SM_DivFloorU128ByU64ToU64_V1",
                source: DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK,
                track: TemplateTrack::Stock,
                surface: TemplateSurface::Public,
            },
            Template {
                file_name: "sqrt_floor_u128_v1.zk",
                namespace: "SM_SqrtFloorU128_V1",
                source: SQRT_FLOOR_U128_V1_ZK,
                track: TemplateTrack::Stock,
                surface: TemplateSurface::Public,
            },
            Template {
                file_name: "cross_mul_lte_u64_v1.zk",
                namespace: "SM_CrossMulLTEU64_V1",
                source: CROSS_MUL_LTE_U64_V1_ZK,
                track: TemplateTrack::Stock,
                surface: TemplateSurface::Public,
            },
            Template {
                file_name: "cross_mul_gte_u64_v1.zk",
                namespace: "SM_CrossMulGTEU64_V1",
                source: CROSS_MUL_GTE_U64_V1_ZK,
                track: TemplateTrack::Stock,
                surface: TemplateSurface::Public,
            },
        ];

        /// Stock-compatible helper templates retained for internal composition and
        /// advanced callers who explicitly need helper semantics.
        ///
        /// These are not the flagship stock v0 downstream surface.
        pub mod helpers {
            use super::{Template, TemplateSurface, TemplateTrack};

            pub const ASSERT_U128_2X64_V1_ZK: &str =
                include_str!("../templates/safemath/assert_u128_2x64_v1.zk");
            pub const ASSERT_U128_LT_2X64_V1_ZK: &str =
                include_str!("../templates/safemath/assert_u128_lt_2x64_v1.zk");
            pub const ASSERT_U128_LTE_2X64_V1_ZK: &str =
                include_str!("../templates/safemath/assert_u128_lte_2x64_v1.zk");
            pub const MIN_SELECT_U128_2X64_V1_ZK: &str =
                include_str!("../templates/safemath/min_select_u128_2x64_v1.zk");

            pub const CATALOG: &[Template] = &[
                Template {
                    file_name: "assert_u128_2x64_v1.zk",
                    namespace: "SM_AssertU128_2x64_V1",
                    source: ASSERT_U128_2X64_V1_ZK,
                    track: TemplateTrack::Stock,
                    surface: TemplateSurface::Helper,
                },
                Template {
                    file_name: "assert_u128_lt_2x64_v1.zk",
                    namespace: "SM_AssertU128LT_2x64_V1",
                    source: ASSERT_U128_LT_2X64_V1_ZK,
                    track: TemplateTrack::Stock,
                    surface: TemplateSurface::Helper,
                },
                Template {
                    file_name: "assert_u128_lte_2x64_v1.zk",
                    namespace: "SM_AssertU128LTE_2x64_V1",
                    source: ASSERT_U128_LTE_2X64_V1_ZK,
                    track: TemplateTrack::Stock,
                    surface: TemplateSurface::Helper,
                },
                Template {
                    file_name: "min_select_u128_2x64_v1.zk",
                    namespace: "SM_MinSelectU128_2x64_V1",
                    source: MIN_SELECT_U128_2X64_V1_ZK,
                    track: TemplateTrack::Stock,
                    surface: TemplateSurface::Helper,
                },
            ];

            /// Returns the embedded stock-helper template source for a packaged file name.
            pub fn template(file_name: &str) -> Option<&'static str> {
                match file_name {
                    "assert_u128_2x64_v1.zk" => Some(ASSERT_U128_2X64_V1_ZK),
                    "assert_u128_lt_2x64_v1.zk" => Some(ASSERT_U128_LT_2X64_V1_ZK),
                    "assert_u128_lte_2x64_v1.zk" => Some(ASSERT_U128_LTE_2X64_V1_ZK),
                    "min_select_u128_2x64_v1.zk" => Some(MIN_SELECT_U128_2X64_V1_ZK),
                    _ => None,
                }
            }
        }

        /// Returns the embedded public stock-compatible template source for a packaged file name.
        pub fn template(file_name: &str) -> Option<&'static str> {
            match file_name {
                "assert_u64_v1.zk" => Some(ASSERT_U64_V1_ZK),
                "assert_nonzero_u64_v1.zk" => Some(ASSERT_NONZERO_U64_V1_ZK),
                "assert_lt_u64_v1.zk" => Some(ASSERT_LT_U64_V1_ZK),
                "assert_lte_u64_v1.zk" => Some(ASSERT_LTE_U64_V1_ZK),
                "div_floor_u128_by_u64_to_u64_v1.zk" => Some(DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK),
                "sqrt_floor_u128_v1.zk" => Some(SQRT_FLOOR_U128_V1_ZK),
                "cross_mul_lte_u64_v1.zk" => Some(CROSS_MUL_LTE_U64_V1_ZK),
                "cross_mul_gte_u64_v1.zk" => Some(CROSS_MUL_GTE_U64_V1_ZK),
                _ => None,
            }
        }
    }

    /// Wider templates retained for future DarkWow cores that support 126/128/252-bit range profiles.
    ///
    /// This module is intentionally non-stock and should not be the default
    /// downstream dependency surface today.
    pub mod experimental {
        use super::{Template, TemplateSurface, TemplateTrack};

        pub const ASSERT_U128_V1_ZK: &str = include_str!("../templates/safemath/assert_u128_v1.zk");
        pub const DIV_FLOOR_V1_ZK: &str = include_str!("../templates/safemath/div_floor_v1.zk");
        pub const SQRT_FLOOR_V1_ZK: &str = include_str!("../templates/safemath/sqrt_floor_v1.zk");
        pub const MIN_SELECT_V1_ZK: &str = include_str!("../templates/safemath/min_select_v1.zk");
        pub const RATIO_LTE_V1_ZK: &str = include_str!("../templates/safemath/ratio_lte_v1.zk");

        /// Complete widened safemath template catalog.
        pub const CATALOG: &[Template] = &[
            Template {
                file_name: "assert_u128_v1.zk",
                namespace: "SafeMath_AssertU128_V1",
                source: ASSERT_U128_V1_ZK,
                track: TemplateTrack::Experimental,
                surface: TemplateSurface::Public,
            },
            Template {
                file_name: "div_floor_v1.zk",
                namespace: "SafeMath_DivFloor_V1",
                source: DIV_FLOOR_V1_ZK,
                track: TemplateTrack::Experimental,
                surface: TemplateSurface::Public,
            },
            Template {
                file_name: "sqrt_floor_v1.zk",
                namespace: "SafeMath_SqrtFloor_V1",
                source: SQRT_FLOOR_V1_ZK,
                track: TemplateTrack::Experimental,
                surface: TemplateSurface::Public,
            },
            Template {
                file_name: "min_select_v1.zk",
                namespace: "SafeMath_MinSelect_V1",
                source: MIN_SELECT_V1_ZK,
                track: TemplateTrack::Experimental,
                surface: TemplateSurface::Public,
            },
            Template {
                file_name: "ratio_lte_v1.zk",
                namespace: "SafeMath_RatioLTE_V1",
                source: RATIO_LTE_V1_ZK,
                track: TemplateTrack::Experimental,
                surface: TemplateSurface::Public,
            },
        ];

        /// Returns the embedded experimental template source for a packaged file name.
        pub fn template(file_name: &str) -> Option<&'static str> {
            match file_name {
                "assert_u128_v1.zk" => Some(ASSERT_U128_V1_ZK),
                "div_floor_v1.zk" => Some(DIV_FLOOR_V1_ZK),
                "sqrt_floor_v1.zk" => Some(SQRT_FLOOR_V1_ZK),
                "min_select_v1.zk" => Some(MIN_SELECT_V1_ZK),
                "ratio_lte_v1.zk" => Some(RATIO_LTE_V1_ZK),
                _ => None,
            }
        }
    }

    /// Complete public template catalog across stock and experimental tracks.
    ///
    /// This is intentionally broader than the stock v0 downstream surface.
    /// Downstream code that targets stock official DarkWow today should usually
    /// prefer `safemath::stock::CATALOG` instead.
    pub const CATALOG: &[Template] = &[
        Template {
            file_name: "assert_u64_v1.zk",
            namespace: "SafeMath_AssertU64_V1",
            source: stock::ASSERT_U64_V1_ZK,
            track: TemplateTrack::Stock,
            surface: TemplateSurface::Public,
        },
        Template {
            file_name: "assert_nonzero_u64_v1.zk",
            namespace: "SafeMath_AssertNonZeroU64_V1",
            source: stock::ASSERT_NONZERO_U64_V1_ZK,
            track: TemplateTrack::Stock,
            surface: TemplateSurface::Public,
        },
        Template {
            file_name: "assert_lt_u64_v1.zk",
            namespace: "SafeMath_AssertLTU64_V1",
            source: stock::ASSERT_LT_U64_V1_ZK,
            track: TemplateTrack::Stock,
            surface: TemplateSurface::Public,
        },
        Template {
            file_name: "assert_lte_u64_v1.zk",
            namespace: "SafeMath_AssertLTEU64_V1",
            source: stock::ASSERT_LTE_U64_V1_ZK,
            track: TemplateTrack::Stock,
            surface: TemplateSurface::Public,
        },
        Template {
            file_name: "div_floor_u128_by_u64_to_u64_v1.zk",
            namespace: "SM_DivFloorU128ByU64ToU64_V1",
            source: stock::DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK,
            track: TemplateTrack::Stock,
            surface: TemplateSurface::Public,
        },
        Template {
            file_name: "sqrt_floor_u128_v1.zk",
            namespace: "SM_SqrtFloorU128_V1",
            source: stock::SQRT_FLOOR_U128_V1_ZK,
            track: TemplateTrack::Stock,
            surface: TemplateSurface::Public,
        },
        Template {
            file_name: "cross_mul_lte_u64_v1.zk",
            namespace: "SM_CrossMulLTEU64_V1",
            source: stock::CROSS_MUL_LTE_U64_V1_ZK,
            track: TemplateTrack::Stock,
            surface: TemplateSurface::Public,
        },
        Template {
            file_name: "cross_mul_gte_u64_v1.zk",
            namespace: "SM_CrossMulGTEU64_V1",
            source: stock::CROSS_MUL_GTE_U64_V1_ZK,
            track: TemplateTrack::Stock,
            surface: TemplateSurface::Public,
        },
        Template {
            file_name: "assert_u128_v1.zk",
            namespace: "SafeMath_AssertU128_V1",
            source: experimental::ASSERT_U128_V1_ZK,
            track: TemplateTrack::Experimental,
            surface: TemplateSurface::Public,
        },
        Template {
            file_name: "div_floor_v1.zk",
            namespace: "SafeMath_DivFloor_V1",
            source: experimental::DIV_FLOOR_V1_ZK,
            track: TemplateTrack::Experimental,
            surface: TemplateSurface::Public,
        },
        Template {
            file_name: "sqrt_floor_v1.zk",
            namespace: "SafeMath_SqrtFloor_V1",
            source: experimental::SQRT_FLOOR_V1_ZK,
            track: TemplateTrack::Experimental,
            surface: TemplateSurface::Public,
        },
        Template {
            file_name: "min_select_v1.zk",
            namespace: "SafeMath_MinSelect_V1",
            source: experimental::MIN_SELECT_V1_ZK,
            track: TemplateTrack::Experimental,
            surface: TemplateSurface::Public,
        },
        Template {
            file_name: "ratio_lte_v1.zk",
            namespace: "SafeMath_RatioLTE_V1",
            source: experimental::RATIO_LTE_V1_ZK,
            track: TemplateTrack::Experimental,
            surface: TemplateSurface::Public,
        },
    ];

    /// Complete template catalog including helper-only stock templates.
    ///
    /// This is primarily useful for introspection and internal tooling, not as a
    /// default downstream dependency surface.
    pub const ALL_TEMPLATES: &[Template] = &[
        CATALOG[0],
        CATALOG[1],
        CATALOG[2],
        CATALOG[3],
        CATALOG[4],
        CATALOG[5],
        CATALOG[6],
        CATALOG[7],
        Template {
            file_name: "assert_u128_2x64_v1.zk",
            namespace: "SM_AssertU128_2x64_V1",
            source: stock::helpers::ASSERT_U128_2X64_V1_ZK,
            track: TemplateTrack::Stock,
            surface: TemplateSurface::Helper,
        },
        Template {
            file_name: "assert_u128_lt_2x64_v1.zk",
            namespace: "SM_AssertU128LT_2x64_V1",
            source: stock::helpers::ASSERT_U128_LT_2X64_V1_ZK,
            track: TemplateTrack::Stock,
            surface: TemplateSurface::Helper,
        },
        Template {
            file_name: "assert_u128_lte_2x64_v1.zk",
            namespace: "SM_AssertU128LTE_2x64_V1",
            source: stock::helpers::ASSERT_U128_LTE_2X64_V1_ZK,
            track: TemplateTrack::Stock,
            surface: TemplateSurface::Helper,
        },
        Template {
            file_name: "min_select_u128_2x64_v1.zk",
            namespace: "SM_MinSelectU128_2x64_V1",
            source: stock::helpers::MIN_SELECT_U128_2X64_V1_ZK,
            track: TemplateTrack::Stock,
            surface: TemplateSurface::Helper,
        },
        CATALOG[8],
        CATALOG[9],
        CATALOG[10],
        CATALOG[11],
        CATALOG[12],
    ];

    /// Returns the embedded public template source for a packaged file name.
    ///
    /// Downstream stock users should usually prefer `safemath::stock::template`
    /// so they do not accidentally mix stock and experimental templates.
    pub fn template(file_name: &str) -> Option<&'static str> {
        stock::template(file_name).or_else(|| experimental::template(file_name))
    }
}

/// Host-side integer relations that mirror the safemath template semantics.
///
/// The most direct host mirrors of the stock public template surface are:
///
/// - `floor_div_u128_by_u64_to_u64`
/// - `floor_sqrt_u128_to_u64`
/// - `cross_mul_lte_u64`
/// - `cross_mul_gte_u64`
///
/// Broader helpers such as `floor_div_u128`, `min_u128`, or the generic
/// `cross_mul_*_u128` functions are still useful host utilities, but they do not
/// imply matching stock `.zk` guarantees by name alone.
pub mod host {
    use std::{error::Error, fmt};

    /// Little-endian `u128` decomposition into two `u64` limbs.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct U128Limbs {
        pub lo: u64,
        pub hi: u64,
    }

    /// Errors returned by host-side arithmetic helpers.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum MathError {
        /// The denominator for a division relation was zero.
        DivisionByZero(&'static str),
        /// An intermediate arithmetic product or sum overflowed `u128` or `u64`.
        Overflow(&'static str),
        /// A conversion to `u64` exceeded the representable range.
        ConversionOverflow(&'static str),
    }

    impl fmt::Display for MathError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::DivisionByZero(label) => write!(f, "{label} division by zero"),
                Self::Overflow(label) => write!(f, "{label} overflow"),
                Self::ConversionOverflow(label) => write!(f, "{label} exceeds u64 range"),
            }
        }
    }

    impl Error for MathError {}

    /// Splits a `u128` into little-endian `(lo, hi)` `u64` limbs.
    pub fn split_u128(value: u128) -> U128Limbs {
        U128Limbs {
            lo: value as u64,
            hi: (value >> 64) as u64,
        }
    }

    /// Recombines a little-endian `(lo, hi)` pair into a `u128`.
    pub fn join_u128(limbs: U128Limbs) -> u128 {
        u128::from(limbs.lo) | (u128::from(limbs.hi) << 64)
    }

    /// Computes `floor(numerator / denominator)` for `u128` inputs.
    pub fn floor_div_u128(numerator: u128, denominator: u128) -> Result<u128, MathError> {
        if denominator == 0 {
            return Err(MathError::DivisionByZero("floor_div_u128"));
        }

        Ok(numerator / denominator)
    }

    /// Computes `floor(numerator / denominator)` and narrows the quotient to `u64`.
    pub fn floor_div_u128_by_u64_to_u64(
        numerator: u128,
        denominator: u64,
    ) -> Result<u64, MathError> {
        let quotient = floor_div_u128(numerator, u128::from(denominator))?;
        u128_to_u64(quotient, "floor_div_u128_by_u64_to_u64")
    }

    /// Computes the largest integer `r` such that `r^2 <= value`.
    pub fn floor_sqrt_u128(value: u128) -> u128 {
        if value < 2 {
            return value;
        }

        let mut lo = 1_u128;
        let mut hi = 1_u128 << 64;

        while lo < hi {
            let mid = lo + (hi - lo + 1) / 2;
            if mid <= value / mid {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }

        lo
    }

    /// Computes `floor(sqrt(value))`, returned as an exact `u64`.
    pub fn floor_sqrt_u128_to_u64(value: u128) -> u64 {
        floor_sqrt_u128(value) as u64
    }

    /// Returns the smaller of two `u128` values.
    pub fn min_u128(lhs: u128, rhs: u128) -> u128 {
        lhs.min(rhs)
    }

    /// Converts a checked `u128` quantity into `u64`.
    pub fn u128_to_u64(value: u128, label: &'static str) -> Result<u64, MathError> {
        u64::try_from(value).map_err(|_| MathError::ConversionOverflow(label))
    }

    /// Adds two `u64` values while surfacing overflow explicitly.
    pub fn add_u64(lhs: u64, rhs: u64, label: &'static str) -> Result<u64, MathError> {
        lhs.checked_add(rhs).ok_or(MathError::Overflow(label))
    }

    /// Compares `lhs_num * lhs_mul <= rhs_num * rhs_mul` with checked products.
    pub fn cross_mul_lte_u128(
        lhs_num: u128,
        lhs_mul: u128,
        rhs_num: u128,
        rhs_mul: u128,
    ) -> Result<bool, MathError> {
        let lhs = lhs_num
            .checked_mul(lhs_mul)
            .ok_or(MathError::Overflow("cross_mul_lte lhs"))?;
        let rhs = rhs_num
            .checked_mul(rhs_mul)
            .ok_or(MathError::Overflow("cross_mul_lte rhs"))?;
        Ok(lhs <= rhs)
    }

    /// Compares `lhs_num * lhs_mul >= rhs_num * rhs_mul` with checked products.
    pub fn cross_mul_gte_u128(
        lhs_num: u128,
        lhs_mul: u128,
        rhs_num: u128,
        rhs_mul: u128,
    ) -> Result<bool, MathError> {
        let lhs = lhs_num
            .checked_mul(lhs_mul)
            .ok_or(MathError::Overflow("cross_mul_gte lhs"))?;
        let rhs = rhs_num
            .checked_mul(rhs_mul)
            .ok_or(MathError::Overflow("cross_mul_gte rhs"))?;
        Ok(lhs >= rhs)
    }

    /// Compares `lhs_num / lhs_den <= rhs_num / rhs_den` for `u64` inputs.
    pub fn cross_mul_lte_u64(
        lhs_num: u64,
        lhs_den: u64,
        rhs_num: u64,
        rhs_den: u64,
    ) -> Result<bool, MathError> {
        cross_mul_lte_u128(
            u128::from(lhs_num),
            u128::from(rhs_den),
            u128::from(rhs_num),
            u128::from(lhs_den),
        )
    }

    /// Compares `lhs_num / lhs_den >= rhs_num / rhs_den` for `u64` inputs.
    pub fn cross_mul_gte_u64(
        lhs_num: u64,
        lhs_den: u64,
        rhs_num: u64,
        rhs_den: u64,
    ) -> Result<bool, MathError> {
        cross_mul_gte_u128(
            u128::from(lhs_num),
            u128::from(rhs_den),
            u128::from(rhs_num),
            u128::from(lhs_den),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::host::{
        add_u64, cross_mul_gte_u64, cross_mul_lte_u64, floor_div_u128,
        floor_div_u128_by_u64_to_u64, floor_sqrt_u128, floor_sqrt_u128_to_u64, join_u128, min_u128,
        split_u128, u128_to_u64, MathError, U128Limbs,
    };
    use super::safemath::{experimental, stock, template, ALL_TEMPLATES, CATALOG};
    use super::{TemplateSurface, TemplateTrack};

    #[test]
    fn safemath_catalog_splits_into_public_stock_helper_and_experimental_tracks() {
        assert_eq!(stock::CATALOG.len(), 8);
        assert_eq!(stock::helpers::CATALOG.len(), 4);
        assert_eq!(experimental::CATALOG.len(), 5);
        assert_eq!(CATALOG.len(), 13);
        assert_eq!(ALL_TEMPLATES.len(), 17);
        assert!(CATALOG.iter().any(|item| {
            item.track == TemplateTrack::Stock
                && item.surface == TemplateSurface::Public
                && item.file_name == "cross_mul_lte_u64_v1.zk"
        }));
        assert!(stock::helpers::CATALOG.iter().any(|item| {
            item.track == TemplateTrack::Stock
                && item.surface == TemplateSurface::Helper
                && item.file_name == "min_select_u128_2x64_v1.zk"
        }));
        assert!(CATALOG.iter().any(|item| {
            item.track == TemplateTrack::Experimental && item.file_name == "div_floor_v1.zk"
        }));
    }

    #[test]
    fn template_lookup_returns_known_entries() {
        assert_eq!(
            stock::template("div_floor_u128_by_u64_to_u64_v1.zk"),
            Some(stock::DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK)
        );
        assert_eq!(
            stock::helpers::template("assert_u128_lte_2x64_v1.zk"),
            Some(stock::helpers::ASSERT_U128_LTE_2X64_V1_ZK)
        );
        assert_eq!(
            experimental::template("sqrt_floor_v1.zk"),
            Some(experimental::SQRT_FLOOR_V1_ZK)
        );
        assert_eq!(
            template("cross_mul_lte_u64_v1.zk"),
            Some(stock::CROSS_MUL_LTE_U64_V1_ZK)
        );
        assert_eq!(
            template("ratio_lte_v1.zk"),
            Some(experimental::RATIO_LTE_V1_ZK)
        );
        assert!(template("min_select_u128_2x64_v1.zk").is_none());
        assert!(template("missing.zk").is_none());
    }

    #[test]
    fn stock_public_and_helper_templates_avoid_widened_range_profiles() {
        assert!(
            stock::DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK.contains("range_check(64, numerator_lo);")
        );
        assert!(stock::SQRT_FLOOR_U128_V1_ZK.contains("range_check(64, radicand_hi);"));
        assert!(stock::CROSS_MUL_LTE_U64_V1_ZK.contains("range_check(64, lhs_num);"));
        assert!(stock::helpers::ASSERT_U128_LTE_2X64_V1_ZK.contains("range_check(64, rhs_hi);"));
        assert!(stock::helpers::MIN_SELECT_U128_2X64_V1_ZK.contains("bool_check(choose_lhs);"));
        assert!(!stock::DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK.contains("range_check(128"));
        assert!(!stock::SQRT_FLOOR_U128_V1_ZK.contains("range_check(126"));
        assert!(!stock::helpers::MIN_SELECT_U128_2X64_V1_ZK.contains("range_check(252"));
    }

    #[test]
    fn experimental_templates_keep_documented_wide_profiles() {
        assert!(experimental::DIV_FLOOR_V1_ZK.contains("range_check(128, numerator);"));
        assert!(experimental::SQRT_FLOOR_V1_ZK.contains("range_check(252, radicand);"));
        assert!(experimental::MIN_SELECT_V1_ZK.contains("range_check(252, lhs);"));
        assert!(stock::ASSERT_LTE_U64_V1_ZK.contains("rhs_plus_one = base_add(rhs, ONE);"));
        assert!(experimental::RATIO_LTE_V1_ZK.contains("range_check(126, lhs_num);"));
        assert!(experimental::RATIO_LTE_V1_ZK
            .contains("less_than_strict(lhs_cross, rhs_cross_plus_one);"));
    }

    #[test]
    fn host_floor_relations_match_expected_vectors() {
        assert_eq!(floor_div_u128(987_654, 10).unwrap(), 98_765);
        assert_eq!(floor_div_u128_by_u64_to_u64(987_654, 10).unwrap(), 98_765);
        assert_eq!(floor_sqrt_u128(97_408_265_472), 312_102);
        assert_eq!(floor_sqrt_u128_to_u64(97_408_265_472), 312_102);
        assert_eq!(floor_sqrt_u128(312_102_u128 * 312_102_u128), 312_102);
        assert_eq!(min_u128(3_086_419, 4_115_226), 3_086_419);
    }

    #[test]
    fn host_cross_multiply_relations_cover_fee_and_price_checks() {
        assert!(cross_mul_lte_u64(30, 500, 1_000, 10_000).unwrap());
        assert!(!cross_mul_lte_u64(60, 500, 1_000, 10_000).unwrap());

        assert!(cross_mul_gte_u64(4_935, 10_000, 0, 1).unwrap());
        assert!(cross_mul_gte_u64(1_000, 1, 4_000, 5).unwrap());
        assert!(!cross_mul_gte_u64(999, 1, 5_000, 5).unwrap());
    }

    #[test]
    fn split_and_join_u128_round_trip() {
        let limbs = split_u128((u128::from(u64::MAX) << 64) | 42);
        assert_eq!(
            limbs,
            U128Limbs {
                lo: 42,
                hi: u64::MAX,
            }
        );
        assert_eq!(join_u128(limbs), (u128::from(u64::MAX) << 64) | 42);
    }

    #[test]
    fn host_conversions_and_overflow_checks_are_explicit() {
        assert_eq!(u128_to_u64(u64::MAX as u128, "ok").unwrap(), u64::MAX);
        assert_eq!(add_u64(40, 2, "sum").unwrap(), 42);
        assert_eq!(
            floor_div_u128(1, 0).unwrap_err(),
            MathError::DivisionByZero("floor_div_u128")
        );
        assert_eq!(
            u128_to_u64(u64::MAX as u128 + 1, "too_big").unwrap_err(),
            MathError::ConversionOverflow("too_big")
        );
        assert_eq!(
            add_u64(u64::MAX, 1, "sum").unwrap_err(),
            MathError::Overflow("sum")
        );
    }
}

//! Shared spec helpers — import by specs that follow the standard EndpointSpec pattern.
//! Extracted per RG-MODULAR: used by 6+ spec files.
use crate::tests::uniform_runner::{EndpointSpec, EndpointExpectation, EndpointResult};

/// Standard mk_ep helper for simple endpoints (no coinbase coordination, no verify_state).
pub fn mk_ep(
    name: &'static str,
    is_zk: bool,
    generate: Box<dyn Fn() -> dwow_core::Result<EndpointResult> + 'static>,
) -> EndpointSpec<'static> {
    EndpointSpec {
        name,
        is_zk,
        expectation: EndpointExpectation::Success,
        generate_with_coinbase: None,
        verify_state: None,
        generate,
    }
}

/// `mk_ep` for a row that expects a rejection — with the check named wherever it is known.
///
/// A bare `EndpointExpectation::Rejection` is satisfied by *any* earlier failure in the frame, so a row
/// that exists to pin one check and asserts only `Rejection` is a control that cannot fail. Pass the
/// needles when the expected failure is known; pass `&[]` only when it genuinely is not, and say so in
/// the row's comment.
pub fn mk_ep_rejecting(
    name: &'static str,
    is_zk: bool,
    needles: &'static [&'static str],
    generate: Box<dyn Fn() -> dwow_core::Result<EndpointResult> + 'static>,
) -> EndpointSpec<'static> {
    EndpointSpec {
        name,
        is_zk,
        expectation: if needles.is_empty() {
            EndpointExpectation::Rejection
        } else {
            EndpointExpectation::RejectionNaming(needles)
        },
        generate_with_coinbase: None,
        verify_state: None,
        generate,
    }
}

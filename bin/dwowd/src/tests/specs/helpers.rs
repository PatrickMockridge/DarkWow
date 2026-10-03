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

/// `mk_ep` for a rejecting row whose refusal a **stage or reason** names, without asserting which
/// contract refused (`EndpointExpectation::RejectionNaming`).
///
/// This is the weakest rejecting variant, and the honest home for a row whose refusal is not
/// attributable to one contract id — a row refused before any contract runs, or a placeholder that
/// names the stage it stops at. A needle is satisfied by *any* call in the frame, so a row whose
/// refuser is determinable should construct its `EndpointSpec` with `RejectionByEndpoint` or
/// `RejectionByChild` explicitly (those rows carry their own `verify_state`, so they do not use a
/// helper). **The needle slice must be non-empty** — a row that names nothing is the bare "any
/// failure will do" shape, and `ContractTestSpec::validate()` refuses it.
pub fn mk_ep_rejecting_naming(
    name: &'static str,
    is_zk: bool,
    needles: &'static [&'static str],
    generate: Box<dyn Fn() -> dwow_core::Result<EndpointResult> + 'static>,
) -> EndpointSpec<'static> {
    EndpointSpec {
        name,
        is_zk,
        expectation: EndpointExpectation::RejectionNaming(needles),
        generate_with_coinbase: None,
        verify_state: None,
        generate,
    }
}

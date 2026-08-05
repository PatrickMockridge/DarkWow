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
        state_tree: "nullifiers",
        state_key_fn: Box::new(|| vec![]),
        generate,
    }
}

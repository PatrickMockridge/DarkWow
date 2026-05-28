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

use dwow_deployooor_contract::DeployFunction;
use dwow_promissory_note_contract::PromissoryNoteFunction;
use dwow_native_token_contract::NativeTokenFunction;
use dwow_sdk::{
    crypto::{DEPLOYOOOR_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID},
    dark_tree, tx,
};
use pyo3::{
    exceptions::PyValueError,
    prelude::{PyModule, PyModuleMethods},
    pyclass, pymethods,
    types::PyDict,
    Bound, Py, PyResult, Python,
};

/// Promissory Note contract definitions (DeFi tokens)
pub mod promissory;
pub use promissory::decode_promissory_function_params;

/// NativeToken contract definitions (fees, PoW rewards)
pub mod native_token;
pub use native_token::decode_native_token_function_params;

/// Deployooor contract definitions
pub mod deployooor;
pub use deployooor::decode_deployooor_function_params;

/// Trait for working with contract function call parameters.
pub trait FunctionParams {
    /// Converts the parameters to a Python dictionary.
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>>;

    /// Appends a formatted, pretty-printed representation of the parameters
    /// to the given string buffer.
    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()>;
}

/// Generates boilerplate pymethods shared by contract call function parameters
#[macro_export]
macro_rules! impl_py_methods {
    ($name: ident) => {
        #[pyo3::pymethods]
        impl $name {
            #[getter]
            pub fn __dict__(&self, py: Python) -> PyResult<Py<PyDict>> {
                self.0.to_pydict(py)
            }

            pub fn __str__(&self) -> PyResult<String> {
                let mut out = String::new();
                self.0.fmt_pretty(&mut out, 0)?;
                Ok(out)
            }
        }
    };
}
pub use impl_py_methods;

/// A class representing a contract call leaf node within a
/// transaction's call tree.
#[pyclass]
pub struct DarkLeafContractCall(pub dark_tree::DarkLeaf<tx::ContractCall>);

#[pymethods]
impl DarkLeafContractCall {
    pub fn data(&self) -> ContractCall {
        ContractCall(self.0.data.clone())
    }

    pub fn parent_index(&self) -> Option<usize> {
        self.0.parent_index
    }

    pub fn children_indexes(&self) -> Vec<usize> {
        self.0.children_indexes.clone()
    }
}

/// A class representing a contract function call.
#[pyclass]
pub struct ContractCall(pub tx::ContractCall);

#[pymethods]
impl ContractCall {
    pub fn contract_id(&self) -> String {
        self.0.contract_id.to_string()
    }

    /// Name of the contract being invoked.
    pub fn contract_name(&self) -> Option<String> {
        if self.0.contract_id == *NATIVE_TOKEN_CONTRACT_ID {
            Some("NativeToken".to_string())
        } else if self.0.contract_id == *DEPLOYOOOR_CONTRACT_ID {
            Some("Deployooor".to_string())
        } else {
            None
        }
    }

    pub fn data(&self) -> Vec<u8> {
        self.0.data.clone()
    }

    pub fn function_index(&self) -> u8 {
        self.0.data[0]
    }

    /// Name of the contract function being invoked.
    pub fn function_name(&self) -> Option<String> {
        match self.contract_name().as_deref() {
            Some("NativeToken") => {
                NativeTokenFunction::try_from(self.function_index()).map(|f| format!("{f:?}")).ok()
            }
            Some("PromissoryNote") => {
                PromissoryNoteFunction::try_from(self.function_index()).map(|f| format!("{f:?}")).ok()
            }
            Some("Deployooor") => {
                DeployFunction::try_from(self.function_index()).map(|f| format!("{f:?}")).ok()
            }
            _ => None,
        }
    }

    /// Represents the parameters of a contract function call as a Python dictionary.
    pub fn function_params_dict(&self, py: Python) -> PyResult<Py<PyDict>> {
        match self.contract_name().as_deref() {
            Some("NativeToken") => {
                decode_native_token_function_params(self.function_index(), &self.0.data)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?
                    .to_pydict(py)
            }
            Some("PromissoryNote") => decode_promissory_function_params(self.function_index(), &self.0.data)
                .map_err(|e| PyValueError::new_err(e.to_string()))?
                .to_pydict(py),
            Some("Deployooor") => {
                decode_deployooor_function_params(self.function_index(), &self.0.data)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?
                    .to_pydict(py)
            }
            //TODO: Add support for custom contracts
            _ => Err(PyValueError::new_err("Unknown Contract")),
        }
    }

    /// Formatted string of the parameters passed to a contract function call.
    pub fn function_params_str(&self, depth: usize) -> PyResult<String> {
        let mut output = String::new();
        match self.contract_name().as_deref() {
            Some("NativeToken") => {
                decode_native_token_function_params(self.function_index(), &self.0.data)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?
                    .fmt_pretty(&mut output, depth)?
            }
            Some("PromissoryNote") => decode_promissory_function_params(self.function_index(), &self.0.data)
                .map_err(|e| PyValueError::new_err(e.to_string()))?
                .fmt_pretty(&mut output, depth)?,
            Some("Deployooor") => {
                decode_deployooor_function_params(self.function_index(), &self.0.data)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?
                    .fmt_pretty(&mut output, depth)?
            }
            //TODO: Add support for custom contracts
            _ => Err(PyValueError::new_err("Unknown Contract"))?,
        }
        Ok(output)
    }
}

/// Create contract module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "contract")?;

    submod.add_class::<DarkLeafContractCall>()?;
    submod.add_class::<ContractCall>()?;

    // native_token, promissory, deployooor submodules will be inside contract submodule
    let native_token_submodule = native_token::create_module(py)?;
    let promissory_submodule = promissory::create_module(py)?;
    let deployooor_submodule = deployooor::create_module(py)?;

    submod.add_submodule(&native_token_submodule)?;
    submod.add_submodule(&promissory_submodule)?;
    submod.add_submodule(&deployooor_submodule)?;

    Ok(submod)
}
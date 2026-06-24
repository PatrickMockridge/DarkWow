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

use std::{
    collections::{HashMap, HashSet},
    io::{stdin, Read},
    str::FromStr,
};

use smol::channel::Sender;

use dwow_core::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    zk::Proof,
};
use crate::wallet_error::{Error, Result};
use crate::wallet_util::{base64_decode, decode_base10};
use crate::contract_imports::promissory_note::TokenId;
use dwow_sdk::{
    crypto::{
        keypair::Address,
        pasta_prelude::PrimeField,
        FuncId, SecretKey,
    },
    dark_tree::DarkTree,
    pasta::pallas,
    ContractCallImport,
};
use dwow_serial::deserialize_async;

use crate::{contract_imports::promissory_note::BALANCE_BASE10_DECIMALS, Dww};

/// Auxiliary function to parse a base64 encoded transaction from stdin.
pub async fn parse_tx_from_stdin() -> Result<Transaction> {
    let mut buf = String::new();
    stdin().read_to_string(&mut buf)?;
    match base64_decode(buf.trim()) {
        Some(bytes) => Ok(deserialize_async(&bytes).await?),
        None => Err(Error::ParseFailed("Failed to decode transaction")),
    }
}

/// Auxiliary function to parse a base64 encoded transaction from
/// provided input or fallback to stdin if its empty.
pub async fn parse_tx_from_input(input: &[String]) -> Result<Transaction> {
    match input.len() {
        0 => parse_tx_from_stdin().await,
        1 => match base64_decode(input[0].trim()) {
            Some(bytes) => Ok(deserialize_async(&bytes).await?),
            None => Err(Error::ParseFailed("Failed to decode transaction")),
        },
        _ => Err(Error::ParseFailed("Multiline input provided")),
    }
}

/// Auxiliary function to parse base64 encoded contract calls from stdin.
pub async fn parse_calls_from_stdin() -> Result<Vec<ContractCallImport>> {
    let lines = stdin().lines();
    let mut calls = vec![];
    for line in lines {
        let Some(line) = base64_decode(&line?) else {
            return Err(Error::ParseFailed("Failed to decode base64"))
        };
        calls.push(deserialize_async(&line).await?);
    }
    Ok(calls)
}

/// Auxiliary function to parse base64 encoded contract calls from
/// provided input or fallback to stdin if its empty.
pub async fn parse_calls_from_input(input: &[String]) -> Result<Vec<ContractCallImport>> {
    if input.is_empty() {
        return parse_calls_from_stdin().await
    }

    let mut calls = vec![];
    for line in input {
        let Some(line) = base64_decode(line) else {
            return Err(Error::ParseFailed("Failed to decode base64"))
        };
        calls.push(deserialize_async(&line).await?);
    }
    Ok(calls)
}

/// Auxiliary function to parse provided string into a values pair.
pub fn parse_value_pair(s: &str) -> Result<(u64, u64)> {
    let v: Vec<&str> = s.split(':').collect();
    if v.len() != 2 {
        return Err(Error::ParseFailed("Invalid value pair. Use a pair such as 13.37:11.0"))
    }

    let val0 = decode_base10(v[0], BALANCE_BASE10_DECIMALS, true);
    let val1 = decode_base10(v[1], BALANCE_BASE10_DECIMALS, true);

    if val0.is_err() || val1.is_err() {
        return Err(Error::ParseFailed("Invalid value pair. Use a pair such as 13.37:11.0"))
    }

    Ok((val0.unwrap(), val1.unwrap()))
}

/// Auxiliary function to parse provided string into a tokens pair.
pub async fn parse_token_pair(drk: &Dww, s: &str) -> Result<(TokenId, TokenId)> {
    let v: Vec<&str> = s.split(':').collect();
    if v.len() != 2 {
        return Err(Error::ParseFailed(
            "Invalid token pair. Use a pair such as:\nWCKD:MLDY\nor\n\
            A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd:FCuoMii64H5Ee4eVWBjP18WTFS8iLUJmGi16Qti1xFQ2"
        ))
    }

    let tok0 = drk.get_token(v[0].to_string());
    let tok1 = drk.get_token(v[1].to_string());

    if tok0.is_err() || tok1.is_err() {
        return Err(Error::ParseFailed(
            "Invalid token pair. Use a pair such as:\nWCKD:MLDY\nor\n\
            A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd:FCuoMii64H5Ee4eVWBjP18WTFS8iLUJmGi16Qti1xFQ2"
        ))
    }

    Ok((tok0.unwrap(), tok1.unwrap()))
}

pub fn print_output(buf: &[String]) {
    for line in buf {
        println!("{line}");
    }
}

/// Auxiliary function to print or insert provided messages to given
/// buffer reference. If a channel sender is provided, the messages
/// are send to that instead.
pub async fn append_or_print(
    buf: &mut Vec<String>,
    sender: Option<&Sender<Vec<String>>>,
    print: &bool,
    messages: Vec<String>,
) {
    // Send the messages to the channel, if provided
    if let Some(sender) = sender {
        if let Err(e) = sender.send(messages).await {
            let err_msg = format!("[append_or_print] Sending messages to channel failed: {e}");
            if *print {
                println!("{err_msg}");
            } else {
                buf.push(err_msg);
            }
        }
        return
    }

    // Print the messages
    if *print {
        for msg in messages {
            println!("{msg}");
        }
        return
    }

    // Insert the messages in the buffer
    for msg in messages {
        buf.push(msg);
    }
}

/// Auxiliary function to parse a base64 encoded mining configuration
/// from stdin.
pub async fn parse_mining_config_from_stdin(
) -> Result<(String, String, Option<String>, Option<String>)> {
    let mut buf = String::new();
    stdin().read_to_string(&mut buf)?;
    let config = buf.trim();
    let (recipient, spend_hook, user_data) = match base64_decode(config) {
        Some(bytes) => deserialize_async(&bytes).await?,
        None => return Err(Error::ParseFailed("Failed to decode mining configuration")),
    };
    Ok((config.to_string(), recipient, spend_hook, user_data))
}

/// Auxiliary function to parse a base64 encoded mining configuration
/// from provided input or fallback to stdin if its empty.
pub async fn parse_mining_config_from_input(
    input: &[String],
) -> Result<(String, String, Option<String>, Option<String>)> {
    match input.len() {
        0 => parse_mining_config_from_stdin().await,
        1 => {
            let config = input[0].trim();
            let (recipient, spend_hook, user_data) = match base64_decode(config) {
                Some(bytes) => deserialize_async(&bytes).await?,
                None => return Err(Error::ParseFailed("Failed to decode mining configuration")),
            };
            Ok((config.to_string(), recipient, spend_hook, user_data))
        }
        _ => Err(Error::ParseFailed("Multiline input provided")),
    }
}

/// Auxiliary function to display the parts of a mining configuration.
pub fn display_mining_config(
    config: &str,
    recipient_str: &str,
    spend_hook: &Option<String>,
    user_data: &Option<String>,
    output: &mut Vec<String>,
) {
    output.push(format!("DarkWow mining configuration address: {config}"));

    match Address::from_str(recipient_str) {
        Ok(recipient) => {
            output.push(format!("Recipient: {recipient_str}"));
            output.push(format!("Public key: {}", recipient.public_key()));
            output.push(format!("Network: {:?}", recipient.network()));
        }
        Err(e) => output.push(format!("Recipient: Invalid ({e})")),
    }

    let spend_hook = match spend_hook {
        Some(spend_hook_str) => match FuncId::from_str(spend_hook_str) {
            Ok(_) => String::from(spend_hook_str),
            Err(e) => format!("Invalid ({e})"),
        },
        None => String::from("-"),
    };
    output.push(format!("Spend hook: {spend_hook}"));

    let user_data = match user_data {
        Some(user_data_str) => match bs58::decode(&user_data_str).into_vec() {
            Ok(bytes) => match bytes.try_into() {
                Ok(bytes) => {
                    if pallas::Base::from_repr(bytes).is_some().into() {
                        String::from(user_data_str)
                    } else {
                        String::from("Invalid")
                    }
                }
                Err(e) => format!("Invalid ({e:?})"),
            },
            Err(e) => format!("Invalid ({e})"),
        },
        None => String::from("-"),
    };
    output.push(format!("User data: {user_data}"));
}

/// Cast `ContractCallImport` to `ContractCallLeaf`
fn to_leaf(call: &ContractCallImport) -> ContractCallLeaf {
    ContractCallLeaf {
        call: call.call().clone(),
        proofs: call.proofs().iter().map(|p| Proof::new(p.clone())).collect(),
    }
}

/// Recursively build subtree for a DarkTree
fn build_subtree(
    idx: usize,
    calls: &[ContractCallImport],
    children_map: &HashMap<usize, &Vec<usize>>,
) -> DarkTree<ContractCallLeaf> {
    let children_idx = children_map.get(&idx).map(|v| v.as_slice()).unwrap_or(&[]);

    let children: Vec<DarkTree<ContractCallLeaf>> =
        children_idx.iter().map(|&i| build_subtree(i, calls, children_map)).collect();

    DarkTree::new(to_leaf(&calls[idx]), children, None, None)
}

/// Recursively retrieve the signature keys in Post order traversal
fn retrieve_signature_keys(
    idx: usize,
    calls: &[ContractCallImport],
    children_map: &HashMap<usize, &Vec<usize>>,
    sig_keys: &mut Vec<Vec<SecretKey>>,
) {
    let children_idx = children_map.get(&idx).map(|v| v.as_slice()).unwrap_or(&[]);

    for i in children_idx {
        retrieve_signature_keys(*i, calls, children_map, sig_keys)
    }

    sig_keys.push(calls[idx].secrets().to_vec());
}

/// Build a `Transaction` given a slice of calls and their mapping
pub fn tx_from_calls_mapped(
    calls: &[ContractCallImport],
    map: &[(usize, Vec<usize>)],
) -> Result<(TransactionBuilder, Vec<Vec<SecretKey>>)> {
    assert_eq!(calls.len(), map.len());

    let children_map: HashMap<usize, &Vec<usize>> = map.iter().map(|(k, v)| (*k, v)).collect();
    let all_children_idx: HashSet<&usize> = children_map.values().flat_map(|v| *v).collect();
    let root_idxs: Vec<usize> =
        map.iter().map(|(k, _)| *k).filter(|k| !all_children_idx.contains(k)).collect();

    // Build the first root call
    let root_idx = root_idxs[0];
    let root_children: Vec<DarkTree<ContractCallLeaf>> =
        children_map[&root_idx].iter().map(|&i| build_subtree(i, calls, &children_map)).collect();
    let mut tx_builder = TransactionBuilder::new(to_leaf(&calls[root_idx]), root_children)?;

    // Build remaining root calls
    for root_idx in &root_idxs[1..] {
        let root_children: Vec<DarkTree<ContractCallLeaf>> = children_map[root_idx]
            .iter()
            .map(|&i| build_subtree(i, calls, &children_map))
            .collect();
        tx_builder.append(to_leaf(&calls[*root_idx]), root_children)?;
    }

    let mut signature_secrets: Vec<Vec<SecretKey>> = vec![];
    for idx in root_idxs {
        retrieve_signature_keys(idx, calls, &children_map, &mut signature_secrets);
    }

    Ok((tx_builder, signature_secrets))
}

/// Auxiliary function to parse a contract call mapping.
///
/// The mapping is in the format of `{0: [1,2], 1: [], 2:[3], 3:[]}`.
/// It supports nesting and this kind of logic as expected.
///
/// Errors out if there are non-unique keys or cyclic references.
pub fn parse_tree(input: &str) -> std::result::Result<Vec<(usize, Vec<usize>)>, String> {
    let s = input
        .trim()
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or("expected {}")?
        .trim();

    let mut entries = vec![];
    let mut seen_keys = HashSet::new();

    if s.is_empty() {
        return Ok(entries)
    }

    let mut rest = s;
    while !rest.is_empty() {
        // Parse key
        let (key_str, after_key) = rest.split_once(':').ok_or("expected ':'")?;
        let key: usize = key_str.trim().parse().map_err(|_| "invalid key")?;

        if !seen_keys.insert(key) {
            return Err(format!("duplicate key: {}", key));
        }

        // Parse array
        let after_key = after_key.trim();
        let arr_start = after_key.strip_prefix('[').ok_or("expected '['")?;
        let (arr_content, after_arr) = arr_start.split_once(']').ok_or("expected ']'")?;

        let children: Vec<usize> = arr_content
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().map_err(|_| "invalid child"))
            .collect::<std::result::Result<_, _>>()?;

        entries.push((key, children));

        // Move to next entry
        rest = after_arr.trim().strip_prefix(',').unwrap_or(after_arr).trim();
    }

    check_cycles(&entries)?;

    Ok(entries)
}

fn check_cycles(entries: &[(usize, Vec<usize>)]) -> std::result::Result<(), String> {
    let graph: HashMap<usize, &Vec<usize>> = entries.iter().map(|(k, v)| (*k, v)).collect();
    let mut visited = HashSet::new();
    let mut path = Vec::new();

    fn dfs(
        node: usize,
        graph: &HashMap<usize, &Vec<usize>>,
        visited: &mut HashSet<usize>,
        path: &mut Vec<usize>,
    ) -> std::result::Result<(), String> {
        if let Some(pos) = path.iter().position(|&n| n == node) {
            let cycle: Vec<_> = path[pos..].iter().chain(&[node]).map(|n| n.to_string()).collect();
            return Err(format!("cycle detected: {}", cycle.join(" -> ")));
        }

        if visited.contains(&node) {
            return Ok(());
        }

        path.push(node);
        if let Some(children) = graph.get(&node) {
            for &child in *children {
                dfs(child, graph, visited, path)?;
            }
        }
        path.pop();
        visited.insert(node);

        Ok(())
    }

    for &(key, _) in entries {
        dfs(key, &graph, &mut visited, &mut path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_sdk::{
        crypto::{pasta_prelude::Field, ContractId},
        ContractCall,
    };
    use rand::rngs::OsRng;

    #[test]
    fn test_parse_tree() {
        // Valid inputs
        assert_eq!(parse_tree("{}").unwrap(), vec![]);
        assert_eq!(parse_tree("{  }").unwrap(), vec![]);
        assert_eq!(parse_tree("{ 0: [] }").unwrap(), vec![(0, vec![])]);
        assert_eq!(parse_tree("{ 0: [1, 2, 3] }").unwrap(), vec![(0, vec![1, 2, 3])]);
        assert_eq!(parse_tree("{0:[],1:[2]}").unwrap(), vec![(0, vec![]), (1, vec![2])]);
        assert_eq!(parse_tree("{ 0: [], 1: [], }").unwrap(), vec![(0, vec![]), (1, vec![])]);
        assert_eq!(parse_tree("{ 0: [1, 2,] }").unwrap(), vec![(0, vec![1, 2])]);

        assert_eq!(
            parse_tree("{ 0: [], 1: [2, 3], 2: [], 3: [4], 4: [] }").unwrap(),
            vec![(0, vec![]), (1, vec![2, 3]), (2, vec![]), (3, vec![4]), (4, vec![])]
        );

        assert_eq!(
            parse_tree("{   0  :  [  ]  ,   1  :  [  2  ,  3  ]   }").unwrap(),
            vec![(0, vec![]), (1, vec![2, 3])]
        );

        assert_eq!(
            parse_tree("{ 999: [1000, 1001], 1000: [], 1001: [] }").unwrap(),
            vec![(999, vec![1000, 1001]), (1000, vec![]), (1001, vec![])]
        );

        // Order preservation
        let keys: Vec<usize> =
            parse_tree("{ 5: [], 2: [], 9: [], 0: [] }").unwrap().iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec![5, 2, 9, 0]);

        // Valid DAG (not a cycle)
        assert!(parse_tree("{ 0: [1, 2], 1: [3], 2: [3], 3: [] }").is_ok());

        // Syntax errors
        assert!(parse_tree("0: [] }").is_err());
        assert!(parse_tree("{ 0: []").is_err());
        assert!(parse_tree("{ 0 [] }").is_err());
        assert!(parse_tree("{ 0: ] }").is_err());
        assert!(parse_tree("{ 0: [1, 2 }").is_err());
        assert!(parse_tree("{ abc: [] }").is_err());
        assert!(parse_tree("{ 0: [abc] }").is_err());
        assert!(parse_tree("{ -1: [] }").is_err());

        // Duplicate keys
        assert!(parse_tree("{ 0: [], 0: [1] }").unwrap_err().contains("duplicate key: 0"));
        assert!(parse_tree("{ 0: [], 1: [], 2: [], 1: [] }")
            .unwrap_err()
            .contains("duplicate key: 1"));

        // Cycle detection
        let err = parse_tree("{ 0: [0] }").unwrap_err();
        assert!(err.contains("cycle detected") && err.contains("0 -> 0"));

        let err = parse_tree("{ 0: [1], 1: [0] }").unwrap_err();
        assert!(err.contains("cycle detected"));

        let err = parse_tree("{ 0: [1], 1: [2], 2: [3], 3: [0] }").unwrap_err();
        assert!(err.contains("cycle detected") && err.contains("0 -> 1 -> 2 -> 3 -> 0"));

        let err = parse_tree("{ 0: [1], 1: [2], 2: [3], 3: [2] }").unwrap_err();
        assert!(err.contains("cycle detected") && err.contains("2 -> 3 -> 2"));
    }

    #[test]
    fn test_tx_from_calls_mapped() {
        let contract0 = ContractId::from(pallas::Base::random(&mut OsRng));
        let contract1 = ContractId::from(pallas::Base::random(&mut OsRng));
        let contract2 = ContractId::from(pallas::Base::random(&mut OsRng));
        let call0 = ContractCallImport::new(
            ContractCall { contract_id: contract0, data: vec![] },
            vec![],
            vec![],
        );
        let call1 = ContractCallImport::new(
            ContractCall { contract_id: contract1, data: vec![] },
            vec![],
            vec![SecretKey::random(&mut OsRng), SecretKey::random(&mut OsRng)],
        );
        let call2 = ContractCallImport::new(
            ContractCall { contract_id: contract2, data: vec![] },
            vec![],
            vec![SecretKey::random(&mut OsRng)],
        );

        // Transaction with 3 root calls, each with no children
        let (mut tx_builder, sig_keys) = tx_from_calls_mapped(
            &[call0.clone(), call1.clone(), call2.clone()],
            &parse_tree("{0 : [], 1: [], 2: []}").unwrap(),
        )
        .unwrap();
        let leafs = tx_builder.calls.build_vec().unwrap();

        assert_eq!(leafs.len(), 3);
        assert_eq!(leafs[0].data.call.contract_id, contract0);
        assert_eq!(leafs[1].data.call.contract_id, contract1);
        assert_eq!(leafs[2].data.call.contract_id, contract2);
        assert_eq!(sig_keys.len(), 3);
        assert_eq!(sig_keys[0].len(), 0);
        assert_eq!(sig_keys[1].len(), 2);
        assert_eq!(sig_keys[2].len(), 1);

        // Transaction with 2 root calls, the second call is child of the first
        let (mut tx_builder, sig_keys) = tx_from_calls_mapped(
            &[call0.clone(), call1.clone(), call2.clone()],
            &parse_tree("{0 : [1], 1: [], 2: []}").unwrap(),
        )
        .unwrap();
        let leafs = tx_builder.calls.build_vec().unwrap();

        assert_eq!(leafs.len(), 3);
        assert_eq!(leafs[0].data.call.contract_id, contract1);
        assert_eq!(leafs[1].data.call.contract_id, contract0);
        assert_eq!(leafs[2].data.call.contract_id, contract2);
        assert_eq!(sig_keys.len(), 3);
        assert_eq!(sig_keys[0].len(), 2);
        assert_eq!(sig_keys[1].len(), 0);
        assert_eq!(sig_keys[2].len(), 1);

        // Transaction with 1 root call, the second and third are the children of the first
        let (mut tx_builder, sig_keys) = tx_from_calls_mapped(
            &[call0.clone(), call1.clone(), call2.clone()],
            &parse_tree("{0 : [1, 2], 1: [], 2: []}").unwrap(),
        )
        .unwrap();
        let leafs = tx_builder.calls.build_vec().unwrap();

        assert_eq!(leafs.len(), 3);
        assert_eq!(leafs[0].data.call.contract_id, contract1);
        assert_eq!(leafs[1].data.call.contract_id, contract2);
        assert_eq!(leafs[2].data.call.contract_id, contract0);
        assert_eq!(sig_keys.len(), 3);
        assert_eq!(sig_keys[0].len(), 2);
        assert_eq!(sig_keys[1].len(), 1);
        assert_eq!(sig_keys[2].len(), 0);

        // Transaction with 1 root call, the first is the child of the second, the second is the
        // child of the third
        let (mut tx_builder, sig_keys) = tx_from_calls_mapped(
            &[call0, call1, call2],
            &parse_tree("{0 : [], 1: [0], 2: [1]}").unwrap(),
        )
        .unwrap();
        let leafs = tx_builder.calls.build_vec().unwrap();

        assert_eq!(leafs.len(), 3);
        assert_eq!(leafs[0].data.call.contract_id, contract0);
        assert_eq!(leafs[1].data.call.contract_id, contract1);
        assert_eq!(leafs[2].data.call.contract_id, contract2);
        assert_eq!(sig_keys.len(), 3);
        assert_eq!(sig_keys[0].len(), 0);
        assert_eq!(sig_keys[1].len(), 2);
        assert_eq!(sig_keys[2].len(), 1);
    }
}

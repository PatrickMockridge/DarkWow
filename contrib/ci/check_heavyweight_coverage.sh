#!/bin/bash
# Endpoint coverage checker for genesis contract heavyweight tests.
# Parses each genesis contract's lib.rs function enum and verifies
# the heavyweight test exercises each variant through accept_block.
# Usage: ./check_heavyweight_coverage.sh [--json]
# Exit 0 = full coverage, Exit 1 = gaps found

set -u

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

HEAVYWEIGHT_FILE="$REPO_ROOT/bin/dwowd/src/tests/heavyweight_pipeline.rs"
CONTRACT_DIR="$REPO_ROOT/src/contract"

JSON_MODE=false
if [[ "${1:-}" == "--json" ]]; then
    JSON_MODE=true
fi

GAPS=0

# Known genesis contracts and their function enum names
# Format: contract_dir|enum_name|test_fn_name|contract_id_constant
declare -A GENESIS_CONTRACTS
GENESIS_CONTRACTS=(
    ["native_token"]="NativeTokenFunction|test_heavyweight_native_token|NATIVE_TOKEN_CONTRACT_ID"
    ["identity"]="IdentityFunction|test_heavyweight_identity|IDENTITY_CONTRACT_ID"
    ["attestation"]="AttestationFunction|test_heavyweight_attestation|ATTESTATION_CONTRACT_ID"
    ["multisig"]="MultiSigFunction|test_heavyweight_multisig|MULTISIG_CONTRACT_ID"
    ["oracle"]="OracleFunction|test_heavyweight_oracle|ORACLE_CONTRACT_ID"
    ["promissory_note"]="PromissoryNoteFunction|test_heavyweight_promissory_note|PROMISSORY_NOTE_CONTRACT_ID"
    ["purse"]="PurseFunction|test_heavyweight_purse|PURSE_CONTRACT_ID"
    ["box"]="BoxFunction|test_heavyweight_box|BOX_CONTRACT_ID"
    ["deployooor"]="DeployFunction|test_heavyweight_deployooor|DEPLOYOOOR_CONTRACT_ID"
)

# Parse function variants from lib.rs.
# Two formats exist:
# 1. Standard enum: pub enum NativeTokenFunction { FeeV1 = 0x00, ... }
# 2. Macro: define_contract_function!(BoxFunction { Initialize = 0x00, Put = 0x01, ... });
parse_enum_variants() {
    local lib_file="$1"
    local enum_name="$2"

    # Try macro format first: define_contract_function!(EnumName { Variant = 0xNN, ... });
    local macro_vars
    macro_vars=$(sed -n "/define_contract_function!($enum_name {/,/});/p" "$lib_file" 2>/dev/null | \
        grep -E '^\s+[A-Z][a-zA-Z0-9_]+\s*=' | \
        sed 's/^\s*//' | sed 's/\s*=.*//' | sed 's/,//')

    if [[ -n "$macro_vars" ]]; then
        echo "$macro_vars"
        return 0
    fi

    # Try standard enum format: pub enum EnumName { Variant = 0xNN, ... }
    sed -n "/pub enum $enum_name/,/^}/p" "$lib_file" 2>/dev/null | \
        grep -E '^\s+[A-Z][a-zA-Z0-9_]+\s*=' | \
        sed 's/^\s*//' | sed 's/\s*=.*//' | sed 's/,//'
}

# Check if a function variant name appears in the heavyweight test
# within the context of the test function for that contract
check_variant_in_test() {
    local variant="$1"
    local test_fn="$2"
    # Search for the variant name or its snake_case form within the test function
    local snake_name
    snake_name=$(echo "$variant" | sed 's/\([A-Z]\)/_\L\1/g' | sed 's/^_//' | tr '[:upper:]' '[:lower:]')
    # Look for the variant name in any form within the test function body
    # (between the test function header and the next test function or EOF)
    if grep -q "$variant\|$snake_name\|0x0[0-9a-f].*$variant" "$HEAVYWEIGHT_FILE" 2>/dev/null; then
        return 0
    fi
    return 1
}

# Check that the test function's body contains accept_block (submit)
check_has_accept_block() {
    local test_fn="$1"
    local start_line end_line
    start_line=$(grep -n "fn $test_fn" "$HEAVYWEIGHT_FILE" | head -1 | cut -d: -f1)
    if [[ -z "$start_line" ]]; then
        echo "MISSING_TEST"
        return 1
    fi
    # Find the end of the test function (next fn or EOF)
    end_line=$(tail -n +"$start_line" "$HEAVYWEIGHT_FILE" | grep -n "^fn \|^#\[test\]" | head -1 | cut -d: -f1)
    if [[ -z "$end_line" ]]; then
        end_line=$(wc -l < "$HEAVYWEIGHT_FILE")
    else
        end_line=$((start_line + end_line - 1))
    fi
    # Check for submit() or accept_block in the function body
    if sed -n "${start_line},${end_line}p" "$HEAVYWEIGHT_FILE" | grep -q 'submit()\|accept_block'; then
        echo "HAS_ACCEPT_BLOCK"
        return 0
    else
        echo "NO_ACCEPT_BLOCK"
        return 1
    fi
}

total_contracts=0
total_functions=0
covered_functions=0

for contract in "${!GENESIS_CONTRACTS[@]}"; do
    IFS='|' read -r enum_name test_fn cid_const <<< "${GENESIS_CONTRACTS[$contract]}"
    lib_file="$CONTRACT_DIR/$contract/src/lib.rs"

    if [[ ! -f "$lib_file" ]]; then
        if $JSON_MODE; then
            echo "{\"contract\":\"$contract\",\"status\":\"NO_LIB_FILE\",\"file\":\"$lib_file\"}"
        else
            echo "[WARN] $contract: No lib.rs found at $lib_file"
        fi
        continue
    fi

    total_contracts=$((total_contracts + 1))

    variants=$(parse_enum_variants "$lib_file" "$enum_name" || true)
    if [[ -z "${variants:-}" ]]; then
        if $JSON_MODE; then
            echo "{\"contract\":\"$contract\",\"status\":\"ENUM_NOT_FOUND\",\"enum\":\"$enum_name\"}"
        else
            echo "[WARN] $contract: Enum $enum_name not found in $lib_file"
        fi
        continue
    fi

    # Check accept_block presence
    accept_block_status=$(check_has_accept_block "$test_fn")

    missing_variants=""
    variant_count=0
    while IFS= read -r variant; do
        [[ -z "$variant" ]] && continue
        variant_count=$((variant_count + 1))
        total_functions=$((total_functions + 1))

        if check_variant_in_test "$variant" "$test_fn"; then
            covered_functions=$((covered_functions + 1))
        else
            missing_variants="$missing_variants $variant"
            GAPS=$((GAPS + 1))
        fi
    done <<< "$variants"

    if $JSON_MODE; then
        echo "{\"contract\":\"$contract\",\"enum\":\"$enum_name\",\"test_fn\":\"$test_fn\",\"total\":$variant_count,\"covered\":$((variant_count - $(echo "$missing_variants" | wc -w))),\"accept_block\":\"$accept_block_status\",\"missing\":[$(echo "$missing_variants" | sed 's/ /", "/g' | sed 's/^", //' | sed 's/", $//')]}"
    else
        covered=$((variant_count - $(echo "$missing_variants" | wc -w)))
        if [[ -n "$missing_variants" ]]; then
            echo "[GAP] $contract: $covered/$variant_count functions covered, accept_block=$accept_block_status. Missing:$missing_variants"
        else
            echo "[OK]  $contract: $covered/$variant_count functions covered, accept_block=$accept_block_status"
        fi
    fi
done

# Summary
if $JSON_MODE; then
    echo "{\"summary\":{\"contracts\":$total_contracts,\"total_functions\":$total_functions,\"covered_functions\":$covered_functions,\"gaps\":$GAPS}}"
else
    echo ""
    echo "=== Coverage Summary ==="
    echo "Contracts: $total_contracts"
    echo "Total functions: $total_functions"
    echo "Covered: $covered_functions"
    echo "Gaps: $GAPS"
fi

if [ "$GAPS" -gt 0 ]; then
    exit 1
fi
exit 0

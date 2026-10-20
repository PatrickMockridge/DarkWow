#!/usr/bin/env python3
"""Compare a contract manifest's published ``[[parameters]]`` with its Rust decoder's struct.

WHY THIS EXISTS. `OBL-C179` / `LR-17`: the manifest tag, the SDK encoder and the contract's
`*Params::decode` are three descriptions of one wire. The manifest's `witness` tag means "the caller
need not supply this; the client fills it from the bound value and publishes it anyway", and
`off_wire` means "not in the call data". A field that silently leaves the published subset, or a
manifest that keeps a field the decoder dropped, changes the wire with nothing checking it. The
`box.put` `tx_binding` removal (`OBL-C198`) is the live instance: the model table carried the field
for weeks and `wallet_model` was the only instrument that noticed.

WHAT IT CHECKS. For contracts whose `[[parameters]]` describe the decoder's struct (box and purse),
the manifest's published list (fields without `off_wire`, in order) must equal the Rust struct's
fields, by name and by width class. The width class is a coarsening (`pallas::Base` and
`MerkleNode` are both `pallas_base`, `MerklePosition` is `u32`, `MerklePath` is `merkle_path`, a
`Vec<u8>` called `proof` is `proof`); it is not a Rust type identity check.

WHAT IT DOES NOT CHECK. It does not prove the decoder's *offsets* are right, only that the field
list and width classes agree; it does not read the circuit's witness map; and it does not cover
`promissory_note`, whose `[[parameters]]` are a client API rather than the decoder. Those functions
are listed in `script/schema_decoder_exceptions.txt` with that reason, and a stale exception is
reported.

Usage:
  scripts/check-schema-decoder-agreement.py            # --check is the default
  scripts/check-schema-decoder-agreement.py --self-test
  scripts/check-schema-decoder-agreement.py --root /path/to/repo
"""

import argparse
import re
import sys
import tomllib
from pathlib import Path

STRUCTS = {
    "box": {"put": "PutParams", "take": "TakeParams"},
    "purse": {
        "deposit": "DepositParams",
        "withdraw": "WithdrawParams",
        "balance": "BalanceParams",
    },
}

EXCEPTIONS = "script/schema_decoder_exceptions.txt"


def rust_struct_fields(src: str, name: str):
    idx = src.find("pub struct " + name)
    if idx < 0:
        return None
    brace = src.find("{", idx)
    if brace < 0:
        return None
    depth = 0
    end = -1
    for i in range(brace, len(src)):
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
            if depth == 0:
                end = i
                break
    if end < 0:
        return None
    body = src[brace + 1:end]
    fields = []
    for m in re.finditer(r"pub\s+(\w+)\s*:\s*([^,]+?)\s*,", body):
        fields.append((m.group(1), m.group(2).strip()))
    return fields


def width_class(rust_type: str, field_name: str):
    t = rust_type
    if "pallas::Scalar" in t:
        return "pallas_scalar"
    if "pallas::Base" in t:
        return "pallas_base"
    if "MerklePosition" in t:
        return "u32"
    if "MerklePath" in t:
        return "merkle_path"
    if "Vec<u8>" in t:
        return "proof" if field_name == "proof" else "bytes"
    if t == "u64":
        return "u64"
    if t == "bool":
        return "bool"
    if any(
        x in t
        for x in (
            "PublicKey",
            "Nullifier",
            "MerkleNode",
            "IntentNullifier",
            "IntentCommitment",
            "CapabilityId",
        )
    ):
        return "pallas_base"
    return None


def published(manifest: dict, function: str):
    for p in manifest.get("parameters", []):
        if p.get("function") != function:
            continue
        return [(f["name"], f["type"]) for f in p["fields"] if not f.get("off_wire", False)]
    return None


def load_exceptions(root: Path):
    path = root / EXCEPTIONS
    out = []
    if not path.exists():
        return out
    for line in path.read_text(errors="replace").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) >= 2:
            out.append((parts[0], parts[1], " ".join(parts[2:])))
    return out


def compare_files(contract: str, function: str, manifest_path: Path, model_path: Path):
    """Return a list of findings for one (contract, function)."""
    findings = []
    try:
        manifest = tomllib.loads(manifest_path.read_text())
    except (OSError, tomllib.TOMLDecodeError) as e:
        return [f"{contract}.{function}: cannot read manifest {manifest_path}: {e}"]
    got = published(manifest, function)
    if got is None:
        return [f"{contract}.{function}: no [[parameters]] entry in {manifest_path}"]
    struct_name = STRUCTS[contract][function]
    try:
        src = model_path.read_text()
    except OSError as e:
        return [f"{contract}.{function}: cannot read model {model_path}: {e}"]
    fields = rust_struct_fields(src, struct_name)
    if fields is None:
        return [f"{contract}.{function}: no struct {struct_name} in {model_path}"]
    want = []
    for name, rust_type in fields:
        wc = width_class(rust_type, name)
        if wc is None:
            findings.append(
                f"{contract}.{function}: struct field '{name}' has unmapped Rust type '{rust_type}'"
            )
            continue
        want.append((name, wc))
    if got != want:
        findings.append(
            f"{contract}.{function}: manifest published {got} != {struct_name} {want}"
        )
    return findings


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="the default mode")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--root", default=str(Path(__file__).resolve().parent.parent))
    args = ap.parse_args()
    root = Path(args.root)

    if args.self_test:
        import tempfile

        box_manifest = root / "src/contract/box/manifest.toml"
        box_model = root / "src/contract/box/src/model/mod.rs"
        with tempfile.TemporaryDirectory() as td:
            td = Path(td)
            good_manifest = td / "good_manifest.toml"
            good_model = td / "good_model.rs"
            bad_manifest = td / "bad_manifest.toml"
            good_manifest.write_text(box_manifest.read_text())
            good_model.write_text(box_model.read_text())
            text = good_manifest.read_text()
            # Remove the published `tx_nonce` from put, the exact live-drift shape.
            lines = text.splitlines()
            out = []
            removed = False
            in_put = False
            for line in lines:
                if 'function = "put"' in line:
                    in_put = True
                elif 'function = ' in line:
                    in_put = False
                if in_put and re.search(r'name = "tx_nonce"', line):
                    removed = True
                    continue
                out.append(line)
            if not removed:
                print("FAIL: --self-test: could not plant the field removal")
                return 1
            bad_manifest.write_text("\n".join(out) + "\n")
            if compare_files("box", "put", good_manifest, good_model):
                print("FAIL: --self-test: the unmodified pair did not agree")
                return 1
            print("OK: --self-test — the unmodified manifest/decoder pair agrees")
            if not compare_files("box", "put", bad_manifest, good_model):
                print("FAIL: --self-test: a removed published field passed")
                return 1
            print("OK: --self-test — a removed published field is reported")
        return 0

    exceptions = load_exceptions(root)
    seen = set()
    findings = []
    compared = 0
    for contract, functions in STRUCTS.items():
        model = root / f"src/contract/{contract}/src/model/mod.rs"
        manifest = root / f"src/contract/{contract}/manifest.toml"
        for function in functions:
            seen.add((contract, function))
            if any(
                (c == contract or c == "*") and (f == function or f == "*")
                for c, f, _ in exceptions
            ):
                print(f"EXCEPTED: {contract}.{function}")
                continue
            findings.extend(compare_files(contract, function, manifest, model))
            compared += 1
    for c, f, reason in exceptions:
        if c == "*" or (c, f) not in seen:
            # A wildcard is legitimate only for a contract whose functions are all client-facing.
            if c == "*":
                continue
            findings.append(f"stale exception: {c}.{f} ({reason}) has no such compared function")

    if findings:
        for f in findings:
            print(f"FAIL: {f}")
        return 1
    print(f"PASS: {compared} manifest/decoder pair(s) agree; {len(exceptions)} declared exception(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

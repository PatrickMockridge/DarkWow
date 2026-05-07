#!/usr/bin/env python3
"""Replace license headers in all .rs files with the new DarkWow header."""

import os
import sys

NEW_HEADER = """/* This file is part of DarkWow
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
 */"""

OLD_AGPL_HEADER_START = "/* This file is part of DarkFi (https://dark.fi)"
OLD_GPL_HEADER_START = "/* This file is part of DarkFi (https://dark.fi)"

def find_header_end(content):
    """Find the closing */ of the initial block comment. Returns index after */ or -1."""
    # The header is the first block comment. Find the first */
    idx = content.find("*/")
    if idx == -1:
        return -1
    # Check that there's a /* before it
    if "/*" not in content[:idx]:
        return -1
    return idx + 2  # position after */

def has_license_header(content):
    """Check if file starts with a DarkFi license header block comment."""
    stripped = content.lstrip()
    return stripped.startswith(OLD_AGPL_HEADER_START)

def replace_header(filepath):
    """Replace license header in a single file. Returns True if replaced."""
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()
    except Exception as e:
        print(f"  SKIP (read error): {filepath}: {e}")
        return False

    if not has_license_header(content):
        return False

    end_idx = find_header_end(content)
    if end_idx == -1:
        print(f"  SKIP (no header end): {filepath}")
        return False

    # Replace everything from start to end of header block comment
    after_header = content[end_idx:]
    new_content = NEW_HEADER + after_header

    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(new_content)
    return True

def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    dirs = ["src", "bin", "script", "example", "fuzz", "proofs"]

    count = 0
    for d in dirs:
        dpath = os.path.join(root, d)
        if not os.path.isdir(dpath):
            continue
        for dirpath, _, filenames in os.walk(dpath):
            # Skip target directories
            if "/target/" in dirpath or dirpath.endswith("/target"):
                continue
            for fname in filenames:
                if not fname.endswith(".rs"):
                    continue
                fpath = os.path.join(dirpath, fname)
                if replace_header(fpath):
                    count += 1

    print(f"Replaced headers in {count} .rs files")

if __name__ == "__main__":
    main()

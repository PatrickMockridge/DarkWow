#!/usr/bin/env python3
"""Replace DarkFi → DarkWow in Rust comments (not code)."""

import os
import sys

def replace_comments(filepath):
    """Replace DarkFi with DarkWow on comment lines only."""
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            lines = f.readlines()
    except Exception:
        return False

    changed = False
    new_lines = []
    for line in lines:
        stripped = line.lstrip()
        if stripped.startswith('//') and 'DarkFi' in line:
            new_line = line.replace('DarkFi', 'DarkWow')
            if new_line != line:
                changed = True
                new_lines.append(new_line)
                continue
        new_lines.append(line)

    if changed:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.writelines(new_lines)
    return changed

def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    dirs = ["src", "bin", "script", "example", "fuzz", "tests", "bench"]

    fcount = 0
    for d in dirs:
        dpath = os.path.join(root, d)
        if not os.path.isdir(dpath):
            continue
        for dirpath, _, filenames in os.walk(dpath):
            if "/target/" in dirpath or dirpath.endswith("/target"):
                continue
            for fname in filenames:
                if not fname.endswith(".rs"):
                    continue
                if replace_comments(os.path.join(dirpath, fname)):
                    fcount += 1

    print(f"Updated comments in {fcount} .rs files")

if __name__ == "__main__":
    main()

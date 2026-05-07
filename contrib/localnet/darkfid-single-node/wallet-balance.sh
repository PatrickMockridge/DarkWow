#!/bin/sh

# Path to `dww` binary
DWW="../../../dww -c dww.toml"

while true; do
    if $DWW ping 2> /dev/null; then
        break
    fi
    sleep 1
done

$DWW scan
$DWW wallet balance

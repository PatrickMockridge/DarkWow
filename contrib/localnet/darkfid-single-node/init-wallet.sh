#!/bin/sh

# Path to `dww` binary
DWW="../../../dww -c dww.toml"

$DWW wallet initialize
$DWW wallet keygen
$DWW wallet default-address 1
wallet=$($DWW wallet address)
sed -i -e "s|DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf|$wallet|g" tmux_sessions.sh

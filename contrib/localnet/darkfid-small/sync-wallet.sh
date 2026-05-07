#!/bin/sh
set -e
set -x

# Accept path to `dww` binary as arg or use default
DEFAULT_DRK="../../../dww -c drk0.toml"
DWW="${1:-$DEFAULT_DRK}"

sync_wallet() {
  while true; do
      if $1 ping 2> /dev/null; then
          break
      fi
      sleep 1
  done

  $1 scan
}

sync_wallet "$DWW"

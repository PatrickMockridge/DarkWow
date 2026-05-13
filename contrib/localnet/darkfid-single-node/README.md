dwowd localnet
================

This will start one `dwowd` node in localnet mode, along with an `xmrig`
daemon to mine blocks.

If we want to test wallet stuff, we must generate a testing wallet and
pass its address to the `xmrig` daemon, so the wallet gets the block
rewards the node produces. We generate a wallet, set it as the default
and set its address as the `XMRIG_USER` field in `tmux_sessions.sh`,
using provided automated script:
```shell
% ./init-wallet.sh
```

Then make sure the `xmrig` daemon binary path is configured correctly
in `tmux_sessions.sh`, start the daemons and wait until `dwowd` is
initialized:
```shell
% ./tmux_sessions.sh
```

After some blocks have been generated we will see some `DRKW` in our
test wallet:
```shell
% ./wallet-balance.sh
```

See the user guide in the book for more info.

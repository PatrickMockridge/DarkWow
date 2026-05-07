#!/bin/sh
set -e
set -x

# Path to `dww` binary
DWW="../../../dww -c dww.toml"

# Script configuration
OUTPUT_FOLDER=/tmp/darkfi
mkdir -p $OUTPUT_FOLDER
SLEEP_TIME=5

# First run the dwowd node and the miner:
#
#   ./clean.sh
#   ./init-wallet.sh
#   ./tmux_sessions.sh
#
# Now you can run this script

mint_token() {
    $DWW alias add $1 "$($DWW token generate-mint | awk '{print $8}')"
    $DWW token mint $1 $2 "$($DWW wallet address)" | tee $OUTPUT_FOLDER/mint-$1.tx | $DWW broadcast
    $DWW token list
}

token_balance() {
    BALANCE="$($DWW wallet balance 2>/dev/null)"

    # No tokens received at all yet
    if echo "$BALANCE" | grep -q "No unspent balances found"; then
        echo 0
        return
    fi

    BALANCE="$(echo "$BALANCE" | grep -q "$1")"
    # Not received yet so no entry
    if [ $? = 1 ]; then
        echo 0
        return
    fi

    # OK we have the token, show the actual balance
    echo "$BALANCE" | awk '{print $5}'
}

wait_token() {
    while [ "$(token_balance $1)" = 0 ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh > /dev/null
    done
}

mint_dao() {
    $DWW dao create 20 10 10 0.67 ANON > $OUTPUT_FOLDER/dao.toml
    $DWW dao import AnonDAO < $OUTPUT_FOLDER/dao.toml
    $DWW dao list
    $DWW dao list AnonDAO

    $DWW dao mint AnonDAO | tee $OUTPUT_FOLDER/dao-mint.tx | $DWW broadcast
}

wait_dao_mint() {
    while [ "$($DWW dao list AnonDAO | grep '^Transaction hash: ' | awk '{print $3}')" = None ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh > /dev/null
    done
}

fill_treasury() {
    PUBKEY="$($DWW dao list AnonDAO | grep '^Wallet Address: ' | cut -d ' ' -f3)"
    SPEND_HOOK="$($DWW dao spend-hook)"
    BULLA="$($DWW dao list AnonDAO | grep '^Bulla: ' | cut -d' ' -f2)"
    $DWW transfer 20 DAWN "$PUBKEY" "$SPEND_HOOK" "$BULLA" | tee $OUTPUT_FOLDER/xfer.tx | $DWW broadcast
}

dao_balance() {
    BALANCE=$($DWW dao balance AnonDAO 2>/dev/null)
    # No tokens received at all yet
    if echo "$BALANCE" | grep -q "No unspent balances found"; then
        echo 0
        return
    fi

    BALANCE=$(echo "$BALANCE" | grep "$1")
    # Not received yet so no entry
    if [ $? = 1 ]; then
        echo 0
        return
    fi

    # OK we have the token, show the actual balance
    echo "$BALANCE" | awk '{print $5}'
}

wait_dao_treasury() {
    while [ "$(dao_balance DAWN)" = 0 ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh > /dev/null
    done
}

propose() {
    MY_ADDR=$($DWW wallet address)
    PROPOSAL="$($DWW dao propose-transfer AnonDAO 1 5 DAWN "$MY_ADDR" | cut -d' ' -f3)"
    $DWW dao proposal "$PROPOSAL" --mint-proposal | tee $OUTPUT_FOLDER/propose.tx | $DWW broadcast
}

wait_proposal() {
    PROPOSAL="$($DWW dao proposals AnonDAO | cut -d' ' -f2)"
    while [ "$($DWW dao proposal $PROPOSAL | grep '^Proposal transaction hash: ' | awk '{print $4}')" = None ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh > /dev/null
    done
}

vote() {
    PROPOSAL="$($DWW dao proposals AnonDAO | cut -d' ' -f2)"
    $DWW dao vote "$PROPOSAL" 1 | tee $OUTPUT_FOLDER/dao-vote.tx | $DWW broadcast
}

wait_vote() {
    PROPOSAL="$($DWW dao proposals AnonDAO | cut -d' ' -f2)"
    while [ "$($DWW dao proposal $PROPOSAL | grep '^Current proposal outcome: ' | awk '{print $4}')" != "Approved" ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh > /dev/null
    done
}

do_exec() {
    PROPOSAL="$($DWW dao proposals AnonDAO | cut -d' ' -f2)"
    $DWW dao exec --early $PROPOSAL | tee $OUTPUT_FOLDER/dao-exec.tx | $DWW broadcast
}

wait_exec() {
    PROPOSAL="$($DWW dao proposals AnonDAO | cut -d' ' -f2)"
    while [ -z "$($DWW dao proposal $PROPOSAL | grep '^Proposal was executed on transaction: ')" ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh > /dev/null
    done
}

wait_token DWW
mint_token ANON 42
wait_token ANON
mint_token DAWN 20
wait_token DAWN
mint_dao
wait_dao_mint
fill_treasury
wait_dao_treasury
propose
wait_proposal
vote
wait_vote
do_exec
wait_exec

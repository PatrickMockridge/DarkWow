# Payments

Using the tokens we minted, we can make payments to other addresses.
For this tutorial we will use a dummy recipient, but you can also test
this with friends by replacing the recipient address with your friend's
address.

Let's try to send some `DRKW` tokens to
`DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf`:

```shell
darkwow wallet transfer 2.69 DRKW DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf
```

The wallet builds the transaction, prints it base64-encoded, and broadcasts
it to the network automatically — no confirmation prompt and no pipe to
`broadcast` needed. (`broadcast` exists for re-broadcasting a transaction
you already have: it reads a **binary** transaction from **stdin**, unlike
`transfer` which assembles one fresh.)

Once confirmed within a block,
`DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf` will receive
the tokens you've sent.

![pablo-waiting1](img/pablo1.jpg)

We can now see the spent coin in our wallet:

```shell
darkwow wallet coins

 Asset ID                                    | Aliases | Value                    | Spend Hook | User Data
----------------------------------------------+---------+--------------------------+------------+-----------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | -       | 1999442971 (19.97253683) | -          | -
```

(Columns: Asset ID, Aliases, Value — raw base units with decimal in
parentheses — Spend Hook, User Data. Aliases is always `-`.)

We have to wait until the next block to see our change reappear in
our wallet. Balance prints one tab-separated line per retained asset:

```shell
darkwow wallet balance

241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb	-	19.97253683
```

(If nothing has been scanned yet, it prints `No retained balances found`.)

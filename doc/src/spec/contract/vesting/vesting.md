# Vesting

> [!WARNING]
> **DEPRECATED**: Vesting is not used on this fork. The contract source
> (`src/contract/vesting/`) was replaced. This spec is kept for reference
> only; it does not correspond to any active code on the `linear-master` branch.
>
> DarkWow has zero premine and no vesting schedules. All tokens are mined.

```
status: deprecated
```

## Abstract

This contract implements fully anonymous vesting, in which all the
vesting information is private. Anyone can become a vesting authority,
submitting commitments to-be-vested for another user(or a DAO), the vestee.
After some time has passed, the vestee can withdraw a chunk of the
vested commitment value. The vesting authority is also able to forfeit a
vesting at any time, retrieving the remaining vested commitment balance.

- [Concepts](concepts.md)
- [Model](model.md)
- [Scheme](scheme.md)

> Open questions:
> 1. Do we need a separate cliff time? If its set thats the start time
> so no real need to keep them both we can assume start == cliff.
> 2. Is using the shared key for signatures safe and needed?
> 3. Should vesting configurations be grouped by authority so is easier
> UX to manage them?
> 4. Is the vested commitment encryption verification formula correct?
> 5. Do we need to check both commitments in withdraw transfer in the proof or
> its fine since transfer itself enforces them?
> 6. Vesting requires 1-1 vested commitment to config matching, which means
> vested commitment is trackable as they are used during the vesting process.
> Does that break any anonymity properties? Withdrawed commitments cannot be
> tracked, just the vested commitment.
> 7. We need to figure out a way to handle withdrawls after end
> blockwindow has passed. We can use `cond_select` where both prover
> and verifier pass the condition checl `current >= end` and in the
> proof we pick current blockwindow or end blockwindow based on that.
> But this require the verifier to know the end blockwindow, unless we
> find a way the condition check can be done without revealing it.
> Another option is to have an explicit `WithdrawAfterEnd` to withdraw
> remaining balance after end blockwindow has passed. We already have
> the metadata leak of ending tracking assumption, so perhaps its
> worthy to sacrifice it.
> 8. Withdrawl calcs correctness? They can also be simplified further
> for proof optimization.
> 9. All calls use the same parameters. Unless we need something in any
> of them they will be the same structure in the final code.
> 10. Do we need to check both commitments in forfeit transfer in the proof or
> its fine since transfer itself enforces them?

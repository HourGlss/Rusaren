use vstd::prelude::*;

fn main() {}

verus! {

spec fn monotonic_accept(newest: Option<nat>, incoming: nat) -> (result: (bool, Option<nat>)) {
    match newest {
        Some(current) =>
            if incoming > current {
                (true, Some(incoming))
            } else {
                (false, Some(current))
            },
        None => (true, Some(incoming)),
    }
}

proof fn first_input_tick_binds_the_monotonic_window(incoming: nat)
    ensures
        monotonic_accept(None, incoming) == (true, Some(incoming)),
{
}

proof fn stale_or_replayed_ticks_are_rejected(current: nat, incoming: nat)
    requires
        incoming <= current,
    ensures
        monotonic_accept(Some(current), incoming) == (false, Some(current)),
{
}

proof fn newer_ticks_advance_the_window(current: nat, incoming: nat)
    requires
        incoming > current,
    ensures
        monotonic_accept(Some(current), incoming) == (true, Some(incoming)),
{
}

proof fn reset_clears_the_window_for_a_new_match(current: nat, incoming: nat)
    ensures
        monotonic_accept(None, incoming) == (true, Some(incoming)),
{
}

}

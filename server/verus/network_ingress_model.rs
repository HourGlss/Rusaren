use vstd::prelude::*;

verus! {

pub enum SessionState {
    AwaitingConnect,
    Bound(int),
}

pub spec fn valid_packet_len(len: nat, max_len: nat) -> bool {
    len <= max_len
}

pub spec fn accept_packet(
    state: SessionState,
    packet_len: nat,
    max_len: nat,
    is_connect: bool,
    claimed_player: int,
) -> (accepted: bool, next: SessionState)
    recommends
        claimed_player > 0,
{
    if !valid_packet_len(packet_len, max_len) {
        (false, state)
    } else {
        match state {
            SessionState::AwaitingConnect =>
                if is_connect {
                    (true, SessionState::Bound(claimed_player))
                } else {
                    (false, SessionState::AwaitingConnect)
                },
            SessionState::Bound(bound_player) =>
                if is_connect {
                    (false, SessionState::Bound(bound_player))
                } else {
                    (true, SessionState::Bound(bound_player))
                },
        }
    }
}

proof fn first_packet_must_bind(
    packet_len: nat,
    max_len: nat,
    claimed_player: int,
)
    requires
        claimed_player > 0,
        valid_packet_len(packet_len, max_len),
    ensures
        accept_packet(
            SessionState::AwaitingConnect,
            packet_len,
            max_len,
            true,
            claimed_player,
        ) == (true, SessionState::Bound(claimed_player)),
        accept_packet(
            SessionState::AwaitingConnect,
            packet_len,
            max_len,
            false,
            claimed_player,
        ) == (false, SessionState::AwaitingConnect),
{
}

proof fn binding_is_monotonic_after_connect(
    packet_len: nat,
    max_len: nat,
    bound_player: int,
    claimed_player: int,
)
    requires
        bound_player > 0,
        claimed_player > 0,
        valid_packet_len(packet_len, max_len),
    ensures
        accept_packet(
            SessionState::Bound(bound_player),
            packet_len,
            max_len,
            false,
            claimed_player,
        ) == (true, SessionState::Bound(bound_player)),
        accept_packet(
            SessionState::Bound(bound_player),
            packet_len,
            max_len,
            true,
            claimed_player,
        ) == (false, SessionState::Bound(bound_player)),
{
}

}

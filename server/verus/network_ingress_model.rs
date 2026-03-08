use vstd::prelude::*;

fn main() {}

verus! {

enum SessionState {
    AwaitingConnect,
    Bound,
}

spec fn valid_packet_len(len: nat, max_len: nat) -> bool {
    len <= max_len
}

spec fn accept_packet(
    state: SessionState,
    packet_len: nat,
    max_len: nat,
    is_connect: bool,
) -> (result: (bool, SessionState))
{
    if !valid_packet_len(packet_len, max_len) {
        (false, state)
    } else {
        match state {
            SessionState::AwaitingConnect =>
                if is_connect {
                    (true, SessionState::AwaitingConnect)
                } else {
                    (false, SessionState::AwaitingConnect)
                },
            SessionState::Bound =>
                if is_connect {
                    (false, SessionState::Bound)
                } else {
                    (true, SessionState::Bound)
                },
        }
    }
}

proof fn first_packet_must_bind(
    packet_len: nat,
    max_len: nat,
)
    requires
        valid_packet_len(packet_len, max_len),
    ensures
        accept_packet(
            SessionState::AwaitingConnect,
            packet_len,
            max_len,
            true,
        ) == (true, SessionState::AwaitingConnect),
        accept_packet(
            SessionState::AwaitingConnect,
            packet_len,
            max_len,
            false,
        ) == (false, SessionState::AwaitingConnect),
{
}

spec fn mark_bound(state: SessionState) -> SessionState {
    match state {
        SessionState::AwaitingConnect => SessionState::Bound,
        SessionState::Bound => SessionState::Bound,
    }
}

proof fn connect_validation_precedes_binding(
    packet_len: nat,
    max_len: nat,
)
    requires
        valid_packet_len(packet_len, max_len),
    ensures
        accept_packet(
            SessionState::AwaitingConnect,
            packet_len,
            max_len,
            true,
        ) == (true, SessionState::AwaitingConnect),
        mark_bound(SessionState::AwaitingConnect) == SessionState::Bound,
        accept_packet(
            SessionState::Bound,
            packet_len,
            max_len,
            true,
        ) == (false, SessionState::Bound),
        accept_packet(
            SessionState::Bound,
            packet_len,
            max_len,
            false,
        ) == (true, SessionState::Bound),
{
}

}

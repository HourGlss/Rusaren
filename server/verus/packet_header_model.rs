use vstd::prelude::*;

fn main() {}

verus! {

spec fn has_minimum_header(packet_len: nat) -> bool {
    packet_len >= 16nat
}

spec fn payload_len_matches(packet_len: nat, payload_len: nat) -> bool
    recommends
        has_minimum_header(packet_len),
{
    payload_len == packet_len - 16nat
}

spec fn header_is_accepted(
    packet_len: nat,
    payload_len: nat,
    has_expected_magic: bool,
    has_expected_version: bool,
) -> bool {
    has_minimum_header(packet_len)
        && has_expected_magic
        && has_expected_version
        && payload_len_matches(packet_len, payload_len)
}

proof fn truncated_packets_are_rejected(packet_len: nat, payload_len: nat)
    requires
        packet_len < 16nat,
    ensures
        !header_is_accepted(packet_len, payload_len, true, true),
{
}

proof fn exact_length_headers_are_accepted(
    packet_len: nat,
    payload_len: nat,
)
    requires
        has_minimum_header(packet_len),
        payload_len_matches(packet_len, payload_len),
    ensures
        header_is_accepted(packet_len, payload_len, true, true),
{
}

proof fn magic_or_version_mismatch_rejects_even_well_formed_lengths(
    packet_len: nat,
    payload_len: nat,
)
    requires
        has_minimum_header(packet_len),
        payload_len_matches(packet_len, payload_len),
    ensures
        !header_is_accepted(packet_len, payload_len, false, true),
        !header_is_accepted(packet_len, payload_len, true, false),
{
}

proof fn payload_mismatch_is_rejected_even_with_good_magic_and_version(
    packet_len: nat,
    payload_len: nat,
)
    requires
        has_minimum_header(packet_len),
        payload_len != packet_len - 16nat,
    ensures
        !header_is_accepted(packet_len, payload_len, true, true),
{
}

}

use vstd::prelude::*;

fn main() {}

verus! {

enum ParseOutcome {
    Accept,
    RejectMalformed,
    RejectDuplicate,
}

spec fn valid_record_store_size(size: nat, maximum: nat) -> bool {
    size <= maximum
}

spec fn valid_field_count(field_count: nat) -> bool {
    field_count == 5
}

spec fn parse_record_line(
    field_count: nat,
    player_id: int,
    name_valid: bool,
    counters_fit_u16: bool,
    duplicate_id: bool,
) -> ParseOutcome {
    if !valid_field_count(field_count) || player_id <= 0 || !name_valid || !counters_fit_u16 {
        ParseOutcome::RejectMalformed
    } else if duplicate_id {
        ParseOutcome::RejectDuplicate
    } else {
        ParseOutcome::Accept
    }
}

proof fn oversized_record_store_files_are_rejected(size: nat, maximum: nat)
    requires
        maximum > 0,
        size > maximum,
    ensures
        !valid_record_store_size(size, maximum),
{
}

proof fn malformed_lines_are_rejected_before_duplicate_checks(
    field_count: nat,
    player_id: int,
    duplicate_id: bool,
)
    requires
        field_count != 5 || player_id <= 0,
    ensures
        parse_record_line(field_count, player_id, true, true, duplicate_id)
            == ParseOutcome::RejectMalformed,
{
}

proof fn duplicate_player_ids_are_rejected_when_other_fields_are_valid(player_id: int)
    requires
        player_id > 0,
    ensures
        parse_record_line(5, player_id, true, true, true) == ParseOutcome::RejectDuplicate,
        parse_record_line(5, player_id, true, true, false) == ParseOutcome::Accept,
{
}

}

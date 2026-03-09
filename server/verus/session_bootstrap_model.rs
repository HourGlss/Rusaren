use vstd::prelude::*;

fn main() {}

verus! {

spec fn token_present(issued: bool, expired: bool) -> bool {
    issued && !expired
}

spec fn consume_token(issued: bool, expired: bool) -> (bool, bool) {
    if token_present(issued, expired) {
        (true, false)
    } else {
        (false, false)
    }
}

proof fn valid_token_is_consumed_once()
    ensures
        consume_token(true, false) == (true, false),
        consume_token(false, false) == (false, false),
        consume_token(true, true) == (false, false),
{
}

proof fn second_use_always_fails()
    ensures
        consume_token(true, false).1 == false,
        consume_token(consume_token(true, false).1, false) == (false, false),
{
}

}

use vstd::prelude::*;

fn main() {}

verus! {

enum SignalMessage {
    Offer,
    IceCandidate,
    Bye,
}

spec fn offer_seen(history: Seq<SignalMessage>) -> bool {
    exists|i: int| 0 <= i < history.len() && history[i] == SignalMessage::Offer
}

spec fn duplicate_offer(history: Seq<SignalMessage>) -> bool {
    exists|i: int, j: int|
        0 <= i < j < history.len()
        && history[i] == SignalMessage::Offer
        && history[j] == SignalMessage::Offer
}

spec fn candidate_before_offer(history: Seq<SignalMessage>) -> bool {
    exists|i: int|
        0 <= i < history.len()
        && history[i] == SignalMessage::IceCandidate
        && !offer_seen(history.take(i as int))
}

spec fn signaling_history_is_accepted(history: Seq<SignalMessage>) -> bool {
    !duplicate_offer(history) && !candidate_before_offer(history)
}

proof fn offer_only_histories_are_accepted(history: Seq<SignalMessage>)
    requires
        forall|i: int| 0 <= i < history.len() ==> history[i] == SignalMessage::Offer,
        history.len() <= 1,
    ensures
        signaling_history_is_accepted(history),
{
}

proof fn duplicate_offers_are_rejected(history: Seq<SignalMessage>)
    requires
        duplicate_offer(history),
    ensures
        !signaling_history_is_accepted(history),
{
}

proof fn candidates_before_offers_are_rejected(history: Seq<SignalMessage>)
    requires
        candidate_before_offer(history),
    ensures
        !signaling_history_is_accepted(history),
{
}

proof fn bye_messages_do_not_change_offer_or_candidate_safety(history: Seq<SignalMessage>)
    requires
        signaling_history_is_accepted(history),
    ensures
        signaling_history_is_accepted(history.push(SignalMessage::Bye)),
{
    assert(!duplicate_offer(history.push(SignalMessage::Bye))) by {
        assert forall|i: int, j: int|
            0 <= i < j < history.push(SignalMessage::Bye).len()
            && history.push(SignalMessage::Bye)[i] == SignalMessage::Offer
            && history.push(SignalMessage::Bye)[j] == SignalMessage::Offer
            implies false by {
                if j == history.len() {
                    assert(history.push(SignalMessage::Bye)[j] == SignalMessage::Bye);
                } else {
                    assert(j < history.len());
                    assert(history.push(SignalMessage::Bye)[i] == history[i]);
                    assert(history.push(SignalMessage::Bye)[j] == history[j]);
                    assert(duplicate_offer(history));
                }
            };
    }

    assert(!candidate_before_offer(history.push(SignalMessage::Bye))) by {
        assert forall|i: int|
            0 <= i < history.push(SignalMessage::Bye).len()
            && history.push(SignalMessage::Bye)[i] == SignalMessage::IceCandidate
            && !offer_seen(history.push(SignalMessage::Bye).take(i as int))
            implies false by {
                if i == history.len() {
                    assert(history.push(SignalMessage::Bye)[i] == SignalMessage::Bye);
                } else {
                    assert(i < history.len());
                    assert(history.push(SignalMessage::Bye)[i] == history[i]);
                    assert(history.push(SignalMessage::Bye).take(i as int) == history.take(i as int));
                    assert(candidate_before_offer(history));
                }
            };
    }
}

proof fn candidates_after_a_single_offer_remain_accepted(history: Seq<SignalMessage>)
    requires
        signaling_history_is_accepted(history),
        offer_seen(history),
        !duplicate_offer(history.push(SignalMessage::IceCandidate)),
    ensures
        signaling_history_is_accepted(history.push(SignalMessage::IceCandidate)),
{
    assert(!candidate_before_offer(history.push(SignalMessage::IceCandidate))) by {
        assert forall|i: int|
            0 <= i < history.push(SignalMessage::IceCandidate).len()
            && history.push(SignalMessage::IceCandidate)[i] == SignalMessage::IceCandidate
            && !offer_seen(history.push(SignalMessage::IceCandidate).take(i as int))
            implies false by {
                if i == history.len() {
                    assert(
                        history.push(SignalMessage::IceCandidate).take(i as int) == history
                    );
                    assert(offer_seen(history.push(SignalMessage::IceCandidate).take(i as int)));
                } else {
                    assert(i < history.len());
                    assert(history.push(SignalMessage::IceCandidate)[i] == history[i]);
                    assert(
                        history.push(SignalMessage::IceCandidate).take(i as int)
                            == history.take(i as int)
                    );
                    assert(candidate_before_offer(history));
                }
            };
    }
}

}

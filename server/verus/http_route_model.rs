use vstd::prelude::*;

fn main() {}

verus! {

enum HttpRouteLabel {
    Root,
    Healthz,
    Metrics,
    WebSocket,
    StaticAsset,
}

spec fn classify_http_route(
    is_root: bool,
    is_healthz: bool,
    is_metrics: bool,
    is_websocket: bool,
) -> HttpRouteLabel {
    if is_root {
        HttpRouteLabel::Root
    } else if is_healthz {
        HttpRouteLabel::Healthz
    } else if is_metrics {
        HttpRouteLabel::Metrics
    } else if is_websocket {
        HttpRouteLabel::WebSocket
    } else {
        HttpRouteLabel::StaticAsset
    }
}

proof fn exact_routes_map_to_their_expected_labels()
    ensures
        classify_http_route(true, false, false, false) == HttpRouteLabel::Root,
        classify_http_route(false, true, false, false) == HttpRouteLabel::Healthz,
        classify_http_route(false, false, true, false) == HttpRouteLabel::Metrics,
        classify_http_route(false, false, false, true) == HttpRouteLabel::WebSocket,
{
}

proof fn unknown_routes_fall_back_to_static_assets()
    ensures
        classify_http_route(false, false, false, false) == HttpRouteLabel::StaticAsset,
{
}

proof fn exact_routes_keep_their_precedence_even_if_multiple_flags_are_set()
    ensures
        classify_http_route(true, true, true, true) == HttpRouteLabel::Root,
        classify_http_route(false, true, true, true) == HttpRouteLabel::Healthz,
        classify_http_route(false, false, true, true) == HttpRouteLabel::Metrics,
        classify_http_route(false, false, false, true) == HttpRouteLabel::WebSocket,
{
}

}

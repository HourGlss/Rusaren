use super::*;

#[test]
fn bootstrap_token_shape_is_url_safe_and_bounded() {
    assert!(is_valid_session_bootstrap_token_shape("abc_DEF-123"));
    assert!(!is_valid_session_bootstrap_token_shape(""));
    assert!(!is_valid_session_bootstrap_token_shape("bad token"));
    assert!(!is_valid_session_bootstrap_token_shape(&"A".repeat(128)));
}

#[test]
fn bootstrap_tokens_are_one_time_use_and_expire() {
    let now = Instant::now();
    let mut registry = SessionBootstrapTokenRegistry::new(Duration::from_millis(50));
    let token = registry.mint(now).expect("token should mint");

    assert_eq!(registry.consume(&token, now), Ok(()));
    assert_eq!(
        registry.consume(&token, now),
        Err("session bootstrap token is missing or already consumed")
    );

    let expired = registry.mint(now).expect("token should mint");
    assert_eq!(
        registry.consume(&expired, now + Duration::from_millis(60)),
        Err("session bootstrap token is missing or already consumed")
    );
}

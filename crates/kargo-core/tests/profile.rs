use kargo_core::profile::Profile;

#[test]
fn profile_dev() {
    let p = Profile::dev();
    assert_eq!(p.debug, Some(true));
    assert_eq!(p.optimization, Some(false));
    assert!(p.compiler_args.is_empty());
}

#[test]
fn profile_release() {
    let p = Profile::release();
    assert_eq!(p.debug, Some(false));
    assert_eq!(p.optimization, Some(true));
    assert!(p.compiler_args.is_empty());
}

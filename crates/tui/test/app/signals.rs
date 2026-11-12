type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::signals;

#[test]
fn install_accepts_the_exit_signals() -> Result<()> {
    // -- Exec
    signals::install()?;

    // -- Check
    Ok(())
}

#[test]
fn install_reports_an_invalid_signal() {
    // -- Exec
    let result = signals::install_for(&[-1]);

    // -- Check
    assert!(
        result.is_err(),
        "an invalid signal number must fail install, got {result:?}"
    );
}

#[test]
fn sigint_is_recorded() -> Result<()> {
    // -- Setup & Fixtures
    signals::install()?;

    // -- Exec & Check
    // SAFETY: `raise` delivers SIGINT to the calling thread; the installed
    // handler only stores into the atomic flag, which is async-signal-safe.
    let raised = unsafe { libc::raise(libc::SIGINT) };
    assert_eq!(raised, 0, "raise(SIGINT) should succeed");
    assert!(
        signals::termination_requested(),
        "SIGINT should be recorded"
    );
    Ok(())
}

#[test]
fn sighup_is_recorded() -> Result<()> {
    // -- Setup & Fixtures
    signals::install()?;

    // -- Exec & Check
    // SAFETY: `raise` delivers SIGHUP to the calling thread; the installed
    // handler only stores into the atomic flag, which is async-signal-safe.
    let raised = unsafe { libc::raise(libc::SIGHUP) };
    assert_eq!(raised, 0, "raise(SIGHUP) should succeed");
    assert!(
        signals::termination_requested(),
        "SIGHUP should be recorded"
    );
    Ok(())
}

#[test]
fn sigterm_is_recorded() -> Result<()> {
    // -- Setup & Fixtures
    signals::install()?;

    // -- Exec & Check
    // SAFETY: `raise` delivers SIGTERM to the calling thread; the installed
    // handler only stores into the atomic flag, which is async-signal-safe.
    let raised = unsafe { libc::raise(libc::SIGTERM) };
    assert_eq!(raised, 0, "raise(SIGTERM) should succeed");
    assert!(
        signals::termination_requested(),
        "SIGTERM should be recorded"
    );
    Ok(())
}

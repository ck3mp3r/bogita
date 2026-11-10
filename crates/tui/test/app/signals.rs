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
fn every_termination_signal_is_recorded() -> Result<()> {
    // -- Setup & Fixtures
    signals::install()?;

    // -- Exec & Check
    for signal in [libc::SIGINT, libc::SIGHUP, libc::SIGTERM] {
        signals::reset();
        // SAFETY: `raise` delivers `signal` to the calling thread; the installed
        // handler only stores into the atomic flag, which is async-signal-safe.
        let raised = unsafe { libc::raise(signal) };
        assert_eq!(raised, 0, "raise({signal}) should succeed");
        assert!(
            signals::termination_requested(),
            "signal {signal} should be recorded"
        );
    }

    // -- Cleanup
    signals::reset();
    Ok(())
}

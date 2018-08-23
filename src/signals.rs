use libc;
use pstate::set_original_pstate;
use std::process::exit;

/// Set the original pstate when the daemon exits.
pub fn daemon_signal_handler() {
    extern "C" fn handler(signal: i32) {
        set_original_pstate();
        exit(127 + signal);
    }

    let handler = handler as libc::sighandler_t;
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGQUIT, handler);
        libc::signal(libc::SIGTERM, handler);
        libc::signal(libc::SIGPIPE, handler);
    }
}

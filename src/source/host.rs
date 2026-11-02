//! This machine's hostname, so a webhook serving several machines knows where a notification came from.
use rustix::system::uname;

/// The hostname (`uname -n`), as text.
pub(crate) fn host_name() -> String {
    uname().nodename().to_string_lossy().into_owned()
}

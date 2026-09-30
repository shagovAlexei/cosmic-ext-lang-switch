// SPDX-License-Identifier: GPL-3.0-only
//! File descriptors passed down through the environment.
use std::os::fd::RawFd;

/// Set by the applet for the daemon it starts: the fd of the privileged Wayland socket.
/// Not `X_PRIVILEGED_WAYLAND_SOCKET` itself: see [`socket_fd`].
pub const DAEMON_WAYLAND_FD: &str = "COSMIC_EXT_LANG_SWITCH_WAYLAND_FD";

/// The fd number in an env var value, if it names an open socket of this process.
/// `X_PRIVILEGED_WAYLAND_SOCKET` stays in the environment of everything launched from
/// the panel's applets while the socket itself doesn't, so the number alone proves nothing.
pub fn socket_fd(var: Option<&str>) -> Option<RawFd> {
    let fd: RawFd = var?.parse().ok()?;
    let meta = std::fs::metadata(format!("/proc/self/fd/{fd}")).ok()?;
    std::os::unix::fs::FileTypeExt::is_socket(&meta.file_type()).then_some(fd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;

    #[test]
    fn socket_fd_accepts_only_an_open_socket() {
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        let fd = a.as_raw_fd();
        assert_eq!(socket_fd(Some(&fd.to_string())), Some(fd));
        let file = std::fs::File::open("/proc/self/stat").unwrap();
        assert_eq!(
            socket_fd(Some(&file.as_raw_fd().to_string())),
            None,
            "not a socket"
        );
        assert_eq!(socket_fd(Some("99999")), None, "closed (leaked env var)");
        assert_eq!(socket_fd(Some("")), None);
        assert_eq!(socket_fd(None), None);
    }
}

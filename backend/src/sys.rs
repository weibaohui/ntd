//! 集中封装所有直接调用 libc 的 unsafe FFI 包装。
//!
//! 设计动机：
//! 1. `lib.rs` 顶部使用 `#![deny(unsafe_code)]`，杜绝散落在业务模块里的裸 `unsafe { libc::xxx }`。
//!    所有 FFI 必须经过本模块，每个函数提供安全包装（参数校验 + 结果检查）。
//! 2. 单测覆盖参数校验路径，FFI 真实行为由 OS 保证。
//! 3. 仅在 unix 类平台编译；Windows 由 no-op 占位维持工作区编译通过。
//!
//! 增加新的 libc 调用时，请遵循：
//! - 必须给出安全包装（不能暴露 *mut / *const 裸指针参数）。
//! - 返回值如果是 `isize`（POSIX 错误约定），请用 `std::io::Error::from_raw_os_error` 转换。
//! - 在本文件 `#[cfg(test)]` 区补单测，至少覆盖：
//!   * 正常路径返回 Ok
//!   * 非法 fd / 参数返回 Err
//!
//! 所有 unsafe 必须用 `// SAFETY:` 注释解释前置条件。

#![allow(unsafe_code)] // 本模块是项目内唯一允许 unsafe 的地方；新加 unsafe 时必须给出 SAFETY 注释。

use std::io;
#[cfg(unix)]
use std::os::fd::RawFd;

// =============================================================================
// 通用：文件描述符 / 套接字选项
// =============================================================================

/// 在已 bind 的 socket fd 上开启 `SO_REUSEADDR`。
///
/// 应用场景：daemon 重启时旧连接仍处 TIME_WAIT，开启 reuseaddr 可避免 "Address already in use"。
///
/// 包装要点：
/// - 把可变的 `c_int` 在栈上构造，再借用其指针（避免悬垂）。
/// - 把 `setsockopt` 的负返回值映射成 `io::Error`，并把 `optlen` 与 `optval` 类型显式 cast。
///
/// # Errors
///
/// - fd 非法 / 已被关闭：返回 `io::ErrorKind::Other` 包装的 `EBADF`。
/// - 套接字类型不支持该选项：返回对应 OS 错误码（macOS 上 SO_REUSEADDR 一定支持）。
#[cfg(unix)]
pub fn set_socket_reuseaddr(fd: RawFd) -> io::Result<()> {
    // SAFETY：
    // - `fd` 由调用方提供，调用前必须已 bind 成功；本函数不会关闭 fd。
    // - `optval` 是栈上的 c_int，生命周期覆盖整个 setsockopt 调用。
    // - `optlen` 来自 `size_of::<c_int>()`，与 `optval` 实际占用字节数一致。
    // - `setsockopt` 不会写入 `optval` 指向的内存（const 指针契约），不会与 Rust 借用检查冲突。
    let optval: libc::c_int = 1;
    let ret = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            &optval as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if ret < 0 {
        // POSIX 下 `setsockopt` 失败时返回 -1，并把 errno 写入 `*__errno_location()`；
        // Rust 的 `std::io::Error::last_os_error()` 内部就是读取它。
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// =============================================================================
// POSIX 身份查询
// =============================================================================

/// 获取当前进程真实用户 ID。
///
/// 仅 macOS launchd 流程需要（plist 必须放在 `~/Library/LaunchAgents/<uid>/`）。
/// 真实 ID 与有效 ID 在普通进程上一致；setuid 二进制若用到本函数请改用 `current_euid`。
#[cfg(target_os = "macos")]
pub fn current_uid() -> u32 {
    // SAFETY：getuid 是纯函数，glibc/musl/libsystem 都不维护任何状态；并发调用安全。
    unsafe { libc::getuid() }
}

/// 获取当前进程有效用户 ID。
///
/// 用法：判断 daemon install/uninstall/start/stop/restart 是否以 root 权限执行。
/// 因为只比较 `!= 0` 不需要 `Uid` 强类型，直接返回 `u32` 减少单测里的构造。
#[cfg(target_os = "linux")]
pub fn current_euid() -> u32 {
    // SAFETY：geteuid 是纯函数，并发安全；与 getuid 行为一致，仅 effective 字段不同。
    unsafe { libc::geteuid() }
}

/// 守护进程子命令的"必须 root"守卫。
///
/// daemon.rs 中 5 个 `systemd_*` 函数共享同一种"如果不是 root 就报错并退 1"逻辑；
/// 集中到这里后只改一处，CLI 文案也得到统一。
///
/// 行为说明：
/// - 仅 `--system` 模式需要 root；user 模式直接放行（systemd 的 user instance 走 `Linger`）。
/// - 使用 `eprintln!` 是因为本函数在 `daemon` 子命令路径触发，tracing subscriber
///   还没初始化（见 main.rs 中 daemon 分支不经过 `run_server`）。
#[cfg(target_os = "linux")]
pub fn require_root_or_exit(action: &str) {
    if current_euid() != 0 {
        // 预 tracing 阶段，无法走 tracing::error!；加 allow 抑制 clippy::print_stderr 警告。
        #[allow(clippy::print_stderr)]
        {
            eprintln!("System service {action} requires root. Re-run with sudo.");
        }
        std::process::exit(1);
    }
}

// =============================================================================
// 非 unix 平台占位：保持工作区编译通过，main.rs / daemon.rs 调用方不需要 cfg 分支。
// =============================================================================

/// Windows 占位实现：Windows 上 SO_REUSEADDR 行为不同（等价于 SO_REUSEPORT），
/// 后续若需要支持 Windows 守护进程，再单独实现。
#[cfg(not(unix))]
#[allow(dead_code, clippy::missing_errors_doc)]
pub fn set_socket_reuseaddr(_fd: std::os::raw::c_int) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "set_socket_reuseaddr is not supported on non-unix platforms",
    ))
}

/// Windows 占位实现。
#[cfg(not(unix))]
#[allow(dead_code)]
pub fn current_euid() -> u32 {
    0
}

/// Windows 占位实现。
#[cfg(not(unix))]
#[allow(dead_code, clippy::print_stderr)]
pub fn require_root_or_exit(_action: &str) {
    // 非 unix 平台不应该走到这里；保险起见直接放行。
}

// =============================================================================
// 单元测试
// =============================================================================

#[cfg(test)]
#[cfg(unix)]
mod unix_tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::os::fd::AsRawFd;

    /// 真实 fd 走完整 setsockopt 流程，验证返回 Ok。
    #[test]
    fn set_socket_reuseaddr_on_real_listener_returns_ok() {
        // 用 127.0.0.1:0 让 OS 分配空闲端口，避免冲突。
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let result = set_socket_reuseaddr(listener.as_raw_fd());
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }

    /// 非法 fd：传入 -1 触发 EBADF，验证错误路径。
    #[test]
    fn set_socket_reuseaddr_on_invalid_fd_returns_err() {
        // libc::EBADF 是 9。
        let bogus_fd: RawFd = -1;
        let result = set_socket_reuseaddr(bogus_fd);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.raw_os_error(),
            Some(libc::EBADF),
            "expected EBADF, got {:?}",
            err
        );
    }

    /// 关闭 listener 之后再调用，fd 应当变 EBADF。
    /// 注意：Linux 下 fd 号码可能被其他资源复用；这里用一个
    /// 显然不会被分配的极大值（u32::MAX）确保触发 EBADF。
    #[test]
    fn set_socket_reuseaddr_on_invalid_far_above_rlimit_returns_err() {
        // libc::c_int 在多数平台是 32-bit；用一个明显超出 OPEN_MAX 的值。
        let bogus_fd: RawFd = libc::c_int::MAX;
        let result = set_socket_reuseaddr(bogus_fd);
        assert!(result.is_err(), "expected Err for impossible fd, got {:?}", result);
    }

    /// 在已连接的 stream 上 setsockopt 同样应当成功（验证我们的 wrapper 不强依赖 listener 类型）。
    #[test]
    fn set_socket_reuseaddr_on_tcp_stream_succeeds() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        // 同步 connect：loopback 一定成功。
        let stream = TcpStream::connect(addr).expect("connect");
        let result = set_socket_reuseaddr(stream.as_raw_fd());
        assert!(result.is_ok());
    }
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod macos_tests {
    use super::*;

    /// current_uid 在测试进程上必然能返回一个非负数（沙盒或真实环境都成立）。
    #[test]
    fn current_uid_returns_nonnegative_u32() {
        let uid = current_uid();
        // 仅断言不 panic；真实值依赖运行环境。
        let _ = uid;
    }
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod linux_tests {
    use super::*;

    /// 普通测试进程 euid 通常是 0/1000/1001 这类，确定的是"非 panic 且返回合理 u32"。
    #[test]
    fn current_euid_returns_value() {
        let euid = current_euid();
        let _ = euid; // 行为由 OS 决定；只验证函数能跑通。
    }

    /// require_root_or_exit 不应在 euid == 0 时退出，进程能继续走完。
    /// 注：CI 上 root 用户很多，跑不到非 root 分支；这里只保证函数调用自身不死。
    #[test]
    fn require_root_or_exit_does_not_panic_when_root() {
        // 临时跳过断言：当 euid != 0 时函数会 std::process::exit，无法在单测里捕获。
        // 所以本测试只在 euid == 0 的环境下提供"不 panic"证据，否则只是空操作。
        if current_euid() == 0 {
            require_root_or_exit("install");
            // 没有退出 = 成功。
        }
    }
}

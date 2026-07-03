//! Резолв «хто зробив запит» для історії проксі.
//!
//! HTTP-запит сам по собі не каже, який процес його надіслав. Але проксі
//! слухає лише `127.0.0.1`, тож у кожного з'єднання відомий локальний порт
//! клієнта — за ним у таблиці TCP-сокетів системи знаходимо PID
//! процесу-власника, а за PID через libproc читаємо назву, шлях до бінарника
//! і поточну робочу директорію (cwd). Резолв треба робити, поки з'єднання ще
//! відкрите (тобто на старті запиту, а не після відповіді).
//!
//! Працює лише на macOS; на інших платформах `resolve` повертає `None`.

use serde::Serialize;

/// Дані про процес-клієнт, який зробив запит через проксі.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub pid: i32,
    /// Ім'я процесу (може бути обрізане системою до 16-32 символів).
    pub name: Option<String>,
    /// Повний шлях до виконуваного файлу.
    pub exe: Option<String>,
    /// Поточна робоча директорія процесу — «звідки» був запит.
    pub cwd: Option<String>,
}

/// Знаходить процес, що тримає клієнтський бік TCP-з'єднання
/// `127.0.0.1:client_port -> 127.0.0.1:proxy_port`, і збирає його дані.
/// Блокуючий виклик (скан таблиці сокетів) — викликати через `spawn_blocking`.
#[cfg(target_os = "macos")]
pub fn resolve(client_port: u16, proxy_port: u16) -> Option<ClientInfo> {
    let pid = pid_by_tcp_ports(client_port, proxy_port)?;
    Some(info_for_pid(pid))
}

#[cfg(not(target_os = "macos"))]
pub fn resolve(_client_port: u16, _proxy_port: u16) -> Option<ClientInfo> {
    None
}

/// Шукає в таблиці TCP-сокетів з'єднання, у якого local port — порт клієнта,
/// а remote port — порт проксі (щоб не сплутати з нашим власним accepted
/// сокетом, у якого порти дзеркальні), і повертає PID власника.
#[cfg(target_os = "macos")]
fn pid_by_tcp_ports(client_port: u16, proxy_port: u16) -> Option<i32> {
    use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};

    let sockets = netstat2::get_sockets_info(
        AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6,
        ProtocolFlags::TCP,
    )
    .ok()?;
    sockets
        .into_iter()
        .find_map(|s| match &s.protocol_socket_info {
            ProtocolSocketInfo::Tcp(tcp)
                if tcp.local_port == client_port && tcp.remote_port == proxy_port =>
            {
                #[allow(clippy::cast_possible_wrap)]
                s.associated_pids.first().map(|&pid| pid as i32)
            }
            _ => None,
        })
}

#[cfg(target_os = "macos")]
fn info_for_pid(pid: i32) -> ClientInfo {
    use libproc::proc_pid;

    let exe = proc_pid::pidpath(pid).ok();
    // proc_name інколи відмовляє або обрізає ім'я — тоді беремо basename шляху.
    let name = proc_pid::name(pid).ok().or_else(|| {
        exe.as_deref().and_then(|p| {
            std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
    });
    ClientInfo {
        pid,
        name,
        exe,
        cwd: pid_cwd(pid),
    }
}

/// `struct proc_vnodepathinfo` з `<sys/proc_info.h>`: два `vnode_info_path`
/// (cwd і chroot-root), кожен — `struct vnode_info` (136 байт `vinfo_stat` +
/// `vi_type`/`vi_pad`/`vi_fsid` = 152) + `char vip_path[MAXPATHLEN=1024]`.
/// libproc-крейт не експортує цю структуру, а його `pidcwd` для macOS не
/// реалізований, тому описуємо layout самі; читаємо лише `pvi_cdir.vip_path`,
/// тож службові поля тримаємо як байтові масиви.
#[cfg(target_os = "macos")]
#[repr(C)]
struct VnodePathInfo {
    cdir_vnode_info: [u8; 152],
    cdir_path: [u8; 1024],
    rdir: [u8; 152 + 1024],
}

#[cfg(target_os = "macos")]
impl libproc::proc_pid::PIDInfo for VnodePathInfo {
    fn flavor() -> libproc::proc_pid::PidInfoFlavor {
        libproc::proc_pid::PidInfoFlavor::VNodePathInfo
    }
}

#[cfg(target_os = "macos")]
fn pid_cwd(pid: i32) -> Option<String> {
    let info = libproc::proc_pid::pidinfo::<VnodePathInfo>(pid, 0).ok()?;
    let len = info
        .cdir_path
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(info.cdir_path.len());
    let path = String::from_utf8_lossy(&info.cdir_path[..len]).to_string();
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn resolves_own_process_cwd() {
        let me = std::process::id() as i32;
        let info = info_for_pid(me);
        assert_eq!(info.pid, me);
        let expected = std::env::current_dir().unwrap();
        assert_eq!(info.cwd.as_deref(), Some(expected.to_str().unwrap()));
        assert!(info.exe.is_some());
        assert!(info.name.is_some());
    }

    #[test]
    fn resolves_pid_by_tcp_connection() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_port = listener.local_addr().unwrap().port();
        let client = std::net::TcpStream::connect(("127.0.0.1", proxy_port)).unwrap();
        let (_accepted, _) = listener.accept().unwrap();
        let client_port = client.local_addr().unwrap().port();

        let info = resolve(client_port, proxy_port).expect("connection should be resolvable");
        assert_eq!(info.pid, std::process::id() as i32);
        assert!(info.cwd.is_some());
    }
}

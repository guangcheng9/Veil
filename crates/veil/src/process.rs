use std::path::{Path, PathBuf};

use crate::Error;
use serde::{Deserialize, Serialize};

/// Runtime identity and the listeners that are actually owned by this process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMetadata {
    pub pid: u32,
    #[serde(default)]
    pub process_started_at: u64,
    #[serde(default)]
    pub instance_id: String,
    #[serde(default, alias = "exe")]
    pub exe_path: String,
    pub management_port: u16,
    #[serde(default)]
    pub listener_ports: Vec<u16>,
    #[serde(default)]
    pub proxy_state: String,
    #[serde(default)]
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_lifecycle_error: Option<String>,
}

pub type PidFile = RuntimeMetadata;

pub fn runtime_path(data_dir: &Path) -> PathBuf {
    data_dir.join("veil.pid")
}

pub fn pid_path(data_dir: &Path) -> PathBuf {
    runtime_path(data_dir)
}

pub fn current_exe() -> String {
    std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| String::from("veil"))
}

pub fn current_process_started_at() -> u64 {
    process_started_at(std::process::id()).unwrap_or_default()
}

pub fn write_runtime(data_dir: &Path, file: &RuntimeMetadata) -> Result<(), Error> {
    std::fs::create_dir_all(data_dir)?;
    let path = runtime_path(data_dir);
    let tmp = data_dir.join(format!(".veil.pid.{}.tmp", std::process::id()));
    let json = serde_json::to_vec(file)?;
    {
        use std::io::Write;
        let mut out = std::fs::File::create(&tmp)?;
        out.write_all(&json)?;
        out.sync_all()?;
    }
    atomic_replace(&tmp, &path)?;
    Ok(())
}

pub fn write_pid(data_dir: &Path, file: &PidFile) -> Result<(), Error> {
    write_runtime(data_dir, file)
}

pub fn read_runtime(data_dir: &Path) -> Result<Option<RuntimeMetadata>, Error> {
    match std::fs::read_to_string(runtime_path(data_dir)) {
        Ok(text) => Ok(Some(serde_json::from_str(text.trim())?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn read_pid(data_dir: &Path) -> Result<Option<PidFile>, Error> {
    read_runtime(data_dir)
}

pub fn remove_runtime_if_instance(data_dir: &Path, instance_id: &str) -> Result<(), Error> {
    if let Some(meta) = read_runtime(data_dir)? {
        if meta.instance_id != instance_id {
            return Ok(());
        }
    } else {
        return Ok(());
    }
    match std::fs::remove_file(runtime_path(data_dir)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn remove_stale_runtime(data_dir: &Path) -> Result<bool, Error> {
    let Some(meta) = read_runtime(data_dir)? else {
        return Ok(false);
    };
    if metadata_matches_process(&meta) {
        return Ok(false);
    }
    std::fs::remove_file(runtime_path(data_dir))?;
    Ok(true)
}

pub fn metadata_matches_process(meta: &RuntimeMetadata) -> bool {
    if !pid_is_alive(meta.pid) {
        return false;
    }
    if meta.process_started_at != 0
        && process_started_at(meta.pid).is_some_and(|v| v != meta.process_started_at)
    {
        return false;
    }
    let Some(actual) = process_exe_path(meta.pid) else {
        return false;
    };
    paths_equal(&actual, Path::new(&meta.exe_path))
}

pub fn pid_is_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        win::is_alive(pid)
    }
    #[cfg(unix)]
    {
        unix::is_alive(pid)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = pid;
        false
    }
}

pub fn process_started_at(pid: u32) -> Option<u64> {
    #[cfg(windows)]
    {
        win::started_at(pid)
    }
    #[cfg(unix)]
    {
        unix::started_at(pid)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = pid;
        None
    }
}

pub fn process_exe_path(pid: u32) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        win::exe_path(pid)
    }
    #[cfg(unix)]
    {
        unix::exe_path(pid)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = pid;
        None
    }
}

pub fn port_owner_pid(port: u16) -> Option<u32> {
    #[cfg(windows)]
    {
        win::port_owner_pid(port)
    }
    #[cfg(not(windows))]
    {
        let _ = port;
        None
    }
}

pub fn describe_port_owner(port: u16) -> String {
    match port_owner_pid(port) {
        Some(pid) => {
            let exe = process_exe_path(pid)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "unknown executable".into());
            format!("port {port} is occupied by PID {pid} ({exe})")
        }
        None => format!("port {port} is already in use (owner could not be determined)"),
    }
}

/// Safely terminate a process whose identity is fully confirmed as this Veil
/// instance. Returns `Ok(true)` if terminated, `Ok(false)` if identity could not
/// be confirmed (never kills in that case).
///
/// `api_instance_id` carries the result of probing the management API:
/// - `None`: probe was not performed or API was unresponsive — design §4.3
///   condition 5 treats "API unresponsive" as satisfying the recovery gate.
/// - `Some(id)`: API responded; `id` must equal `meta.instance_id`, otherwise
///   the port owner is a *different* Veil instance and must not be killed.
pub fn terminate_confirmed(
    meta: &RuntimeMetadata,
    api_instance_id: Option<&str>,
) -> Result<bool, Error> {
    if meta.instance_id.is_empty()
        || meta.process_started_at == 0
        || meta.listener_ports.is_empty()
        || !meta.listener_ports.contains(&meta.management_port)
    {
        return Ok(false);
    }
    if !metadata_matches_process(meta) {
        return Ok(false);
    }
    if meta
        .listener_ports
        .iter()
        .any(|port| port_owner_pid(*port) != Some(meta.pid))
    {
        return Ok(false);
    }
    // Condition 5: if the management API responded, it must report the same
    // instance. A mismatch means the live owner of these ports is a different
    // Veil (different data dir / install) — never auto-kill it.
    if let Some(api_id) = api_instance_id {
        if api_id != meta.instance_id {
            return Ok(false);
        }
    }
    kill_pid(meta.pid)?;
    Ok(true)
}

/// Spawn this binary as `veil start --foreground` detached from the console.
pub fn spawn_daemon(exe: &str) -> Result<(), Error> {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("start").arg("--foreground");
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const DETACHED_PROCESS: u32 = 0x00000008;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }
    cmd.spawn()?;
    Ok(())
}

#[derive(Debug)]
pub struct InstanceLock {
    #[cfg(windows)]
    handle: *mut std::ffi::c_void,
    #[cfg(unix)]
    file: std::fs::File,
}

pub fn acquire_instance_lock(data_dir: &Path) -> std::io::Result<InstanceLock> {
    acquire_named_lock(data_dir, "instance")
}

pub fn acquire_startup_lock(data_dir: &Path) -> std::io::Result<InstanceLock> {
    acquire_named_lock(data_dir, "startup")
}

fn acquire_named_lock(data_dir: &Path, purpose: &str) -> std::io::Result<InstanceLock> {
    std::fs::create_dir_all(data_dir)?;
    #[cfg(windows)]
    {
        win::acquire_lock(data_dir, purpose)
    }
    #[cfg(unix)]
    {
        unix::acquire_lock(data_dir, purpose)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (data_dir, purpose);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "single-instance locking is not supported on this platform",
        ))
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        #[cfg(windows)]
        win::release_lock(self.handle);
        #[cfg(unix)]
        unix::release_lock(&self.file);
    }
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    let a = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let b = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    #[cfg(windows)]
    {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

fn kill_pid(pid: u32) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        win::terminate(pid)
    }
    #[cfg(unix)]
    {
        unix::terminate(pid)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = pid;
        Ok(())
    }
}

fn atomic_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        win::atomic_replace(from, to)
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to)
    }
}

pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = secs.div_euclid(86_400);
    let day_secs = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    let hour = day_secs / 3_600;
    let minute = (day_secs % 3_600) / 60;
    let second = day_secs % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(windows)]
mod win {
    use super::{InstanceLock, Path, PathBuf};
    use sha2::Digest;
    use sha2::Sha256;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const PROCESS_TERMINATE: u32 = 0x0001;
    const STILL_ACTIVE: u32 = 259;
    const ERROR_ALREADY_EXISTS: u32 = 183;
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    const AF_INET: u32 = 2;
    const TCP_TABLE_OWNER_PID_LISTENER: u32 = 3;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[repr(C)]
    struct TcpRowOwnerPid {
        state: u32,
        local_addr: u32,
        local_port: u32,
        remote_addr: u32,
        remote_port: u32,
        owning_pid: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetExitCodeProcess(handle: *mut c_void, exit_code: *mut u32) -> i32;
        fn TerminateProcess(handle: *mut c_void, exit_code: u32) -> i32;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn QueryFullProcessImageNameW(
            process: *mut c_void,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CreateMutexW(
            attributes: *mut c_void,
            initial_owner: i32,
            name: *const u16,
        ) -> *mut c_void;
        fn GetLastError() -> u32;
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    #[link(name = "iphlpapi")]
    extern "system" {
        fn GetExtendedTcpTable(
            table: *mut c_void,
            size: *mut u32,
            order: i32,
            family: u32,
            class: u32,
            reserved: u32,
        ) -> u32;
    }

    fn open(pid: u32, access: u32) -> *mut c_void {
        unsafe { OpenProcess(access, 0, pid) }
    }

    pub fn is_alive(pid: u32) -> bool {
        unsafe {
            let handle = open(pid, PROCESS_QUERY_LIMITED_INFORMATION);
            if handle.is_null() {
                return false;
            }
            let mut code = 0u32;
            let ok = GetExitCodeProcess(handle, &mut code);
            CloseHandle(handle);
            ok != 0 && code == STILL_ACTIVE
        }
    }

    pub fn started_at(pid: u32) -> Option<u64> {
        unsafe {
            let handle = open(pid, PROCESS_QUERY_LIMITED_INFORMATION);
            if handle.is_null() {
                return None;
            }
            let mut creation = FileTime::default();
            let mut exit = FileTime::default();
            let mut kernel = FileTime::default();
            let mut user = FileTime::default();
            let ok = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
            CloseHandle(handle);
            (ok != 0).then_some(((creation.high as u64) << 32) | creation.low as u64)
        }
    }

    pub fn exe_path(pid: u32) -> Option<PathBuf> {
        unsafe {
            let handle = open(pid, PROCESS_QUERY_LIMITED_INFORMATION);
            if handle.is_null() {
                return None;
            }
            let mut buf = vec![0u16; 32_768];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(handle);
            if ok == 0 {
                return None;
            }
            buf.truncate(len as usize);
            Some(PathBuf::from(String::from_utf16_lossy(&buf)))
        }
    }

    pub fn terminate(pid: u32) -> std::io::Result<()> {
        unsafe {
            let handle = open(pid, PROCESS_TERMINATE);
            if handle.is_null() {
                return Ok(());
            }
            let ok = TerminateProcess(handle, 1);
            CloseHandle(handle);
            if ok == 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }

    pub fn acquire_lock(data_dir: &Path, purpose: &str) -> std::io::Result<InstanceLock> {
        let canonical = std::fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
        let digest = hex::encode(Sha256::digest(canonical.to_string_lossy().as_bytes()));
        let name = format!("Local\\Veil-{}-{purpose}", &digest[..24]);
        let wide: Vec<u16> = std::ffi::OsStr::new(&name)
            .encode_wide()
            .chain(Some(0))
            .collect();
        unsafe {
            let handle = CreateMutexW(std::ptr::null_mut(), 0, wide.as_ptr());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "another Veil instance owns this data directory",
                ));
            }
            Ok(InstanceLock { handle })
        }
    }

    pub fn release_lock(handle: *mut c_void) {
        if !handle.is_null() {
            unsafe {
                CloseHandle(handle);
            }
        }
    }

    pub fn port_owner_pid(port: u16) -> Option<u32> {
        unsafe {
            let mut size = 0u32;
            let first = GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            );
            if first != ERROR_INSUFFICIENT_BUFFER || size < 4 {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            if GetExtendedTcpTable(
                buf.as_mut_ptr().cast(),
                &mut size,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            ) != 0
            {
                return None;
            }
            let count = *(buf.as_ptr() as *const u32) as usize;
            let rows = buf.as_ptr().add(4) as *const TcpRowOwnerPid;
            for index in 0..count {
                let row = &*rows.add(index);
                if u16::from_be(row.local_port as u16) == port {
                    return Some(row.owning_pid);
                }
            }
            None
        }
    }

    pub fn atomic_replace(from: &Path, to: &Path) -> std::io::Result<()> {
        let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        let ok = unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ok == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(unix)]
mod unix {
    use super::{InstanceLock, Path, PathBuf};
    use std::os::fd::AsRawFd;

    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
        fn flock(fd: i32, operation: i32) -> i32;
    }

    pub fn is_alive(pid: u32) -> bool {
        let rc = unsafe { kill(pid as i32, 0) };
        rc == 0 || std::io::Error::last_os_error().kind() == std::io::ErrorKind::PermissionDenied
    }

    pub fn started_at(pid: u32) -> Option<u64> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let after_name = stat.rsplit_once(')')?.1.trim();
        after_name.split_whitespace().nth(19)?.parse().ok()
    }

    pub fn exe_path(pid: u32) -> Option<PathBuf> {
        std::fs::read_link(format!("/proc/{pid}/exe")).ok()
    }

    pub fn terminate(pid: u32) -> std::io::Result<()> {
        const SIGTERM: i32 = 15;
        let rc = unsafe { kill(pid as i32, SIGTERM) };
        if rc == 0 {
            Ok(())
        } else {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(err)
            }
        }
    }

    pub fn acquire_lock(data_dir: &Path, purpose: &str) -> std::io::Result<InstanceLock> {
        const LOCK_EX: i32 = 2;
        const LOCK_NB: i32 = 4;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(data_dir.join(format!("veil.{purpose}.lock")))?;
        if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "another Veil instance owns this data directory",
            ));
        }
        Ok(InstanceLock { file })
    }

    pub fn release_lock(file: &std::fs::File) {
        const LOCK_UN: i32 = 8;
        unsafe {
            flock(file.as_raw_fd(), LOCK_UN);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        acquire_instance_lock, current_exe, current_process_started_at, now_rfc3339, read_runtime,
        write_runtime, RuntimeMetadata,
    };

    fn metadata() -> RuntimeMetadata {
        RuntimeMetadata {
            pid: std::process::id(),
            process_started_at: current_process_started_at(),
            instance_id: "01TEST".into(),
            exe_path: current_exe(),
            management_port: 18788,
            listener_ports: vec![18788, 18787],
            proxy_state: "running".into(),
            started_at: now_rfc3339(),
            last_lifecycle_error: None,
        }
    }

    #[test]
    fn runtime_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let want = metadata();
        write_runtime(dir.path(), &want).unwrap();
        assert_eq!(read_runtime(dir.path()).unwrap(), Some(want));
    }

    #[test]
    fn only_one_instance_lock_is_granted() {
        let dir = tempfile::tempdir().unwrap();
        let _first = acquire_instance_lock(dir.path()).unwrap();
        let second = acquire_instance_lock(dir.path()).unwrap_err();
        assert_eq!(second.kind(), std::io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn timestamp_is_utc_rfc3339_shape() {
        let value = now_rfc3339();
        assert_eq!(value.len(), 20);
        assert!(value.ends_with('Z'));
    }
}

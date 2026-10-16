use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

pub enum LockError {
    AlreadyRunning(u32),
    Io(std::io::Error),
}

/// Lock de instancia unica via `flock(LOCK_EX | LOCK_NB)`. Liberado ao dropar.
pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    pub fn acquire(path: &Path) -> Result<Self, LockError> {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(LockError::Io)?;
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            let mut content = String::new();
            let _ = file.read_to_string(&mut content);
            let pid = content.trim().parse::<u32>().unwrap_or(0);
            return Err(LockError::AlreadyRunning(pid));
        }
        let _ = file.set_len(0);
        let _ = file.write_all(std::process::id().to_string().as_bytes());
        let _ = file.flush();
        Ok(Self { _file: file })
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // Um subprocesso pode herdar o descritor antes de exec; fechar apenas
        // este File manteria o flock ativo ate a ultima referencia ser fechada.
        unsafe { libc::flock(self._file.as_raw_fd(), libc::LOCK_UN) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_lock_fails_while_first_held() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("runtime.lock");
        let first = InstanceLock::acquire(&p).ok().expect("primeiro lock");
        match InstanceLock::acquire(&p) {
            Err(LockError::AlreadyRunning(pid)) => assert_eq!(pid, std::process::id()),
            _ => panic!("segundo lock deveria falhar"),
        }
        drop(first);
        assert!(InstanceLock::acquire(&p).is_ok());
    }

    #[test]
    fn dropping_lock_releases_it_with_a_duplicated_descriptor() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("runtime.lock");
        let first = InstanceLock::acquire(&p).ok().expect("primeiro lock");
        let duplicate = first._file.try_clone().unwrap();

        drop(first);
        assert!(InstanceLock::acquire(&p).is_ok());
        drop(duplicate);
    }
}

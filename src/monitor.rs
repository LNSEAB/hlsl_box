use crate::*;
use std::{
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    sync::{
        atomic::{self, AtomicBool},
        mpsc,
    },
};
use windows::Win32::{Foundation::*, Storage::FileSystem::*, System::IO::*};

pub struct DirMonitor {
    th: Option<std::thread::JoinHandle<()>>,
    exit: Arc<AtomicBool>,
    rx: mpsc::Receiver<PathBuf>,
}

impl DirMonitor {
    pub fn new(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let exit_flag = Arc::new(AtomicBool::new(false));
        let exit = exit_flag.clone();
        let path = path.as_ref().to_path_buf();
        let th = std::thread::spawn(move || unsafe {
            if let Err(e) = Self::read_directory(path, exit_flag, tx) {
                error!("read_directory: {}", e);
            }
        });
        Ok(Self {
            th: Some(th),
            exit,
            rx,
        })
    }

    pub fn try_recv(&self) -> Option<PathBuf> {
        self.rx.try_recv().ok()
    }

    unsafe fn read_directory(
        dir_path: PathBuf,
        exit: Arc<AtomicBool>,
        tx: mpsc::Sender<PathBuf>,
    ) -> anyhow::Result<()> {
        assert!(dir_path.is_dir());
        let path = dir_path.to_string_lossy().to_string();
        let dir = CreateFileW(
            path.as_str(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            HANDLE::default(),
        )?;
        let mut buffer = vec![0u8; 2048];
        loop {
            if exit.load(atomic::Ordering::SeqCst) {
                break;
            }
            let mut len = 0;
            let ret = ReadDirectoryChangesW(
                dir,
                buffer.as_mut_ptr() as _,
                buffer.len() as _,
                false,
                FILE_NOTIFY_CHANGE_LAST_WRITE,
                &mut len,
                std::ptr::null_mut(),
                None,
            );
            if !ret.as_bool() {
                break;
            }
            let mut p = buffer.as_ptr() as *const u8;
            loop {
                let data = (p as *const FILE_NOTIFY_INFORMATION).as_ref().unwrap();
                if data.Action == FILE_ACTION_MODIFIED {
                    let file_name = std::slice::from_raw_parts(
                        data.FileName.as_ptr() as *const u16,
                        data.FileNameLength as usize / std::mem::size_of::<u16>(),
                    );
                    let file_name = PathBuf::from(String::from_utf16_lossy(file_name));
                    tx.send(dir_path.join(file_name)).ok();
                }
                if data.NextEntryOffset == 0 {
                    break;
                }
                p = p.offset(data.NextEntryOffset as _);
            }
        }
        CloseHandle(dir);
        Ok(())
    }
}

impl Drop for DirMonitor {
    fn drop(&mut self) {
        if let Some(th) = self.th.take() {
            self.exit.store(true, atomic::Ordering::SeqCst);
            unsafe {
                CancelSynchronousIo(HANDLE(th.as_raw_handle() as _));
            }
            th.join().ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_monitor() {
        let dir_path = Path::new("./target/dir_monitor_test");
        let file_path = dir_path.join("test.txt");
        if let Err(e) = std::fs::create_dir(dir_path) {
            match e.kind() {
                std::io::ErrorKind::AlreadyExists => {}
                _ => Err(e).unwrap(),
            }
        }
        let dm = DirMonitor::new(&dir_path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        {
            use std::io::Write;
            let file = std::fs::File::create(&file_path).unwrap();
            let mut writer = std::io::BufWriter::new(file);
            writer.write("a".as_bytes()).unwrap();
        }
        assert!(
            dm.rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap()
                == file_path
        );
    }
}

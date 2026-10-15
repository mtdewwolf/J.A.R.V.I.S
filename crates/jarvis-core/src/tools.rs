//! Fail-closed descriptor-relative reads; no path check followed by reopening.
use serde::{Deserialize, Serialize};
use std::{
    ffi::CString,
    fs::File,
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    },
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadArgs {
    pub path: String,
}

pub fn validate(args: &ReadArgs) -> Result<(), String> {
    if args.path.is_empty()
        || args.path.len() > 1024
        || args.path.contains('\\')
        || args.path.contains('\0')
    {
        return Err("invalid relative path".into());
    }
    for component in args.path.split('/') {
        if component.is_empty()
            || component.starts_with('.')
            || matches!(component, ".." | "credentials" | "keys")
        {
            return Err("hidden, absolute, traversal or credential paths are excluded".into());
        }
        let lower = component.to_ascii_lowercase();
        if lower.ends_with(".pem")
            || lower.ends_with(".key")
            || lower == "id_rsa"
            || lower == "id_ed25519"
        {
            return Err("key files are excluded".into());
        }
    }
    Ok(())
}

pub fn read(root: &Path, args: &ReadArgs) -> Result<String, String> {
    validate(args)?;
    let dir = File::open(root).map_err(|_| "export directory unavailable")?;
    if !dir
        .metadata()
        .map_err(|_| "export directory unavailable")?
        .is_dir()
    {
        return Err("export root is not a directory".into());
    }
    let path = CString::new(args.path.as_bytes()).map_err(|_| "invalid path")?;
    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    // RESOLVE_NO_XDEV | NO_MAGICLINKS | NO_SYMLINKS | BENEATH.
    let how = OpenHow {
        flags: (libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK) as u64,
        mode: 0,
        resolve: 0x01 | 0x02 | 0x04 | 0x08,
    };
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            dir.as_raw_fd(),
            path.as_ptr(),
            &how,
            std::mem::size_of::<OpenHow>(),
        )
    };
    if fd < 0 {
        return Err("safe open denied or openat2 unavailable".into());
    }
    let file = unsafe { File::from_raw_fd(fd as i32) };
    let meta = file.metadata().map_err(|_| "file metadata unavailable")?;
    if !meta.is_file()
        || meta.nlink() != 1
        || meta.len() > crate::limits::MAX_TOOL_OUTPUT_BYTES as u64
    {
        return Err("only single-link regular files up to 64 KiB are allowed".into());
    }
    let mut bytes = Vec::new();
    file.take((crate::limits::MAX_TOOL_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "read failed")?;
    if bytes.len() > crate::limits::MAX_TOOL_OUTPUT_BYTES {
        return Err("file exceeded 64 KiB during read".into());
    }
    String::from_utf8(bytes).map_err(|_| "file is not UTF-8".into())
}

pub fn validate_proposal(
    name: &str,
    args: &serde_json::Value,
    registry: &crate::connections::Registry,
) -> Result<(), String> {
    match name {
        "workspace.read_text" => validate(
            &serde_json::from_value::<ReadArgs>(args.clone())
                .map_err(|_| "invalid read arguments")?,
        ),
        "connection.call" => registry.validate(args).map(|_| ()),
        _ => Err("tool is not registered".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traversal_links_special_files_and_limits() {
        let root = std::env::temp_dir().join(crate::ids::new_id("jarvis-tools"));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("ok.txt"), "hello").unwrap();
        assert_eq!(
            read(
                &root,
                &ReadArgs {
                    path: "ok.txt".into()
                }
            )
            .unwrap(),
            "hello"
        );
        for path in [
            "../ok.txt",
            "/ok.txt",
            ".env",
            "x/../ok.txt",
            "x//ok.txt",
            "id_rsa",
            "x.key",
        ] {
            assert!(
                read(&root, &ReadArgs { path: path.into() }).is_err(),
                "{path}"
            );
        }
        std::os::unix::fs::symlink("ok.txt", root.join("link.txt")).unwrap();
        assert!(read(
            &root,
            &ReadArgs {
                path: "link.txt".into()
            }
        )
        .is_err());
        std::fs::hard_link(root.join("ok.txt"), root.join("hard.txt")).unwrap();
        assert!(read(
            &root,
            &ReadArgs {
                path: "hard.txt".into()
            }
        )
        .is_err());
        std::fs::write(root.join("large.txt"), vec![0; 65537]).unwrap();
        assert!(read(
            &root,
            &ReadArgs {
                path: "large.txt".into()
            }
        )
        .is_err());
        std::fs::write(root.join("binary.txt"), [255]).unwrap();
        assert!(read(
            &root,
            &ReadArgs {
                path: "binary.txt".into()
            }
        )
        .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}

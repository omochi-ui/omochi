//! SDK-compatible local manifest discovery. Discovery does not grant remote trust.
use anyhow::{Result, bail};
use omochi_protocol::session::valid_service_id;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

const MAX_MANIFEST_BYTES: u64 = 128 * 1024;

/// Wake a scanner on changes, never treating a filesystem event as trusted data.
/// Keep periodic scans: notifications can be unavailable or lost on some filesystems.
pub fn watch(
    root: &Path,
    changed: impl Fn() + Send + 'static,
) -> Result<notify::RecommendedWatcher> {
    use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
    scan(root)?;
    let mut watcher = RecommendedWatcher::new(
        move |event: notify::Result<Event>| {
            // Our own manifest reads must not cause a scan/notification loop.
            if !matches!(
                event,
                Ok(Event {
                    kind: EventKind::Access(_),
                    ..
                })
            ) {
                changed();
            }
        },
        Config::default().with_follow_symlinks(false),
    )?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok(watcher)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub socket: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}
impl Manifest {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("missing service name")
        }
        if self.socket.is_empty()
            || self.socket == "."
            || self.socket == ".."
            || self.socket.contains(['/', '\\', '\0'])
        {
            bail!("socket must be a plain filename")
        }
        if let Some(icon) = &self.icon {
            // Match JS string.length (UTF-16 code units), not UTF-8 bytes.
            if icon.encode_utf16().count() > 64 * 1024 {
                bail!("icon too large")
            }
            if icon.starts_with("data:") {
                let Some((kind, data)) = icon.split_once(";base64,") else {
                    bail!("invalid data icon")
                };
                if ![
                    "data:image/png",
                    "data:image/svg+xml",
                    "data:image/webp",
                    "data:image/jpeg",
                    "data:image/gif",
                ]
                .contains(&kind)
                    || data.is_empty()
                    || !data
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b))
                {
                    bail!("unsupported icon")
                }
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredService {
    pub id: String,
    pub manifest: Manifest,
    pub socket_path: PathBuf,
}

/// Caller runs scans off the async executor, both after filesystem notifications and
/// periodically. Half-written/invalid manifests disappear until the next valid scan.
pub fn scan(root: &Path) -> Result<BTreeMap<String, DiscoveredService>> {
    let metadata = fs::symlink_metadata(root)?;
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    // Reaching the socket is local authentication: never traverse an untrusted root.
    if !metadata.is_dir() || metadata.mode() & 0o077 != 0 || metadata.uid() != unsafe { getuid() } {
        bail!("runtime root must be an owned private directory")
    }
    let mut found = BTreeMap::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !valid_service_id(&id) {
            continue;
        }
        let directory = entry.path();
        let read = || -> Result<Manifest> {
            let metadata = fs::symlink_metadata(directory.join("manifest.json"))?;
            if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
                bail!("manifest must be a bounded regular file")
            }
            let mut bytes = Vec::new();
            fs::File::open(directory.join("manifest.json"))?
                .take(MAX_MANIFEST_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_MANIFEST_BYTES {
                bail!("manifest too large")
            }
            let manifest: Manifest = serde_json::from_slice(&bytes)?;
            manifest.validate()?;
            Ok(manifest)
        };
        if let Ok(manifest) = read() {
            found.insert(
                id.clone(),
                DiscoveredService {
                    id,
                    socket_path: directory.join(&manifest.socket),
                    manifest,
                },
            );
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    fn filesystem_changes_wake_validated_scans() {
        let tmp = tempfile::tempdir().unwrap();
        fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(tmp.path().join("app")).unwrap();
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let _watcher = watch(tmp.path(), move || {
            let _ = send.try_send(());
        })
        .unwrap();
        fs::write(
            tmp.path().join("app/staging.json"),
            r#"{"name":"App","socket":"a2ui.sock"}"#,
        )
        .unwrap();
        fs::rename(
            tmp.path().join("app/staging.json"),
            tmp.path().join("app/manifest.json"),
        )
        .unwrap();
        receive
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(scan(tmp.path()).unwrap()["app"].manifest.name, "App");
        fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(watch(tmp.path(), || {}).is_err());
    }

    #[test]
    fn sdk_manifests_are_discovered_but_escaping_paths_are_not() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).unwrap();
        for (id, socket) in [("python-app", "a2ui.sock"), ("bad", "../gateway.sock")] {
            fs::create_dir(root.join(id)).unwrap();
            fs::write(
                root.join(id).join("manifest.json"),
                serde_json::json!({"name": id, "socket": socket, "icon": "🍡"}).to_string(),
            )
            .unwrap();
        }
        symlink(root.join("python-app"), root.join("alias")).unwrap();
        let services = scan(root).unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(
            services["python-app"].socket_path,
            root.join("python-app/a2ui.sock")
        );
        fs::write(root.join("python-app/manifest.json"), "{").unwrap();
        assert!(scan(root).unwrap().is_empty());
    }
    #[test]
    fn rejects_public_runtime_root_and_active_data_icons() {
        let tmp = tempfile::tempdir().unwrap();
        fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(scan(tmp.path()).is_err());
        let manifest = Manifest {
            name: "app".into(),
            socket: "a2ui.sock".into(),
            icon: Some("data:text/html;base64,YQ==".into()),
        };
        assert!(manifest.validate().is_err());
    }
}

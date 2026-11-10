use crate::{
    LocalEndpoint, LocalListener, LocalStream,
    files::{self, FileLock},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::watch;

pub fn validate_service_id(id: &str) -> io::Result<()> {
    if id.is_empty()
        || id.len() > 64
        || id == "."
        || id == ".."
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
    {
        return Err(crate::invalid(
            "service ID must match [a-z0-9._-]{1,64} and cannot be . or ..",
        ));
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: u32,
    pub service_id: String,
    pub name: String,
    pub endpoint: LocalEndpoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}
impl Manifest {
    pub fn validate(&self) -> io::Result<()> {
        validate_service_id(&self.service_id)?;
        if self.version != 2 || self.name.trim().is_empty() {
            return Err(crate::invalid("invalid manifest version or name"));
        }
        if let Some(icon) = &self.icon {
            if icon.encode_utf16().count() > 64 * 1024 {
                return Err(crate::invalid("icon too large"));
            }
            if icon.starts_with("data:") {
                let (kind, data) = icon
                    .split_once(";base64,")
                    .ok_or_else(|| crate::invalid("invalid icon"))?;
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
                    return Err(crate::invalid("invalid data icon"));
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
    pub endpoint: LocalEndpoint,
}
#[derive(Debug, Clone)]
pub struct LocalRuntime {
    root: PathBuf,
}
impl LocalRuntime {
    pub fn prepare(root: Option<PathBuf>) -> io::Result<Self> {
        let root = root
            .map(Ok)
            .unwrap_or_else(crate::platform::prepare_default_runtime)?;
        files::private_directory(&root)?;
        let _lock = FileLock::acquire(&root.join(".runtime.lock~"), false)?;
        let marker = root.join("runtime.json");
        match files::read_bounded(&marker, 1024) {
            Ok(bytes)
                if serde_json::from_slice::<serde_json::Value>(&bytes)?
                    == serde_json::json!({"version":2}) => {}
            Ok(_) => return Err(crate::invalid("unsupported runtime format")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if fs::read_dir(&root)?.any(|e| e.is_ok_and(|e| e.file_name() != ".runtime.lock~"))
                {
                    return Err(crate::invalid(
                        "nonempty legacy runtime; stop all publishers and choose a new directory",
                    ));
                }
                files::atomic_write(&marker, b"{\"version\":2}\n")?;
            }
            Err(e) => return Err(e),
        }
        Ok(Self { root })
    }
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        files::validate_directory(&root)?;
        if serde_json::from_slice::<serde_json::Value>(&files::read_bounded(
            &root.join("runtime.json"),
            1024,
        )?)? != serde_json::json!({"version":2})
        {
            return Err(crate::invalid("unsupported runtime format"));
        }
        Ok(Self { root })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn service_directory(&self, id: &str) -> io::Result<PathBuf> {
        validate_service_id(id)?;
        Ok(self.root.join(format!("svc-{id}~")))
    }
    pub fn scan(&self) -> io::Result<BTreeMap<String, DiscoveredService>> {
        files::validate_directory(&self.root)?;
        let mut result = BTreeMap::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(id) = name.strip_prefix("svc-").and_then(|s| s.strip_suffix('~')) else {
                continue;
            };
            let read = || -> io::Result<DiscoveredService> {
                validate_service_id(id)?;
                files::validate_directory(&entry.path())?;
                let manifest: Manifest = serde_json::from_slice(&files::read_bounded(
                    &entry.path().join("manifest.json"),
                    128 * 1024,
                )?)?;
                manifest.validate()?;
                if manifest.service_id != id {
                    return Err(crate::invalid("manifest ID differs from directory"));
                }
                let endpoint = manifest.endpoint.resolve(&entry.path())?;
                Ok(DiscoveredService {
                    id: id.into(),
                    manifest,
                    endpoint,
                })
            };
            if let Ok(service) = read() {
                result.insert(id.into(), service);
            }
        }
        Ok(result)
    }
    pub async fn publish(
        &self,
        id: &str,
        name: &str,
        icon: Option<String>,
    ) -> io::Result<ServicePublication> {
        let directory = self.service_directory(id)?;
        let lock = FileLock::acquire(&self.root.join(format!(".svc-{id}.lock~")), true)?;
        files::private_directory(&directory)?;
        ServicePublication::bind(directory, "manifest.json", Some((id, name, icon)), lock).await
    }
    pub async fn publish_gateway(&self) -> io::Result<ServicePublication> {
        let lock = FileLock::acquire(&self.root.join(".gateway.lock~"), true)?;
        ServicePublication::bind(self.root.clone(), "gateway.json", None, lock).await
    }
    pub fn gateway_endpoint(&self) -> io::Result<LocalEndpoint> {
        #[derive(Deserialize)]
        struct Gateway {
            version: u32,
            endpoint: LocalEndpoint,
        }
        let record: Gateway =
            serde_json::from_slice(&files::read_bounded(&self.root.join("gateway.json"), 4096)?)?;
        if record.version != 2 {
            return Err(crate::invalid("unsupported gateway record"));
        }
        record.endpoint.resolve(&self.root)
    }
}
struct OwnedPublication {
    endpoint: LocalEndpoint,
    socket_identity: Option<(u64, u64)>,
    manifest: PathBuf,
    manifest_identity: Option<(u64, u64)>,
    directory: Option<PathBuf>,
    lock: Option<FileLock>,
    finished: watch::Sender<bool>,
}
impl Drop for OwnedPublication {
    fn drop(&mut self) {
        if files::file_identity(&self.manifest).ok() == self.manifest_identity
            && self.manifest_identity.is_some()
        {
            let _ = fs::remove_file(&self.manifest);
        }
        if let LocalEndpoint::Unix { path } = &self.endpoint {
            if self.socket_identity.is_some()
                && files::file_identity(path).ok() == self.socket_identity
            {
                let _ = fs::remove_file(path);
            }
        }
        if let Some(directory) = &self.directory {
            let _ = fs::remove_dir(directory);
        }
        self.lock.take();
        self.finished.send_replace(true);
    }
}
pub struct ServicePublication {
    listener: LocalListener,
    owned: Arc<OwnedPublication>,
    finished: watch::Receiver<bool>,
}
impl ServicePublication {
    async fn bind(
        directory: PathBuf,
        filename: &str,
        metadata: Option<(&str, &str, Option<String>)>,
        lock: FileLock,
    ) -> io::Result<Self> {
        let socket_path = directory.join(if metadata.is_some() {
            "a2ui.sock"
        } else {
            "gateway.sock"
        });
        #[cfg(unix)]
        {
            match fs::symlink_metadata(&socket_path) {
                Ok(m) => {
                    use std::os::unix::fs::FileTypeExt;
                    if !m.file_type().is_socket() {
                        return Err(crate::invalid("endpoint path is not a socket"));
                    }
                    match tokio::net::UnixStream::connect(&socket_path).await {
                        Ok(_) => {
                            return Err(io::Error::new(
                                io::ErrorKind::AddrInUse,
                                "existing live publisher",
                            ));
                        }
                        Err(e)
                            if matches!(
                                e.kind(),
                                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                            ) =>
                        {
                            fs::remove_file(&socket_path)?
                        }
                        Err(e) => return Err(e),
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        let endpoint = LocalEndpoint::for_socket(socket_path)?;
        let (finished, receiver) = watch::channel(false);
        let mut owned = OwnedPublication {
            endpoint: endpoint.clone(),
            socket_identity: None,
            manifest: directory.join(filename),
            manifest_identity: None,
            directory: metadata.as_ref().map(|_| directory.clone()),
            lock: Some(lock),
            finished,
        };
        let mut listener = LocalListener::bind(endpoint.clone())?;
        if let LocalEndpoint::Unix { path } = &endpoint {
            owned.socket_identity = Some(files::file_identity(path)?);
        }
        let advertised = match &endpoint {
            LocalEndpoint::Unix { path } => LocalEndpoint::Unix {
                path: path.file_name().unwrap().into(),
            },
            other => other.clone(),
        };
        let bytes = if let Some((id, name, icon)) = metadata {
            let manifest = Manifest {
                version: 2,
                service_id: id.into(),
                name: name.into(),
                icon,
                endpoint: advertised,
            };
            manifest.validate()?;
            serde_json::to_vec(&manifest)?
        } else {
            serde_json::to_vec(&serde_json::json!({"version":2,"endpoint":advertised}))?
        };
        if bytes.len() > 128 * 1024 {
            return Err(crate::invalid("manifest exceeds 128 KiB"));
        }
        files::atomic_write(&owned.manifest, &bytes)?;
        owned.manifest_identity = Some(files::file_identity(&owned.manifest)?);
        let owned = Arc::new(owned);
        listener.lifetime = Some(owned.clone());
        Ok(Self {
            listener,
            owned,
            finished: receiver,
        })
    }
    pub fn endpoint(&self) -> &LocalEndpoint {
        self.listener.endpoint()
    }
    pub fn directory(&self) -> &Path {
        self.owned.manifest.parent().unwrap()
    }
    pub fn finished(&self) -> watch::Receiver<bool> {
        self.finished.clone()
    }
    pub async fn accept(&self) -> io::Result<LocalStream> {
        self.listener.accept().await
    }
    pub async fn stop(&self) {
        if files::file_identity(&self.owned.manifest).ok() == self.owned.manifest_identity {
            let _ = fs::remove_file(&self.owned.manifest);
        }
        self.listener.close().await;
    }
}

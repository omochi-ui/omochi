//! Browser-compatible dashboard persistence in the new Rust state directory.
use fs2::FileExt;
use omochi_protocol::session::{valid_node_id, valid_service_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct ApiError(pub u16, pub String);
impl ApiError {
    pub fn new(code: u16, message: &str) -> Self {
        Self(code, message.into())
    }
}
impl From<std::io::Error> for ApiError {
    fn from(_: std::io::Error) -> Self {
        Self::new(503, "Could not access dashboard storage")
    }
}
type Result<T> = std::result::Result<T, ApiError>;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub node_id: String,
    pub service_id: String,
    pub surface_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Panel {
    pub id: String,
    pub title: String,
    pub reference: Reference,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    pub name: String,
    pub panels: Vec<Panel>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dashboard {
    pub id: String,
    pub revision: u64,
    #[serde(flatten)]
    pub input: Input,
}
#[derive(Serialize, Deserialize)]
struct Storage {
    version: u32,
    dashboards: Vec<Dashboard>,
}
#[derive(Clone)]
pub struct DashboardStore {
    root: PathBuf,
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.encode_utf16().count() <= max
}
pub fn validate(value: Value) -> Result<Input> {
    let mut input: Input =
        serde_json::from_value(value).map_err(|_| ApiError::new(400, "Invalid dashboard"))?;
    if !text(&input.name, 200) || input.panels.len() > 200 {
        return Err(ApiError::new(
            400,
            "A dashboard needs a name and at most 200 panels",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut references = BTreeSet::new();
    for (index, panel) in input.panels.iter().enumerate() {
        if !text(&panel.id, 100)
            || !text(&panel.title, 200)
            || !valid_node_id(&panel.reference.node_id)
            || !valid_service_id(&panel.reference.service_id)
            || !text(&panel.reference.surface_id, 4096)
            || panel.x > 11
            || panel.y > 100000
            || panel.w == 0
            || panel.w > 12
            || panel.h == 0
            || panel.h > 1000
            || panel.x + panel.w > 12
        {
            return Err(ApiError::new(
                400,
                "Invalid panel reference or grid position",
            ));
        }
        let reference = (
            &panel.reference.node_id,
            &panel.reference.service_id,
            &panel.reference.surface_id,
        );
        if !ids.insert(&panel.id) || !references.insert(reference) {
            return Err(ApiError::new(400, "Duplicate panel or surface"));
        }
        if input.panels[..index].iter().any(|other| {
            panel.x < other.x + other.w
                && other.x < panel.x + panel.w
                && panel.y < other.y + other.h
                && other.y < panel.y + panel.h
        }) {
            return Err(ApiError::new(400, "Panels must not overlap"));
        }
    }
    input.name = input.name.trim().into();
    Ok(input)
}
impl DashboardStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_owned();
        if !root.exists() {
            fs::create_dir_all(&root)?;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        let meta = fs::symlink_metadata(&root)?;
        unsafe extern "C" {
            fn getuid() -> u32;
        }
        if !meta.is_dir() || meta.mode() & 0o077 != 0 || meta.uid() != unsafe { getuid() } {
            return Err(ApiError::new(
                503,
                "Dashboard directory must be owned and private",
            ));
        }
        Ok(Self { root })
    }
    fn read(&self) -> Result<Storage> {
        let bytes = match fs::read(self.root.join("dashboards.json")) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Storage {
                    version: 1,
                    dashboards: vec![],
                });
            }
            Err(e) => return Err(e.into()),
        };
        let corrupt = || {
            ApiError::new(
                503,
                "Dashboard storage is invalid; repair dashboards.json before saving",
            )
        };
        let storage: Storage = serde_json::from_slice(&bytes).map_err(|_| corrupt())?;
        if storage.version != 1 {
            return Err(corrupt());
        }
        let mut ids = BTreeSet::new();
        for dashboard in &storage.dashboards {
            if !text(&dashboard.id, 100)
                || dashboard.revision == 0
                || dashboard.revision > 9_007_199_254_740_991
                || !ids.insert(&dashboard.id)
            {
                return Err(corrupt());
            }
            validate(serde_json::to_value(&dashboard.input).map_err(|_| corrupt())?)
                .map_err(|_| corrupt())?;
        }
        Ok(storage)
    }
    pub fn list(&self) -> Result<Vec<Dashboard>> {
        Ok(self.read()?.dashboards)
    }
    pub fn mutate(
        &self,
        id: Option<&str>,
        value: Value,
        remove: bool,
    ) -> Result<Option<Dashboard>> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(self.root.join("dashboards.lock"))?;
        lock.lock_exclusive()?;
        let mut storage = self.read()?;
        let current = id.and_then(|id| storage.dashboards.iter().find(|d| d.id == id));
        if id.is_some() && current.is_none() {
            return Err(ApiError::new(404, "Dashboard not found"));
        }
        if let Some(current) = current
            && value.get("revision").and_then(Value::as_u64) != Some(current.revision)
        {
            return Err(ApiError::new(
                409,
                "This dashboard changed elsewhere. Reload before saving.",
            ));
        }
        let result = if remove {
            None
        } else {
            let revision = current.map_or(1, |d| d.revision + 1);
            if revision > 9_007_199_254_740_991 {
                return Err(ApiError::new(409, "Dashboard revision exhausted"));
            }
            Some(Dashboard {
                id: id
                    .map(str::to_owned)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                revision,
                input: validate(value)?,
            })
        };
        storage.dashboards.retain(|d| Some(d.id.as_str()) != id);
        if let Some(dashboard) = &result {
            storage.dashboards.push(dashboard.clone());
        }
        let temporary = self
            .root
            .join(format!(".dashboards-{}.tmp", uuid::Uuid::new_v4()));
        let save = (|| -> Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(
                &serde_json::to_vec(&storage)
                    .map_err(|_| ApiError::new(503, "Could not encode dashboard"))?,
            )?;
            file.sync_all()?;
            fs::rename(&temporary, self.root.join("dashboards.json"))?;
            File::open(&self.root)?.sync_all()?;
            Ok(())
        })();
        if save.is_err() {
            let _ = fs::remove_file(temporary);
        }
        save?;
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn stale_writes_and_corrupt_storage_do_not_overwrite_saved_dashboards() {
        let root = tempfile::tempdir().unwrap();
        let store = DashboardStore::open(root.path().join("state")).unwrap();
        let saved = store
            .mutate(None, json!({"name":" Home ","panels":[]}), false)
            .unwrap()
            .unwrap();
        assert_eq!(saved.input.name, "Home");
        assert_eq!(
            store
                .mutate(
                    Some(&saved.id),
                    json!({"revision":0,"name":"stale","panels":[]}),
                    false
                )
                .unwrap_err()
                .0,
            409
        );
        store
            .mutate(
                Some(&saved.id),
                json!({"revision":1,"name":"new","panels":[]}),
                false,
            )
            .unwrap();
        assert_eq!(store.list().unwrap()[0].revision, 2);
        fs::write(store.root.join("dashboards.json"), "corrupt").unwrap();
        assert_eq!(
            store
                .mutate(None, json!({"name":"overwrite","panels":[]}), false)
                .unwrap_err()
                .0,
            503
        );
        assert_eq!(
            fs::read_to_string(store.root.join("dashboards.json")).unwrap(),
            "corrupt"
        );
    }
}

//! Immutable media registry and the transport-independent resources/1 responder.
use crate::{Error, Result};
use serde_json::{Value, json};
use std::os::unix::fs::OpenOptionsExt;
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
pub const CHUNK: u64 = 24 * 1024;
pub fn is_resource(v: &Value) -> bool {
    v["omochi"] == "resources/1"
}
pub fn hello() -> Value {
    json!({"omochi":"resources/1","kind":"hello"})
}
struct Entry {
    path: PathBuf,
    size: u64,
    mime: String,
}
struct Store {
    directory: Option<PathBuf>,
    entries: HashMap<String, Entry>,
    total: u64,
    closed: bool,
}
impl Drop for Store {
    fn drop(&mut self) {
        if let Some(path) = &self.directory {
            let _ = fs::remove_dir_all(path);
        }
    }
}
#[derive(Clone)]
pub struct ResourceRegistry {
    store: Arc<Mutex<Store>>,
    max_file: u64,
    max_total: u64,
}
impl Default for ResourceRegistry {
    fn default() -> Self {
        Self::new(1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024)
    }
}
fn random_id() -> Result<String> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn mime_for(path: &Path) -> Option<&'static str> {
    Some(
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "avif" => "image/avif",
            "mp3" => "audio/mpeg",
            "wav" => "audio/wav",
            "ogg" => "audio/ogg",
            "m4a" => "audio/mp4",
            "mp4" => "video/mp4",
            "webm" => "video/webm",
            "mov" => "video/quicktime",
            _ => return None,
        },
    )
}
fn media_mime(mime: &str) -> bool {
    let Some((kind, sub)) = mime.split_once('/') else {
        return false;
    };
    matches!(kind, "image" | "audio" | "video")
        && !sub.is_empty()
        && sub
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b))
}
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let a = c[0] as usize;
        let b = *c.get(1).unwrap_or(&0) as usize;
        let d = *c.get(2).unwrap_or(&0) as usize;
        out.push(ALPHABET[a >> 2] as char);
        out.push(ALPHABET[((a & 3) << 4) | (b >> 4)] as char);
        out.push(if c.len() > 1 {
            ALPHABET[((b & 15) << 2) | (d >> 6)] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            ALPHABET[d & 63] as char
        } else {
            '='
        });
    }
    out
}
impl ResourceRegistry {
    pub fn new(max_file: u64, max_total: u64) -> Self {
        Self {
            store: Arc::new(Mutex::new(Store {
                directory: None,
                entries: HashMap::new(),
                total: 0,
                closed: false,
            })),
            max_file,
            max_total,
        }
    }
    pub fn file(&self, path: impl AsRef<Path>, mime: Option<&str>) -> Result<String> {
        let path = path.as_ref();
        self.register(
            File::open(path)?,
            mime.or_else(|| mime_for(path)).unwrap_or(""),
        )
    }
    pub fn bytes(&self, data: &[u8], mime: &str) -> Result<String> {
        self.register(data, mime)
    }
    fn register(&self, mut source: impl Read, mime: &str) -> Result<String> {
        let mut store = self.store.lock().unwrap();
        if store.closed || !media_mime(mime) {
            return Err(Error::Declaration(
                "closed registry or invalid media MIME type".into(),
            ));
        }
        if store.directory.is_none() {
            let dir = std::env::temp_dir().join(format!("omochi-resources-{}", random_id()?));
            let mut builder = fs::DirBuilder::new();
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700).create(&dir)?;
            store.directory = Some(dir);
        }
        let id = random_id()?;
        let path = store.directory.as_ref().unwrap().join(&id);
        let mut target = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let mut size = 0;
        let result = (|| -> Result<()> {
            let mut buffer = [0u8; 65536];
            loop {
                let n = source.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                size += n as u64;
                if size > self.max_file || store.total + size > self.max_total {
                    return Err(Error::Declaration("resource storage limit exceeded".into()));
                }
                target.write_all(&buffer[..n])?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            drop(target);
            let _ = fs::remove_file(path);
            return Err(error);
        }
        store.total += size;
        store.entries.insert(
            id.clone(),
            Entry {
                path,
                size,
                mime: mime.into(),
            },
        );
        Ok(format!("omochi://resources/{id}"))
    }
    pub fn release(&self, uri: &str) {
        let mut store = self.store.lock().unwrap();
        if let Some(id) = uri.strip_prefix("omochi://resources/") {
            if let Some(entry) = store.entries.remove(id) {
                store.total -= entry.size;
                let _ = fs::remove_file(entry.path);
            }
        }
    }
    pub fn close(&self) {
        let mut store = self.store.lock().unwrap();
        store.entries.clear();
        store.total = 0;
        store.closed = true;
        if let Some(dir) = store.directory.take() {
            let _ = fs::remove_dir_all(dir);
        }
    }
    pub fn respond(&self, request: &Value) -> Value {
        let mut result =
            json!({"omochi":"resources/1","kind":"response","id":request["id"],"status":200});
        let valid_id = request["id"].as_str().is_some_and(|s| {
            !s.is_empty()
                && s.len() <= 80
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        });
        let resource = request["resource"].as_str().unwrap_or("");
        let op = request["op"].as_str().unwrap_or("");
        let offset = request["offset"].as_u64();
        let length = request["length"].as_u64();
        if request["kind"] != "request"
            || !valid_id
            || resource.len() != 32
            || !resource
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !(op == "stat"
                || op == "read"
                    && offset.is_some_and(|v| v <= 9007199254740991)
                    && length.is_some_and(|v| v > 0 && v <= CHUNK))
        {
            result["status"] = json!(400);
            return result;
        }
        let store = self.store.lock().unwrap();
        let Some(entry) = store.entries.get(resource) else {
            result["status"] = json!(404);
            return result;
        };
        if op == "stat" {
            result["size"] = json!(entry.size);
            result["mime"] = json!(entry.mime);
            result["etag"] = json!(format!("\"{resource}\""));
            return result;
        }
        let (offset, length) = (offset.unwrap(), length.unwrap());
        if offset + length > entry.size {
            result["status"] = json!(416);
            return result;
        }
        let read = (|| -> std::io::Result<Vec<u8>> {
            let mut file = File::open(&entry.path)?;
            file.seek(SeekFrom::Start(offset))?;
            let mut data = vec![0; length as usize];
            file.read_exact(&mut data)?;
            Ok(data)
        })();
        match read {
            Ok(data) => result["data"] = json!(base64(&data)),
            Err(_) => result["status"] = json!(500),
        };
        result
    }
}

use napi::{Env, Error, Result, bindgen_prelude::Buffer};
use napi_derive::napi;
use std::sync::Arc;
#[napi]
pub fn abi_version() -> u32 {
    1
}
#[napi]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
#[napi]
pub struct LocalContext {
    inner: Arc<omochi_local::binding::Context>,
}
#[napi]
impl LocalContext {
    #[napi(constructor)]
    pub fn new(mut env: Env) -> Result<Self> {
        let inner = Arc::new(omochi_local::binding::Context::default());
        env.add_env_cleanup_hook(inner.clone(), |inner| {
            let _ = std::thread::spawn(move || {
                if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    runtime.block_on(inner.shutdown());
                }
            })
            .join();
        })?;
        Ok(Self { inner })
    }
    #[napi]
    pub async fn request(&self, command: String, data: Buffer) -> Result<Buffer> {
        self.inner
            .request(command.as_bytes(), &data)
            .await
            .map(Into::into)
            .map_err(|e| Error::from_reason(format!("{:?}: {e}", e.kind())))
    }
}

#[napi]
pub fn runtime_root() -> Result<String> {
    omochi_local::default_runtime_root()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|e| Error::from_reason(e.to_string()))
}

#[napi]
pub fn create_private_directory(path: String) -> Result<()> {
    omochi_local::files::new_private_directory(std::path::Path::new(&path))
        .map_err(|e| Error::from_reason(e.to_string()))
}

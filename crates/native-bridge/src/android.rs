//! OS integration only: initialize DNS/network and TLS access before Dart starts.
use jni::{EnvUnowned, errors::ThrowRuntimeExAndDefault, objects::JObject};

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_omochi_omochi_MainActivity_initSharedHub<'local>(
    mut env: EnvUnowned<'local>,
    _activity: JObject<'local>,
    context: JObject<'local>,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let vm = env.get_java_vm()?;
        let context_ref = env.new_global_ref(&context)?;
        rustls_platform_verifier::android::init_with_env(env, context)?;
        static INITIALIZED: std::sync::Once = std::sync::Once::new();
        INITIALIZED.call_once(|| unsafe {
            // iroh's documented Android entry point retains raw VM/context pointers.
            // A global application reference intentionally lives for this process.
            iroh_dns::install_android_jni_context(
                vm.get_raw().cast(),
                context_ref.into_raw().cast(),
            );
        });
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_omochi_mobile_MainActivity_initSharedHub<'local>(
    env: EnvUnowned<'local>,
    _activity: JObject<'local>,
    context: JObject<'local>,
) {
    Java_fyi_omochi_omochi_MainActivity_initSharedHub(env, _activity, context);
}

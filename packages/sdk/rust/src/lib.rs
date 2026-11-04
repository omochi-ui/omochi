#[cfg(feature = "runtime")]
mod input_json;
#[doc(hidden)]
#[cfg(feature = "runtime")]
pub use input_json::checked_json;
#[cfg(feature = "runtime")]
mod components;
#[cfg(feature = "runtime")]
pub use components::*;
#[cfg(feature = "runtime")]
extern crate self as omochi_ui;
#[cfg(feature = "runtime")]
mod input;
#[cfg(feature = "runtime")]
pub use input::{
    ActionInput, AdaptedInput, Expr, ExpressionAdapter, Field, FieldsOnly, Input, IntoActionInput,
    MERGE_OBJECTS_CAPABILITY, Projection, SameValue, Variant, WholeValue, serialize_input_field,
};
#[cfg(feature = "runtime")]
pub use omochi_ui_macros::ActionInput;
#[cfg(feature = "runtime")]
mod app;
#[cfg(feature = "runtime")]
mod builder;
#[cfg(feature = "runtime")]
mod catalog;
mod error;
#[cfg(feature = "runtime")]
mod ir;
#[cfg(feature = "runtime")]
mod reconcile;
mod snapshot;
#[cfg(feature = "runtime")]
mod transport;
#[cfg(feature = "runtime")]
mod unix_server;

#[cfg(feature = "runtime")]
pub use app::{ActionContext, App, Runtime};
#[cfg(feature = "runtime")]
pub use builder::{
    ActionCall, ActionHandle, ButtonOptions, CheckBoxOptions, ChoicePickerOptions,
    DateTimeInputOptions, ImageOptions, LayoutOptions, ListOptions, Local, ModelBinding,
    STANDARD_CATALOG, SliderOptions, Tab, TextFieldOptions, TextOptions, Ui,
};
#[cfg(feature = "runtime")]
pub use catalog::*;
pub use error::{Error, Result};
#[cfg(feature = "runtime")]
pub use ir::{Declaration, JsonObject, LocalDeclaration, Node, Slot};
#[cfg(feature = "runtime")]
pub use reconcile::{PROTOCOL_VERSION, Reconciler};
pub use snapshot::{A2uiSnapshot, SnapshotCursor};
#[cfg(feature = "runtime")]
pub use transport::{CallbackTransport, StdioTransport, Transport};
#[cfg(feature = "runtime")]
pub use unix_server::{
    UnixSocketServerOptions, UnixSocketServerTransport, omochi_runtime_root, validate_service_id,
};

#[cfg(feature = "runtime")]
pub mod resources;
#[cfg(feature = "runtime")]
pub use resources::ResourceRegistry;

#[cfg(feature = "runtime")]
pub type Binding<T> = ModelBinding<T>;

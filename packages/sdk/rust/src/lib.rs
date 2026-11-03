mod input_json;
#[doc(hidden)]
pub use input_json::checked_json;
mod components;
pub use components::*;
extern crate self as omochi_ui;
mod input;
pub use input::{
    ActionInput, AdaptedInput, Expr, ExpressionAdapter, Field, FieldsOnly, Input, IntoActionInput,
    MERGE_OBJECTS_CAPABILITY, Projection, SameValue, Variant, WholeValue, serialize_input_field,
};
pub use omochi_ui_macros::ActionInput;
mod app;
mod builder;
mod catalog;
mod error;
mod ir;
mod reconcile;
mod snapshot;
mod transport;
mod unix_server;

pub use app::{ActionContext, App, Runtime};
pub use builder::{
    ActionCall, ActionHandle, ButtonOptions, CheckBoxOptions, ChoicePickerOptions,
    DateTimeInputOptions, ImageOptions, LayoutOptions, ListOptions, Local, ModelBinding,
    STANDARD_CATALOG, SliderOptions, Tab, TextFieldOptions, TextOptions, Ui,
};
pub use catalog::*;
pub use error::{Error, Result};
pub use ir::{Declaration, JsonObject, LocalDeclaration, Node, Slot};
pub use reconcile::{PROTOCOL_VERSION, Reconciler};
pub use snapshot::{A2uiSnapshot, SnapshotCursor};
pub use transport::{CallbackTransport, StdioTransport, Transport};
pub use unix_server::{
    UnixSocketServerOptions, UnixSocketServerTransport, omochi_runtime_root, validate_service_id,
};

pub mod resources;
pub use resources::ResourceRegistry;

pub type Binding<T> = ModelBinding<T>;

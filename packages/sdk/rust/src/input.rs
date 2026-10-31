use crate::Result;
use serde::{Serialize, Serializer};
use serde_json::Value;
use std::marker::PhantomData;

/// A browser expression. The result type is never part of the wire representation.
#[derive(Clone, Debug)]
pub struct Expr<T> {
    pub(crate) path: String,
    pub(crate) marker: PhantomData<fn() -> T>,
}
impl<T> Serialize for Expr<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("path", &self.path)?;
        map.end()
    }
}

/// A literal, a browser reference, or generated structured input for T.
pub enum Input<T> {
    Literal(T),
    Expression(Expr<T>),
    Structured(StructuredInput<T>),
}
impl<T> From<T> for Input<T> {
    fn from(value: T) -> Self {
        Self::Literal(value)
    }
}
impl<T> Input<T> {
    pub fn expression(value: Expr<T>) -> Self {
        Self::Expression(value)
    }
    pub fn fields(value: impl IntoActionInput<T>) -> Result<Self> {
        Ok(Self::Structured(StructuredInput {
            value: value.encode()?,
            marker: PhantomData,
        }))
    }
    pub fn literal_matches(&self, predicate: impl FnOnce(&T) -> bool) -> bool {
        match self {
            Self::Literal(value) => predicate(value),
            _ => false,
        }
    }
}
impl<T: Default> Default for Input<T> {
    fn default() -> Self {
        Self::Literal(T::default())
    }
}
impl<T: Serialize> Serialize for Input<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::Literal(v) => v.serialize(serializer),
            Self::Expression(v) => v.serialize(serializer),
            Self::Structured(v) => v.value.serialize(serializer),
        }
    }
}
pub trait IntoActionInput<T> {
    fn encode(self) -> Result<Value>;
}
impl<T: Serialize> IntoActionInput<T> for T {
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<T> IntoActionInput<T> for Expr<T> {
    fn encode(self) -> Result<Value> {
        Ok(serde_json::json!({"path": self.path}))
    }
}
pub trait ActionInput: Serialize {
    type Expression: IntoActionInput<Self>
    where
        Self: Sized;
}

/// A typed projection generated from the serialized field name.
pub struct Field<T, F> {
    pub(crate) path: String,
    marker: PhantomData<fn(T, F) -> (T, F)>,
}
impl<T, F> Field<T, F> {
    #[doc(hidden)]
    pub fn new(path: String) -> Self {
        Self {
            path,
            marker: PhantomData,
        }
    }
}

impl From<&str> for Input<String> {
    fn from(v: &str) -> Self {
        Self::Literal(v.into())
    }
}
impl<T> From<Expr<T>> for Input<T> {
    fn from(v: Expr<T>) -> Self {
        Self::Expression(v)
    }
}

/// Explicit support for custom Serde wire shapes. Implementations must handle both
/// literal values and unresolved expressions; no implicit JSON fallback is used.
pub trait ExpressionAdapter<T> {
    type Wire: Serialize;
    fn encode_literal(value: &T) -> Result<Value>;
    fn encode_expression(value: &Expr<Self::Wire>) -> Result<Value> {
        crate::checked_json(value)
    }
    fn project(field_path: &str) -> String;
}
pub struct AdaptedInput<T, A: ExpressionAdapter<T>> {
    input: std::result::Result<T, Expr<A::Wire>>,
}
impl<T, A: ExpressionAdapter<T>> From<T> for AdaptedInput<T, A> {
    fn from(value: T) -> Self {
        Self { input: Ok(value) }
    }
}
impl<T, A: ExpressionAdapter<T>> AdaptedInput<T, A> {
    pub fn literal_matches(&self, predicate: impl FnOnce(&T) -> bool) -> bool {
        match &self.input {
            Ok(value) => predicate(value),
            Err(_) => false,
        }
    }
    pub fn expression(value: Expr<A::Wire>) -> Self {
        Self { input: Err(value) }
    }
    fn encoded(&self) -> Result<Value> {
        match &self.input {
            Ok(v) => A::encode_literal(v),
            Err(v) => A::encode_expression(v),
        }
    }
}
impl<T, A: ExpressionAdapter<T>> Serialize for AdaptedInput<T, A> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.encoded()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}
impl<T, A: ExpressionAdapter<T>> IntoActionInput<T> for AdaptedInput<T, A> {
    fn encode(self) -> Result<Value> {
        self.encoded()
    }
}

/// Compile-time guarantees for typed inputs and controls.
///
/// ```compile_fail
/// use omochi_ui::{App, ActionInput};
/// use serde::{Serialize, Deserialize};
/// #[derive(Serialize, Deserialize, ActionInput)]
/// struct Request { count: i64 }
/// let mut app = App::new(());
/// let run = app.action::<Request, _, _>("run", |_, _| async { Ok(()) }).unwrap();
/// run.call(serde_json::json!({"count":"wrong"})).unwrap();
/// ```
///
/// ```compile_fail
/// use omochi_ui::{Ui, Slider, STANDARD_CATALOG};
/// let mut ui = Ui::new("main", STANDARD_CATALOG);
/// let name = ui.local("name", "text".to_owned()).unwrap();
/// ui.add(Slider::new("number", 0.0, 1.0).bind(name.bind().unwrap())).unwrap();
/// ```
///
/// ```compile_fail
/// use omochi_ui::{Ui, Slider, STANDARD_CATALOG};
/// let mut ui = Ui::new("main", STANDARD_CATALOG);
/// let number = ui.local("number", 1.0).unwrap();
/// let slider = Slider::new("number", 0.0, 1.0).default(0.0).bind(number.bind().unwrap());
/// ```
///
/// ```compile_fail
/// use omochi_ui::ActionInput;
/// use serde::Serialize;
/// #[derive(Serialize, ActionInput)]
/// struct Request { count: i64 }
/// let field = Request::field_missing();
/// ```
#[allow(dead_code)]
pub struct TypeSafetyExamples;

/// A variant guard and the corresponding serialized payload path.
pub struct Variant<T, V> {
    pub(crate) path: String,
    pub(crate) matches: fn(&Value) -> bool,
    marker: PhantomData<fn(T) -> V>,
}
impl<T, V> Variant<T, V> {
    #[doc(hidden)]
    pub fn new(path: String, matches: fn(&Value) -> bool) -> Self {
        Self {
            path,
            matches,
            marker: PhantomData,
        }
    }
}

/// The encoded representation can only be constructed through a typed conversion.
pub struct StructuredInput<T> {
    value: Value,
    marker: PhantomData<fn() -> T>,
}
impl<T: Serialize> IntoActionInput<Vec<T>> for Vec<Input<T>> {
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<T: Serialize, const N: usize> IntoActionInput<[T; N]> for [Input<T>; N]
where
    [Input<T>; N]: Serialize,
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<T: Serialize> IntoActionInput<Option<T>> for Option<Input<T>> {
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<T: Serialize> IntoActionInput<std::collections::BTreeMap<String, T>>
    for std::collections::BTreeMap<String, Input<T>>
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<T: Serialize> IntoActionInput<std::collections::HashMap<String, T>>
    for std::collections::HashMap<String, Input<T>>
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<T0: Serialize> IntoActionInput<(T0,)> for (Input<T0>,) {
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<T0: Serialize, T1: Serialize> IntoActionInput<(T0, T1)> for (Input<T0>, Input<T1>) {
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<T0: Serialize, T1: Serialize, T2: Serialize> IntoActionInput<(T0, T1, T2)>
    for (Input<T0>, Input<T1>, Input<T2>)
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<T0: Serialize, T1: Serialize, T2: Serialize, T3: Serialize> IntoActionInput<(T0, T1, T2, T3)>
    for (Input<T0>, Input<T1>, Input<T2>, Input<T3>)
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<T0: Serialize, T1: Serialize, T2: Serialize, T3: Serialize, T4: Serialize>
    IntoActionInput<(T0, T1, T2, T3, T4)>
    for (Input<T0>, Input<T1>, Input<T2>, Input<T3>, Input<T4>)
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<T0: Serialize, T1: Serialize, T2: Serialize, T3: Serialize, T4: Serialize, T5: Serialize>
    IntoActionInput<(T0, T1, T2, T3, T4, T5)>
    for (
        Input<T0>,
        Input<T1>,
        Input<T2>,
        Input<T3>,
        Input<T4>,
        Input<T5>,
    )
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<
    T0: Serialize,
    T1: Serialize,
    T2: Serialize,
    T3: Serialize,
    T4: Serialize,
    T5: Serialize,
    T6: Serialize,
> IntoActionInput<(T0, T1, T2, T3, T4, T5, T6)>
    for (
        Input<T0>,
        Input<T1>,
        Input<T2>,
        Input<T3>,
        Input<T4>,
        Input<T5>,
        Input<T6>,
    )
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<
    T0: Serialize,
    T1: Serialize,
    T2: Serialize,
    T3: Serialize,
    T4: Serialize,
    T5: Serialize,
    T6: Serialize,
    T7: Serialize,
> IntoActionInput<(T0, T1, T2, T3, T4, T5, T6, T7)>
    for (
        Input<T0>,
        Input<T1>,
        Input<T2>,
        Input<T3>,
        Input<T4>,
        Input<T5>,
        Input<T6>,
        Input<T7>,
    )
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<
    T0: Serialize,
    T1: Serialize,
    T2: Serialize,
    T3: Serialize,
    T4: Serialize,
    T5: Serialize,
    T6: Serialize,
    T7: Serialize,
    T8: Serialize,
> IntoActionInput<(T0, T1, T2, T3, T4, T5, T6, T7, T8)>
    for (
        Input<T0>,
        Input<T1>,
        Input<T2>,
        Input<T3>,
        Input<T4>,
        Input<T5>,
        Input<T6>,
        Input<T7>,
        Input<T8>,
    )
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<
    T0: Serialize,
    T1: Serialize,
    T2: Serialize,
    T3: Serialize,
    T4: Serialize,
    T5: Serialize,
    T6: Serialize,
    T7: Serialize,
    T8: Serialize,
    T9: Serialize,
> IntoActionInput<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)>
    for (
        Input<T0>,
        Input<T1>,
        Input<T2>,
        Input<T3>,
        Input<T4>,
        Input<T5>,
        Input<T6>,
        Input<T7>,
        Input<T8>,
        Input<T9>,
    )
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<
    T0: Serialize,
    T1: Serialize,
    T2: Serialize,
    T3: Serialize,
    T4: Serialize,
    T5: Serialize,
    T6: Serialize,
    T7: Serialize,
    T8: Serialize,
    T9: Serialize,
    T10: Serialize,
> IntoActionInput<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10)>
    for (
        Input<T0>,
        Input<T1>,
        Input<T2>,
        Input<T3>,
        Input<T4>,
        Input<T5>,
        Input<T6>,
        Input<T7>,
        Input<T8>,
        Input<T9>,
        Input<T10>,
    )
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}
impl<
    T0: Serialize,
    T1: Serialize,
    T2: Serialize,
    T3: Serialize,
    T4: Serialize,
    T5: Serialize,
    T6: Serialize,
    T7: Serialize,
    T8: Serialize,
    T9: Serialize,
    T10: Serialize,
    T11: Serialize,
> IntoActionInput<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11)>
    for (
        Input<T0>,
        Input<T1>,
        Input<T2>,
        Input<T3>,
        Input<T4>,
        Input<T5>,
        Input<T6>,
        Input<T7>,
        Input<T8>,
        Input<T9>,
        Input<T10>,
        Input<T11>,
    )
{
    fn encode(self) -> Result<Value> {
        crate::checked_json(self)
    }
}

/// Internal Serde bridge: a flattened unresolved object stays an expression until the click.
#[doc(hidden)]
pub trait FlattenInput: Serialize {
    fn unresolved(&self) -> bool;
}
impl<T: Serialize> FlattenInput for Input<T> {
    fn unresolved(&self) -> bool {
        !matches!(self, Self::Literal(_))
    }
}
impl<T, A: ExpressionAdapter<T>> FlattenInput for AdaptedInput<T, A> {
    fn unresolved(&self) -> bool {
        self.input.is_err()
    }
}
#[doc(hidden)]
pub fn serialize_input_field<T: FlattenInput, S: Serializer>(
    value: &T,
    serializer: S,
    flatten: bool,
    conditional: bool,
) -> std::result::Result<S::Ok, S::Error> {
    if conditional && value.unresolved() {
        return Err(serde::ser::Error::custom(
            "skip_serializing_if cannot evaluate an unresolved input",
        ));
    }
    if flatten && value.unresolved() {
        let encoded = crate::checked_json(value).map_err(serde::ser::Error::custom)?;
        if encoded.get("path").is_some() || encoded.get("call").is_some() {
            use serde::ser::SerializeMap;
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry(&FlattenKey, &encoded)?;
            return map.end();
        }
    }
    value.serialize(serializer)
}
struct FlattenKey;
impl Serialize for FlattenKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_unit_struct("__omochi_flatten_expression")
    }
}

pub const MERGE_OBJECTS_CAPABILITY: &str = "omochi.mergeObjects/1";
pub(crate) fn uses_object_merge(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.get("call").and_then(Value::as_str) == Some("omochiMergeObjects")
                || map.values().any(uses_object_merge)
        }
        Value::Array(items) => items.iter().any(uses_object_merge),
        _ => false,
    }
}

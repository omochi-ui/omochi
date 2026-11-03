//! Typed Basic Catalog component builders.
use crate::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::marker::PhantomData;

pub trait Component {
    fn add_to(self, ui: &mut Ui) -> Result<()>;
}
impl Ui {
    pub fn add(&mut self, component: impl Component) -> Result<()> {
        component.add_to(self)
    }
}
fn value<T: Serialize>(v: T) -> Value {
    serde_json::to_value(v).expect("component property serialization")
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextVariant {
    H1,
    H2,
    H3,
    H4,
    H5,
    Caption,
    Body,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ButtonVariant {
    Default,
    Primary,
    Borderless,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Justify {
    Center,
    End,
    SpaceAround,
    SpaceBetween,
    SpaceEvenly,
    Start,
    Stretch,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageFit {
    Contain,
    Cover,
    Fill,
    None,
    ScaleDown,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageVariant {
    Icon,
    Avatar,
    SmallFeature,
    MediumFeature,
    LargeFeature,
    Header,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Vertical,
    Horizontal,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ListStyle {
    Ordered,
    Unordered,
    None,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextFieldVariant {
    LongText,
    Number,
    ShortText,
    Obscured,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChoiceVariant {
    MultipleSelection,
    MutuallyExclusive,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChoiceDisplay {
    Checkbox,
    Chips,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    Horizontal,
    Vertical,
}

pub struct Text {
    text: Input<String>,
    key: Option<String>,
    variant: Option<TextVariant>,
}
impl Text {
    pub fn new(text: impl Into<Input<String>>) -> Self {
        Self {
            text: text.into(),
            key: None,
            variant: None,
        }
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
    pub fn variant(mut self, variant: TextVariant) -> Self {
        self.variant = Some(variant);
        self
    }
}
impl Component for Text {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.text_with(
            value(self.text),
            TextOptions {
                key: self.key,
                variant: self.variant.map(|v| value(v).as_str().unwrap().into()),
            },
        )
    }
}

pub struct Button<A = Unset> {
    label: Input<String>,
    action: A,
    key: Option<String>,
    variant: Option<ButtonVariant>,
}
impl Button<Unset> {
    pub fn new(label: impl Into<Input<String>>) -> Self {
        Self {
            label: label.into(),
            action: Unset,
            key: None,
            variant: None,
        }
    }
    pub fn on_press(self, action: ActionCall) -> Button<ActionCall> {
        Button {
            label: self.label,
            action,
            key: self.key,
            variant: self.variant,
        }
    }
}
impl<A> Button<A> {
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = Some(variant);
        self
    }
}
impl Component for Button<ActionCall> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.button(
            value(self.label),
            self.action,
            ButtonOptions {
                key: self.key,
                variant: self.variant.map(|v| value(v).as_str().unwrap().into()),
            },
        )
    }
}

pub struct Unset;
pub struct Controlled;

pub struct TextField<C = Unset> {
    name: String,
    props: JsonObject,
    key: Option<String>,
    default: Option<String>,
    bind: Option<ModelBinding<String>>,
    marker: PhantomData<C>,
    label: Value,
}
impl TextField<Unset> {
    pub fn new(name: impl Into<String>, label: impl Into<Input<String>>) -> Self {
        Self {
            name: name.into(),
            props: JsonObject::new(),
            key: None,
            default: None,
            bind: None,
            marker: PhantomData,
            label: value(label.into()),
        }
    }
    pub fn default(self, default: String) -> TextField<Controlled> {
        TextField {
            name: self.name,
            props: self.props,
            key: self.key,
            default: Some(default),
            bind: None,
            marker: PhantomData,
            label: self.label,
        }
    }
    pub fn bind(self, bind: ModelBinding<String>) -> TextField<Controlled> {
        TextField {
            name: self.name,
            props: self.props,
            key: self.key,
            default: None,
            bind: Some(bind),
            marker: PhantomData,
            label: self.label,
        }
    }
}
impl<C> TextField<C> {
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}
impl<C> Component for TextField<C> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.text_field(
            &self.name,
            TextFieldOptions {
                key: self.key,
                default: self.default,
                bind: self.bind,
                label: self.label,
                ..Default::default()
            },
        )?;
        ui.extend_last(self.props);
        Ok(())
    }
}

impl<C> TextField<C> {
    pub fn variant(mut self, v: TextFieldVariant) -> Self {
        self.props.insert("variant".into(), value(v));
        self
    }
    pub fn validation_regexp(mut self, v: String) -> Self {
        self.props.insert("validationRegexp".into(), value(v));
        self
    }
}

pub struct CheckBox<C = Unset> {
    name: String,
    props: JsonObject,
    key: Option<String>,
    default: Option<bool>,
    bind: Option<ModelBinding<bool>>,
    marker: PhantomData<C>,
    label: Value,
}
impl CheckBox<Unset> {
    pub fn new(name: impl Into<String>, label: impl Into<Input<String>>) -> Self {
        Self {
            name: name.into(),
            props: JsonObject::new(),
            key: None,
            default: None,
            bind: None,
            marker: PhantomData,
            label: value(label.into()),
        }
    }
    pub fn default(self, default: bool) -> CheckBox<Controlled> {
        CheckBox {
            name: self.name,
            props: self.props,
            key: self.key,
            default: Some(default),
            bind: None,
            marker: PhantomData,
            label: self.label,
        }
    }
    pub fn bind(self, bind: ModelBinding<bool>) -> CheckBox<Controlled> {
        CheckBox {
            name: self.name,
            props: self.props,
            key: self.key,
            default: None,
            bind: Some(bind),
            marker: PhantomData,
            label: self.label,
        }
    }
}
impl<C> CheckBox<C> {
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}
impl<C> Component for CheckBox<C> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.check_box(
            &self.name,
            CheckBoxOptions {
                key: self.key,
                default: self.default,
                bind: self.bind,
                label: self.label,
                ..Default::default()
            },
        )?;
        ui.extend_last(self.props);
        Ok(())
    }
}

impl<C> CheckBox<C> {}

pub struct Slider<C = Unset> {
    name: String,
    props: JsonObject,
    key: Option<String>,
    default: Option<f64>,
    bind: Option<ModelBinding<f64>>,
    marker: PhantomData<C>,
    min: f64,
    max: f64,
}
impl Slider<Unset> {
    pub fn new(name: impl Into<String>, min: f64, max: f64) -> Self {
        Self {
            name: name.into(),
            props: JsonObject::new(),
            key: None,
            default: None,
            bind: None,
            marker: PhantomData,
            min,
            max,
        }
    }
    pub fn default(self, default: f64) -> Slider<Controlled> {
        Slider {
            name: self.name,
            props: self.props,
            key: self.key,
            default: Some(default),
            bind: None,
            marker: PhantomData,
            min: self.min,
            max: self.max,
        }
    }
    pub fn bind(self, bind: ModelBinding<f64>) -> Slider<Controlled> {
        Slider {
            name: self.name,
            props: self.props,
            key: self.key,
            default: None,
            bind: Some(bind),
            marker: PhantomData,
            min: self.min,
            max: self.max,
        }
    }
}
impl<C> Slider<C> {
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}
impl<C> Component for Slider<C> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.slider(
            &self.name,
            self.min,
            self.max,
            SliderOptions {
                key: self.key,
                default: self.default,
                bind: self.bind,
                ..Default::default()
            },
        )?;
        ui.extend_last(self.props);
        Ok(())
    }
}

impl<C> Slider<C> {
    pub fn label(mut self, v: Input<String>) -> Self {
        self.props.insert("label".into(), value(v));
        self
    }
}

pub struct DateTimeInput<C = Unset> {
    name: String,
    props: JsonObject,
    key: Option<String>,
    default: Option<String>,
    bind: Option<ModelBinding<String>>,
    marker: PhantomData<C>,
}
impl DateTimeInput<Unset> {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            props: JsonObject::new(),
            key: None,
            default: None,
            bind: None,
            marker: PhantomData,
        }
    }
    pub fn default(self, default: String) -> DateTimeInput<Controlled> {
        DateTimeInput {
            name: self.name,
            props: self.props,
            key: self.key,
            default: Some(default),
            bind: None,
            marker: PhantomData,
        }
    }
    pub fn bind(self, bind: ModelBinding<String>) -> DateTimeInput<Controlled> {
        DateTimeInput {
            name: self.name,
            props: self.props,
            key: self.key,
            default: None,
            bind: Some(bind),
            marker: PhantomData,
        }
    }
}
impl<C> DateTimeInput<C> {
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}
impl<C> Component for DateTimeInput<C> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.date_time_input(
            &self.name,
            DateTimeInputOptions {
                key: self.key,
                default: self.default,
                bind: self.bind,
                ..Default::default()
            },
        )?;
        ui.extend_last(self.props);
        Ok(())
    }
}

impl<C> DateTimeInput<C> {
    pub fn label(mut self, v: Input<String>) -> Self {
        self.props.insert("label".into(), value(v));
        self
    }
    pub fn enable_date(mut self, v: bool) -> Self {
        self.props.insert("enableDate".into(), value(v));
        self
    }
    pub fn enable_time(mut self, v: bool) -> Self {
        self.props.insert("enableTime".into(), value(v));
        self
    }
    pub fn min(mut self, v: Input<String>) -> Self {
        self.props.insert("min".into(), value(v));
        self
    }
    pub fn max(mut self, v: Input<String>) -> Self {
        self.props.insert("max".into(), value(v));
        self
    }
}

pub struct ChoicePicker<C = Unset> {
    name: String,
    props: JsonObject,
    key: Option<String>,
    default: Option<Vec<String>>,
    bind: Option<ModelBinding<Vec<String>>>,
    marker: PhantomData<C>,
    choices: Vec<Value>,
}
impl ChoicePicker<Unset> {
    pub fn new(name: impl Into<String>, choices: Vec<Value>) -> Self {
        Self {
            name: name.into(),
            props: JsonObject::new(),
            key: None,
            default: None,
            bind: None,
            marker: PhantomData,
            choices,
        }
    }
    pub fn default(self, default: Vec<String>) -> ChoicePicker<Controlled> {
        ChoicePicker {
            name: self.name,
            props: self.props,
            key: self.key,
            default: Some(default),
            bind: None,
            marker: PhantomData,
            choices: self.choices,
        }
    }
    pub fn bind(self, bind: ModelBinding<Vec<String>>) -> ChoicePicker<Controlled> {
        ChoicePicker {
            name: self.name,
            props: self.props,
            key: self.key,
            default: None,
            bind: Some(bind),
            marker: PhantomData,
            choices: self.choices,
        }
    }
}
impl<C> ChoicePicker<C> {
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}
impl<C> Component for ChoicePicker<C> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.choice_picker(
            &self.name,
            self.choices,
            ChoicePickerOptions {
                key: self.key,
                default: self.default,
                bind: self.bind,
                ..Default::default()
            },
        )?;
        ui.extend_last(self.props);
        Ok(())
    }
}
impl<C> ChoicePicker<C> {
    pub fn label(mut self, v: Input<String>) -> Self {
        self.props.insert("label".into(), value(v));
        self
    }
    pub fn variant(mut self, v: ChoiceVariant) -> Self {
        self.props.insert("variant".into(), value(v));
        self
    }
    pub fn display_style(mut self, v: ChoiceDisplay) -> Self {
        self.props.insert("displayStyle".into(), value(v));
        self
    }
    pub fn filterable(mut self, v: bool) -> Self {
        self.props.insert("filterable".into(), value(v));
        self
    }
}

pub struct Image {
    props: JsonObject,
    key: Option<String>,
}
impl Image {
    pub fn new(url: impl Into<Input<String>>) -> Self {
        Self {
            props: json!({"url": value(url.into())})
                .as_object()
                .unwrap()
                .clone(),
            key: None,
        }
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
    pub fn fit(mut self, v: ImageFit) -> Self {
        self.props.insert("fit".into(), value(v));
        self
    }
    pub fn variant(mut self, v: ImageVariant) -> Self {
        self.props.insert("variant".into(), value(v));
        self
    }
    pub fn description(mut self, v: Input<String>) -> Self {
        self.props.insert("description".into(), value(v));
        self
    }
}
impl Component for Image {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.append("Image", self.props, self.key)?;
        Ok(())
    }
}
pub struct Icon {
    props: JsonObject,
    key: Option<String>,
}
impl Icon {
    pub fn new(name: impl Into<Input<String>>) -> Self {
        Self {
            props: json!({"name": value(name.into())})
                .as_object()
                .unwrap()
                .clone(),
            key: None,
        }
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}
impl Component for Icon {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.append("Icon", self.props, self.key)?;
        Ok(())
    }
}
pub struct Video {
    props: JsonObject,
    key: Option<String>,
}
impl Video {
    pub fn new(url: impl Into<Input<String>>) -> Self {
        Self {
            props: json!({"url": value(url.into())})
                .as_object()
                .unwrap()
                .clone(),
            key: None,
        }
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}
impl Component for Video {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.append("Video", self.props, self.key)?;
        Ok(())
    }
}
pub struct AudioPlayer {
    props: JsonObject,
    key: Option<String>,
}
impl AudioPlayer {
    pub fn new(url: impl Into<Input<String>>) -> Self {
        Self {
            props: json!({"url": value(url.into())})
                .as_object()
                .unwrap()
                .clone(),
            key: None,
        }
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
    pub fn description(mut self, v: Input<String>) -> Self {
        self.props.insert("description".into(), value(v));
        self
    }
}
impl Component for AudioPlayer {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.append("AudioPlayer", self.props, self.key)?;
        Ok(())
    }
}
pub struct Divider {
    props: JsonObject,
    key: Option<String>,
}
impl Divider {
    pub fn new(axis: Axis) -> Self {
        Self {
            props: json!({"axis": value(axis)}).as_object().unwrap().clone(),
            key: None,
        }
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}
impl Component for Divider {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.append("Divider", self.props, self.key)?;
        Ok(())
    }
}
pub struct Row<F> {
    build: F,
    options: LayoutOptions,
}
impl<F: FnOnce(&mut Ui) -> Result<()>> Row<F> {
    pub fn new(build: F) -> Self {
        Self {
            build,
            options: Default::default(),
        }
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.options.key = Some(key.into());
        self
    }
    pub fn align(mut self, align: Align) -> Self {
        self.options.align = Some(value(align).as_str().unwrap().into());
        self
    }
    pub fn justify(mut self, justify: Justify) -> Self {
        self.options.justify = Some(value(justify).as_str().unwrap().into());
        self
    }
}
impl<F: FnOnce(&mut Ui) -> Result<()>> Component for Row<F> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.row(self.options, self.build)
    }
}
pub struct Column<F> {
    build: F,
    options: LayoutOptions,
}
impl<F: FnOnce(&mut Ui) -> Result<()>> Column<F> {
    pub fn new(build: F) -> Self {
        Self {
            build,
            options: Default::default(),
        }
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.options.key = Some(key.into());
        self
    }
    pub fn align(mut self, align: Align) -> Self {
        self.options.align = Some(value(align).as_str().unwrap().into());
        self
    }
    pub fn justify(mut self, justify: Justify) -> Self {
        self.options.justify = Some(value(justify).as_str().unwrap().into());
        self
    }
}
impl<F: FnOnce(&mut Ui) -> Result<()>> Component for Column<F> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.column(self.options, self.build)
    }
}
pub struct List<F> {
    build: F,
    options: ListOptions,
}
impl<F: FnOnce(&mut Ui) -> Result<()>> List<F> {
    pub fn new(build: F) -> Self {
        Self {
            build,
            options: Default::default(),
        }
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.options.key = Some(key.into());
        self
    }
    pub fn align(mut self, align: Align) -> Self {
        self.options.align = Some(value(align).as_str().unwrap().into());
        self
    }
}
impl<F: FnOnce(&mut Ui) -> Result<()>> Component for List<F> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.list(self.options, self.build)
    }
}
pub struct Card<F> {
    build: F,
    options: LayoutOptions,
}
impl<F: FnOnce(&mut Ui) -> Result<()>> Card<F> {
    pub fn new(build: F) -> Self {
        Self {
            build,
            options: Default::default(),
        }
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.options.key = Some(key.into());
        self
    }
}
impl<F: FnOnce(&mut Ui) -> Result<()>> Component for Card<F> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.card(self.options.key, self.build)
    }
}
pub struct Tabs<F> {
    tabs: Vec<Tab<F>>,
    key: Option<String>,
}
impl<F: FnOnce(&mut Ui) -> Result<()>> Tabs<F> {
    pub fn new(tabs: Vec<Tab<F>>) -> Self {
        Self { tabs, key: None }
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}
impl<F: FnOnce(&mut Ui) -> Result<()>> Component for Tabs<F> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.tabs(self.key, self.tabs)
    }
}
pub struct Modal<F, G> {
    trigger: F,
    content: G,
    key: Option<String>,
}
impl<F: FnOnce(&mut Ui) -> Result<()>, G: FnOnce(&mut Ui) -> Result<()>> Modal<F, G> {
    pub fn new(trigger: F, content: G) -> Self {
        Self {
            trigger,
            content,
            key: None,
        }
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}
impl<F: FnOnce(&mut Ui) -> Result<()>, G: FnOnce(&mut Ui) -> Result<()>> Component for Modal<F, G> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.modal(self.key, self.trigger, self.content)
    }
}

/// Add generated custom-catalog properties through the same component interface.
pub struct CatalogComponent<P> {
    key: String,
    props: P,
}
impl<P: ComponentProps> CatalogComponent<P> {
    pub fn new(key: impl Into<String>, props: P) -> Self {
        Self {
            key: key.into(),
            props,
        }
    }
}
impl<P: ComponentProps> Component for CatalogComponent<P> {
    fn add_to(self, ui: &mut Ui) -> Result<()> {
        ui.component(self.key, self.props)
    }
}

impl<F: FnOnce(&mut Ui) -> Result<()>> List<F> {
    pub fn direction(mut self, v: Direction) -> Self {
        self.options.direction = Some(value(v).as_str().unwrap().into());
        self
    }
    pub fn list_style(mut self, v: ListStyle) -> Self {
        self.options.list_style = Some(value(v).as_str().unwrap().into());
        self
    }
}

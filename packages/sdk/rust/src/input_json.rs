//! JSON serializer that rejects duplicate flattened keys and non-finite numbers.
use serde::ser::{self, Error as _};
use serde::{Serialize, Serializer};
use serde_json::{Error, Map, Value};

pub fn checked_json<T: Serialize>(value: T) -> crate::Result<Value> {
    value
        .serialize(Checked)
        .map_err(|e| crate::Error::Declaration(e.to_string()))
}
struct Checked;
struct Sequence {
    values: Vec<Value>,
    variant: Option<String>,
}
struct Object {
    values: Map<String, Value>,
    key: Option<String>,
    variant: Option<String>,
}
impl Object {
    fn insert<T: ?Sized + Serialize>(&mut self, key: String, value: &T) -> Result<(), Error> {
        if self.values.contains_key(&key) {
            return Err(Error::custom(format!("duplicate input key: {key}")));
        }
        self.values.insert(key, value.serialize(Checked)?);
        Ok(())
    }
    fn finish(self) -> Value {
        let v = Value::Object(self.values);
        if let Some(tag) = self.variant {
            serde_json::json!({tag: v})
        } else {
            v
        }
    }
}
impl Serializer for Checked {
    type Ok = Value;
    type Error = Error;
    type SerializeSeq = Sequence;
    type SerializeTuple = Sequence;
    type SerializeTupleStruct = Sequence;
    type SerializeTupleVariant = Sequence;
    type SerializeMap = Object;
    type SerializeStruct = Object;
    type SerializeStructVariant = Object;
    fn serialize_bool(self, v: bool) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_i8(self, v: i8) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_i16(self, v: i16) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_i32(self, v: i32) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_i64(self, v: i64) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_i128(self, v: i128) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_u8(self, v: u8) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_u16(self, v: u16) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_u32(self, v: u32) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_u64(self, v: u64) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_u128(self, v: u128) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_char(self, v: char) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_f32(self, v: f32) -> Result<Value, Error> {
        if !v.is_finite() {
            return Err(Error::custom("non-finite input number"));
        }
        serde_json::to_value(v)
    }
    fn serialize_f64(self, v: f64) -> Result<Value, Error> {
        if !v.is_finite() {
            return Err(Error::custom("non-finite input number"));
        }
        serde_json::to_value(v)
    }
    fn serialize_str(self, v: &str) -> Result<Value, Error> {
        Ok(Value::String(v.into()))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_none(self) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    fn serialize_some<T: ?Sized + Serialize>(self, v: &T) -> Result<Value, Error> {
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<Value, Error> {
        Ok(Value::String(variant.into()))
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<Value, Error> {
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<Value, Error> {
        Ok(serde_json::json!({variant: v.serialize(self)?}))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Sequence, Error> {
        Ok(Sequence {
            values: vec![],
            variant: None,
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<Sequence, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(self, _: &'static str, len: usize) -> Result<Sequence, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Sequence, Error> {
        Ok(Sequence {
            values: vec![],
            variant: Some(variant.into()),
        })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Object, Error> {
        Ok(Object {
            values: Map::new(),
            key: None,
            variant: None,
        })
    }
    fn serialize_struct(self, _: &'static str, len: usize) -> Result<Object, Error> {
        self.serialize_map(Some(len))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Object, Error> {
        Ok(Object {
            values: Map::new(),
            key: None,
            variant: Some(variant.into()),
        })
    }
}
impl ser::SerializeSeq for Sequence {
    type Ok = Value;
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Error> {
        self.values.push(v.serialize(Checked)?);
        Ok(())
    }
    fn end(self) -> Result<Value, Error> {
        let v = Value::Array(self.values);
        Ok(if let Some(tag) = self.variant {
            serde_json::json!({tag:v})
        } else {
            v
        })
    }
}
impl ser::SerializeTuple for Sequence {
    type Ok = Value;
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Error> {
        self.values.push(v.serialize(Checked)?);
        Ok(())
    }
    fn end(self) -> Result<Value, Error> {
        let v = Value::Array(self.values);
        Ok(if let Some(tag) = self.variant {
            serde_json::json!({tag:v})
        } else {
            v
        })
    }
}
impl ser::SerializeTupleStruct for Sequence {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Error> {
        self.values.push(v.serialize(Checked)?);
        Ok(())
    }
    fn end(self) -> Result<Value, Error> {
        let v = Value::Array(self.values);
        Ok(if let Some(tag) = self.variant {
            serde_json::json!({tag:v})
        } else {
            v
        })
    }
}
impl ser::SerializeTupleVariant for Sequence {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Error> {
        self.values.push(v.serialize(Checked)?);
        Ok(())
    }
    fn end(self) -> Result<Value, Error> {
        let v = Value::Array(self.values);
        Ok(if let Some(tag) = self.variant {
            serde_json::json!({tag:v})
        } else {
            v
        })
    }
}
impl ser::SerializeMap for Object {
    type Ok = Value;
    type Error = Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), Error> {
        let v = key.serialize(Checked)?;
        self.key = Some(match v {
            Value::String(s) => s,
            _ => return Err(Error::custom("input map keys must be strings")),
        });
        Ok(())
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Error> {
        let k = self
            .key
            .take()
            .ok_or_else(|| Error::custom("missing map key"))?;
        self.insert(k, v)
    }
    fn end(self) -> Result<Value, Error> {
        Ok(self.finish())
    }
}
impl ser::SerializeStruct for Object {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.insert(key.into(), v)
    }
    fn end(self) -> Result<Value, Error> {
        Ok(self.finish())
    }
}
impl ser::SerializeStructVariant for Object {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.insert(key.into(), v)
    }
    fn end(self) -> Result<Value, Error> {
        Ok(self.finish())
    }
}

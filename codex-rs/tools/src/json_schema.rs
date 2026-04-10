use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde_json::Map as JsonMap;
use serde_json::Value as JsonValue;
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JsonSchemaPrimitiveType {
    String,
    Number,
    Boolean,
    Integer,
    Object,
    Array,
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum JsonSchemaType {
    Single(JsonSchemaPrimitiveType),
    Multiple(Vec<JsonSchemaPrimitiveType>),
}

/// Generic JSON-Schema subset needed for our tool definitions.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonSchema {
    Boolean {
        description: Option<String>,
    },
    String {
        description: Option<String>,
    },
    LiteralString {
        value: String,
        description: Option<String>,
    },
    /// MCP schema allows "number" | "integer" for Number.
    Number {
        description: Option<String>,
    },
    /// Integer type, serialized as "type": "integer".
    Integer {
        description: Option<String>,
    },
    Null {
        description: Option<String>,
    },
    Array {
        items: Box<JsonSchema>,
        description: Option<String>,
    },
    Object {
        properties: BTreeMap<String, JsonSchema>,
        required: Option<Vec<String>>,
        additional_properties: Option<AdditionalProperties>,
    },
    OneOf {
        variants: Vec<JsonSchema>,
    },
    AnyOf {
        variants: Vec<JsonSchema>,
    },
    AllOf {
        variants: Vec<JsonSchema>,
    },
}

impl JsonSchema {
    pub fn boolean(description: Option<String>) -> Self {
        Self::Boolean { description }
    }

    pub fn string(description: Option<String>) -> Self {
        Self::String { description }
    }

    pub fn number(description: Option<String>) -> Self {
        Self::Number { description }
    }

    pub fn integer(description: Option<String>) -> Self {
        Self::Integer { description }
    }

    pub fn null(description: Option<String>) -> Self {
        Self::Null { description }
    }

    pub fn string_enum(values: Vec<JsonValue>, description: Option<String>) -> Self {
        match values.as_slice() {
            [JsonValue::String(value)] => Self::LiteralString {
                value: value.clone(),
                description,
            },
            _ => Self::String { description },
        }
    }

    pub fn array(items: JsonSchema, description: Option<String>) -> Self {
        Self::Array {
            items: Box::new(items),
            description,
        }
    }

    pub fn object(
        properties: BTreeMap<String, JsonSchema>,
        required: Option<Vec<String>>,
        additional_properties: Option<AdditionalProperties>,
    ) -> Self {
        Self::Object {
            properties,
            required,
            additional_properties,
        }
    }

    pub fn any_of(variants: Vec<JsonSchema>, _description: Option<String>) -> Self {
        Self::AnyOf { variants }
    }
}

impl Serialize for JsonSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        schema_to_json(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for JsonSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = JsonValue::deserialize(deserializer)?;
        schema_from_json(value).map_err(serde::de::Error::custom)
    }
}

/// Whether additional properties are allowed, and if so, any required schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AdditionalProperties {
    Boolean(bool),
    Schema(Box<JsonSchema>),
}

impl From<bool> for AdditionalProperties {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<JsonSchema> for AdditionalProperties {
    fn from(value: JsonSchema) -> Self {
        Self::Schema(Box::new(value))
    }
}

/// Parse the tool `input_schema` or return an error for invalid schema.
pub fn parse_tool_input_schema(input_schema: &JsonValue) -> Result<JsonSchema, serde_json::Error> {
    let mut input_schema = input_schema.clone();
    sanitize_json_schema(&mut input_schema);
    if let JsonValue::Object(map) = &input_schema
        && normalized_schema_types(map) == [JsonSchemaPrimitiveType::Null]
    {
        return Err(singleton_null_schema_error());
    }
    serde_json::from_value::<JsonSchema>(input_schema)
}

fn schema_to_json(schema: &JsonSchema) -> JsonValue {
    match schema {
        JsonSchema::Boolean { description } => typed_schema("boolean", description.as_deref()),
        JsonSchema::String { description } => typed_schema("string", description.as_deref()),
        JsonSchema::LiteralString { value, description } => {
            let mut schema = typed_schema("string", description.as_deref());
            if let JsonValue::Object(map) = &mut schema {
                map.insert("enum".to_string(), json!([value]));
            }
            schema
        }
        JsonSchema::Number { description } => typed_schema("number", description.as_deref()),
        JsonSchema::Integer { description } => typed_schema("integer", description.as_deref()),
        JsonSchema::Null { description } => typed_schema("null", description.as_deref()),
        JsonSchema::Array { items, description } => {
            let mut map = JsonMap::from_iter([
                ("type".to_string(), JsonValue::String("array".to_string())),
                ("items".to_string(), schema_to_json(items)),
            ]);
            if let Some(description) = description {
                map.insert(
                    "description".to_string(),
                    JsonValue::String(description.clone()),
                );
            }
            JsonValue::Object(map)
        }
        JsonSchema::Object {
            properties,
            required,
            additional_properties,
        } => {
            let mut map = JsonMap::from_iter([
                ("type".to_string(), JsonValue::String("object".to_string())),
                (
                    "properties".to_string(),
                    JsonValue::Object(
                        properties
                            .iter()
                            .map(|(key, value)| (key.clone(), schema_to_json(value)))
                            .collect(),
                    ),
                ),
            ]);
            if let Some(required) = required {
                map.insert("required".to_string(), json!(required));
            }
            if let Some(additional_properties) = additional_properties {
                map.insert(
                    "additionalProperties".to_string(),
                    match additional_properties {
                        AdditionalProperties::Boolean(value) => JsonValue::Bool(*value),
                        AdditionalProperties::Schema(schema) => schema_to_json(schema),
                    },
                );
            }
            JsonValue::Object(map)
        }
        JsonSchema::OneOf { variants } => {
            json!({ "oneOf": variants.iter().map(schema_to_json).collect::<Vec<_>>() })
        }
        JsonSchema::AnyOf { variants } => {
            json!({ "anyOf": variants.iter().map(schema_to_json).collect::<Vec<_>>() })
        }
        JsonSchema::AllOf { variants } => {
            json!({ "allOf": variants.iter().map(schema_to_json).collect::<Vec<_>>() })
        }
    }
}

fn schema_from_json(value: JsonValue) -> Result<JsonSchema, String> {
    let JsonValue::Object(mut map) = value else {
        return Err("tool schema must be a JSON object".to_string());
    };

    if let Some(one_of) = take_schema_array(&mut map, "oneOf")? {
        return Ok(JsonSchema::OneOf { variants: one_of });
    }
    if let Some(any_of) = take_schema_array(&mut map, "anyOf")? {
        return Ok(JsonSchema::AnyOf { variants: any_of });
    }
    if let Some(all_of) = take_schema_array(&mut map, "allOf")? {
        return Ok(JsonSchema::AllOf { variants: all_of });
    }

    let schema_type_value = map
        .remove("type")
        .ok_or_else(|| "tool schema object missing `type`".to_string())?;
    if let JsonValue::Array(schema_types) = &schema_type_value {
        let variants = schema_types
            .iter()
            .filter_map(JsonValue::as_str)
            .map(schema_from_primitive_type)
            .collect::<Result<Vec<_>, _>>()?;
        return match variants.as_slice() {
            [variant] => Ok(variant.clone()),
            _ => Ok(JsonSchema::AnyOf { variants }),
        };
    }
    let schema_type = schema_type_value
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| "tool schema object missing `type`".to_string())?;
    let description = map
        .remove("description")
        .and_then(|value| value.as_str().map(ToOwned::to_owned));

    match schema_type.as_str() {
        "boolean" => Ok(JsonSchema::Boolean { description }),
        "string" => {
            let literal = parse_literal_string(map.remove("enum"), map.remove("const"))?;
            Ok(match literal {
                Some(value) => JsonSchema::LiteralString { value, description },
                None => JsonSchema::String { description },
            })
        }
        "number" => Ok(JsonSchema::Number { description }),
        "integer" => Ok(JsonSchema::Integer { description }),
        "null" => Ok(JsonSchema::Null { description }),
        "array" => {
            let items = map
                .remove("items")
                .ok_or_else(|| "array schema missing `items`".to_string())?;
            Ok(JsonSchema::Array {
                items: Box::new(schema_from_json(items)?),
                description,
            })
        }
        "object" => {
            let properties = match map.remove("properties") {
                Some(JsonValue::Object(properties)) => properties
                    .into_iter()
                    .map(|(key, value)| Ok((key, schema_from_json(value)?)))
                    .collect::<Result<BTreeMap<_, _>, String>>()?,
                Some(_) => return Err("object schema `properties` must be an object".to_string()),
                None => BTreeMap::new(),
            };
            let required = match map.remove("required") {
                Some(JsonValue::Array(required)) => Some(
                    required
                        .into_iter()
                        .map(|value| {
                            value.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                                "object schema `required` values must be strings".to_string()
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                Some(_) => return Err("object schema `required` must be an array".to_string()),
                None => None,
            };
            let additional_properties = match map.remove("additionalProperties") {
                Some(JsonValue::Bool(value)) => Some(AdditionalProperties::Boolean(value)),
                Some(value @ JsonValue::Object(_)) => Some(AdditionalProperties::Schema(Box::new(
                    schema_from_json(value)?,
                ))),
                Some(_) => {
                    return Err(
                        "object schema `additionalProperties` must be a boolean or object"
                            .to_string(),
                    );
                }
                None => None,
            };
            Ok(JsonSchema::Object {
                properties,
                required,
                additional_properties,
            })
        }
        other => Err(format!("unsupported tool schema type `{other}`")),
    }
}

fn typed_schema(schema_type: &str, description: Option<&str>) -> JsonValue {
    let mut map = JsonMap::from_iter([(
        "type".to_string(),
        JsonValue::String(schema_type.to_string()),
    )]);
    if let Some(description) = description {
        map.insert(
            "description".to_string(),
            JsonValue::String(description.to_string()),
        );
    }
    JsonValue::Object(map)
}

fn take_schema_array(
    map: &mut JsonMap<String, JsonValue>,
    key: &str,
) -> Result<Option<Vec<JsonSchema>>, String> {
    let Some(value) = map.remove(key) else {
        return Ok(None);
    };
    let JsonValue::Array(values) = value else {
        return Err(format!("tool schema `{key}` must be an array"));
    };
    values
        .into_iter()
        .map(schema_from_json)
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn parse_literal_string(
    enum_value: Option<JsonValue>,
    const_value: Option<JsonValue>,
) -> Result<Option<String>, String> {
    if let Some(JsonValue::Array(values)) = enum_value
        && values.len() == 1
        && let Some(value) = values[0].as_str()
    {
        return Ok(Some(value.to_string()));
    }
    if let Some(JsonValue::String(value)) = const_value {
        return Ok(Some(value));
    }
    Ok(None)
}

fn schema_from_primitive_type(schema_type: &str) -> Result<JsonSchema, String> {
    match schema_type {
        "string" => Ok(JsonSchema::String { description: None }),
        "number" => Ok(JsonSchema::Number { description: None }),
        "boolean" => Ok(JsonSchema::Boolean { description: None }),
        "integer" => Ok(JsonSchema::Integer { description: None }),
        "object" => Ok(JsonSchema::Object {
            properties: BTreeMap::new(),
            required: None,
            additional_properties: None,
        }),
        "array" => Ok(JsonSchema::Array {
            items: Box::new(JsonSchema::String { description: None }),
            description: None,
        }),
        "null" => Ok(JsonSchema::Null { description: None }),
        other => Err(format!("unsupported tool schema type `{other}`")),
    }
}

/// Sanitize a JSON Schema (as serde_json::Value) so it can fit our limited
/// JsonSchema enum while preserving nullable unions.
fn sanitize_json_schema(value: &mut JsonValue) {
    match value {
        JsonValue::Bool(_) => {
            *value = json!({ "type": "string" });
        }
        JsonValue::Array(values) => {
            for value in values {
                sanitize_json_schema(value);
            }
        }
        JsonValue::Object(map) => {
            if let Some(properties) = map.get_mut("properties")
                && let Some(properties_map) = properties.as_object_mut()
            {
                for value in properties_map.values_mut() {
                    sanitize_json_schema(value);
                }
            }
            if let Some(items) = map.get_mut("items") {
                sanitize_json_schema(items);
            }
            for combiner in ["oneOf", "anyOf", "allOf", "prefixItems"] {
                if let Some(value) = map.get_mut(combiner) {
                    sanitize_json_schema(value);
                }
            }

            if let Some(additional_properties) = map.get_mut("additionalProperties")
                && !matches!(additional_properties, JsonValue::Bool(_))
            {
                sanitize_json_schema(additional_properties);
            }
            if let Some(const_value) = map.remove("const") {
                map.insert("enum".to_string(), JsonValue::Array(vec![const_value]));
            }

            let mut schema_types = normalized_schema_types(map);

            if schema_types.is_empty()
                && !(map.contains_key("oneOf")
                    || map.contains_key("anyOf")
                    || map.contains_key("allOf"))
            {
                if map.contains_key("properties")
                    || map.contains_key("required")
                    || map.contains_key("additionalProperties")
                {
                    schema_types.push(JsonSchemaPrimitiveType::Object);
                } else if map.contains_key("items") || map.contains_key("prefixItems") {
                    schema_types.push(JsonSchemaPrimitiveType::Array);
                } else if map.contains_key("enum") || map.contains_key("format") {
                    schema_types.push(JsonSchemaPrimitiveType::String);
                } else if map.contains_key("minimum")
                    || map.contains_key("maximum")
                    || map.contains_key("exclusiveMinimum")
                    || map.contains_key("exclusiveMaximum")
                    || map.contains_key("multipleOf")
                {
                    schema_types.push(JsonSchemaPrimitiveType::Number);
                } else {
                    schema_types.push(JsonSchemaPrimitiveType::String);
                }
            }

            if !schema_types.is_empty() {
                write_schema_types(map, &schema_types);
                ensure_default_children_for_schema_types(map, &schema_types);
            }
        }
        _ => {}
    }
}

fn ensure_default_children_for_schema_types(
    map: &mut JsonMap<String, JsonValue>,
    schema_types: &[JsonSchemaPrimitiveType],
) {
    if schema_types.contains(&JsonSchemaPrimitiveType::Object) && !map.contains_key("properties") {
        map.insert("properties".to_string(), JsonValue::Object(JsonMap::new()));
    }
    if schema_types.contains(&JsonSchemaPrimitiveType::Array) && !map.contains_key("items") {
        map.insert("items".to_string(), json!({ "type": "string" }));
    }
}

fn normalized_schema_types(map: &JsonMap<String, JsonValue>) -> Vec<JsonSchemaPrimitiveType> {
    let Some(schema_type) = map.get("type") else {
        return Vec::new();
    };
    match schema_type {
        JsonValue::String(schema_type) => schema_type_from_str(schema_type).into_iter().collect(),
        JsonValue::Array(schema_types) => schema_types
            .iter()
            .filter_map(JsonValue::as_str)
            .filter_map(schema_type_from_str)
            .collect(),
        _ => Vec::new(),
    }
}

fn write_schema_types(
    map: &mut JsonMap<String, JsonValue>,
    schema_types: &[JsonSchemaPrimitiveType],
) {
    match schema_types {
        [] => {
            map.remove("type");
        }
        [schema_type] => {
            map.insert(
                "type".to_string(),
                JsonValue::String(schema_type_name(*schema_type).to_string()),
            );
        }
        _ => {
            map.insert(
                "type".to_string(),
                JsonValue::Array(
                    schema_types
                        .iter()
                        .map(|schema_type| {
                            JsonValue::String(schema_type_name(*schema_type).to_string())
                        })
                        .collect(),
                ),
            );
        }
    }
}

fn schema_type_from_str(schema_type: &str) -> Option<JsonSchemaPrimitiveType> {
    match schema_type {
        "string" => Some(JsonSchemaPrimitiveType::String),
        "number" => Some(JsonSchemaPrimitiveType::Number),
        "boolean" => Some(JsonSchemaPrimitiveType::Boolean),
        "integer" => Some(JsonSchemaPrimitiveType::Integer),
        "object" => Some(JsonSchemaPrimitiveType::Object),
        "array" => Some(JsonSchemaPrimitiveType::Array),
        "null" => Some(JsonSchemaPrimitiveType::Null),
        _ => None,
    }
}

fn schema_type_name(schema_type: JsonSchemaPrimitiveType) -> &'static str {
    match schema_type {
        JsonSchemaPrimitiveType::String => "string",
        JsonSchemaPrimitiveType::Number => "number",
        JsonSchemaPrimitiveType::Boolean => "boolean",
        JsonSchemaPrimitiveType::Integer => "integer",
        JsonSchemaPrimitiveType::Object => "object",
        JsonSchemaPrimitiveType::Array => "array",
        JsonSchemaPrimitiveType::Null => "null",
    }
}

fn singleton_null_schema_error() -> serde_json::Error {
    serde_json::Error::io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "tool input schema must not be a singleton null type",
    ))
}

#[cfg(test)]
#[path = "json_schema_tests.rs"]
mod tests;

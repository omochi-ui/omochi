use anyhow::{Context, Result, bail};
use omochi_protocol::session::{valid_node_id, valid_service_id};
use serde_json::{Map, Value};

pub fn split_route(route: &str) -> Result<(&str, &str)> {
    let (node, service) = route.split_once('~').context("missing node")?;
    if !valid_node_id(node) || !valid_service_id(service) {
        bail!("invalid route")
    }
    Ok((node, service))
}
pub fn prefix(route: &str, mut message: Value) -> Result<Value> {
    split_route(route)?;
    if message["version"] != "v0.9" {
        bail!("unsupported A2UI version")
    }
    for key in [
        "createSurface",
        "updateComponents",
        "updateDataModel",
        "deleteSurface",
    ] {
        if let Some(body) = message.get_mut(key) {
            let id = body
                .get("surfaceId")
                .and_then(Value::as_str)
                .context("missing surface ID")?;
            body["surfaceId"] = format!("{route}/{id}").into();
            return Ok(message);
        }
    }
    bail!("invalid server message")
}
pub fn route_action(mut message: Value) -> Result<(String, Value)> {
    if message["version"] != "v0.9" {
        bail!("unsupported A2UI version")
    }
    for key in ["action", "error"] {
        if let Some(body) = message.get_mut(key) {
            let id = body
                .get("surfaceId")
                .and_then(Value::as_str)
                .context("missing surface ID")?;
            let (route, surface) = id.split_once('/').context("missing route")?;
            split_route(route)?;
            if surface.is_empty() {
                bail!("empty surface")
            }
            let route = route.to_owned();
            body["surfaceId"] = surface.into();
            if let Some(surfaces) = message.pointer_mut("/metadata/a2uiClientDataModel/surfaces") {
                let object = surfaces.as_object().context("invalid client data model")?;
                let mut scoped = Map::new();
                for (key, value) in object {
                    if let Some((channel, surface)) = key.split_once('/')
                        && channel == route
                    {
                        scoped.insert(surface.into(), value.clone());
                    }
                }
                *surfaces = Value::Object(scoped);
            }
            return Ok((route, message));
        }
    }
    bail!("invalid client message")
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn scopes_drafts_to_only_the_target_service_and_preserves_nested_surface_ids() {
        let route = format!("{}~app", "a".repeat(64));
        let other = format!("{}~other", "a".repeat(64));
        let input = json!({"version":"v0.9","action":{"surfaceId":format!("{route}/nested/main"),"name":"click"},"metadata":{"a2uiClientDataModel":{"surfaces":{format!("{route}/nested/main"): {"draft":"mine"},format!("{other}/main"): {"secret":"other"}}}}});
        let (target, output) = route_action(input).unwrap();
        assert_eq!(target, route);
        assert_eq!(output["action"]["surfaceId"], "nested/main");
        assert_eq!(
            output
                .pointer("/metadata/a2uiClientDataModel/surfaces")
                .unwrap(),
            &json!({"nested/main":{"draft":"mine"}})
        );
    }
}

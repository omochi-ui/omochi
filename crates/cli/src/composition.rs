//! Existing composition format, executed without a shell or a JavaScript runtime.
use anyhow::{Context, Result, bail};
use omochi_hub::dashboards::{Panel, Reference};
use omochi_process::{Child, Command, Stdio};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    id: String,
    services: BTreeMap<String, Service>,
    dashboard: Dashboard,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Service {
    command: Option<Vec<String>>,
    utility: Option<Vec<String>>,
    cwd: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Dashboard {
    name: String,
    rows: Vec<Row>,
    panels: BTreeMap<String, Definition>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    panels: Vec<String>,
    height: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    title: String,
    service: String,
    surface: Option<String>,
}
pub struct ServiceCommand {
    argv: Vec<String>,
    cwd: PathBuf,
    env: BTreeMap<String, String>,
}
pub struct Composition {
    pub services: Vec<ServiceCommand>,
    pub preset: Value,
}

fn text(value: &str, max: usize) -> Result<String> {
    if value.trim().is_empty() || value.encode_utf16().count() > max {
        bail!("invalid text (expected 1..{max} characters)");
    }
    Ok(value.trim().to_owned())
}
pub fn load(file: &Path, node: &str) -> Result<Composition> {
    let file = file
        .canonicalize()
        .with_context(|| format!("could not read {}", file.display()))?;
    let source = std::fs::read_to_string(&file)?;
    parse(&source, file.parent().unwrap(), node)
}
fn parse(source: &str, root: &Path, node: &str) -> Result<Composition> {
    if !omochi_protocol::session::valid_node_id(node) {
        bail!("invalid local node identity");
    }
    let doc: Document = toml::from_str(source)?;
    if doc.version != 1 {
        bail!("composition.version must be 1");
    }
    let id = text(&doc.id, 100)?;
    let name = text(&doc.dashboard.name, 200)?;
    if doc.services.is_empty() || doc.services.len() > 256 {
        bail!("composition requires 1..256 services");
    }
    let mut services = Vec::new();
    for (id, service) in &doc.services {
        if omochi_local::validate_service_id(id).is_err() {
            bail!("invalid service id: {id}");
        }
        let argv = match (&service.command, &service.utility) {
            (Some(argv), None) => argv.clone(),
            (None, Some(utility)) => {
                if !utility.first().is_some_and(|s| {
                    matches!(s.as_str(), "button" | "watch" | "process" | "tail" | "http")
                }) {
                    bail!("invalid utility mode for {id}");
                }
                let executable = std::env::current_exe()?.with_file_name("omochi-utils");
                let mut argv = vec![
                    executable.to_string_lossy().into_owned(),
                    utility[0].clone(),
                    "--service-id".into(),
                    id.clone(),
                ];
                argv.extend_from_slice(&utility[1..]);
                argv
            }
            _ => bail!("service {id} requires exactly one of command or utility"),
        };
        if argv.is_empty() || argv.iter().any(|a| a.is_empty() || a.contains('\0')) {
            bail!("service {id} command must be a non-empty array of strings");
        }
        let cwd = match &service.cwd {
            Some(cwd) => root.join(text(cwd, 4096)?),
            None => root.to_owned(),
        };
        let mut env = service.env.clone();
        if env
            .iter()
            .any(|(k, v)| k.is_empty() || k.contains(['=', '\0']) || v.contains('\0'))
        {
            bail!("invalid environment for {id}");
        }
        env.insert("OMOCHI_SERVICE".into(), id.clone());
        services.push(ServiceCommand { argv, cwd, env });
    }
    if doc.dashboard.rows.is_empty() || doc.dashboard.panels.is_empty() {
        bail!("dashboard needs rows and panels");
    }
    let mut panels = Vec::new();
    let mut placed = BTreeSet::new();
    let mut y = 0;
    for row in doc.dashboard.rows {
        let count = row.panels.len();
        if !(1..=12).contains(&count) {
            bail!("rows must contain 1..12 panels");
        }
        let height = match row.height.as_deref().unwrap_or("medium") {
            "small" => 4,
            "medium" => 6,
            "large" => 9,
            _ => bail!("invalid row height"),
        };
        let mut x = 0;
        for (column, key) in row.panels.into_iter().enumerate() {
            if !placed.insert(key.clone()) {
                bail!("panel {key} placed twice");
            }
            let panel = doc
                .dashboard
                .panels
                .get(&key)
                .with_context(|| format!("unknown panel {key}"))?;
            let service = text(&panel.service, 64)?;
            if !doc.services.contains_key(&service) {
                bail!("undeclared service {service}");
            }
            let w = (12 / count + usize::from(column < 12 % count)) as u32;
            panels.push(Panel {
                id: text(&key, 100)?,
                title: text(&panel.title, 200)?,
                reference: Reference {
                    node_id: node.into(),
                    service_id: service,
                    surface_id: text(panel.surface.as_deref().unwrap_or("main"), 4096)?,
                },
                x,
                y,
                w,
                h: height,
            });
            x += w;
        }
        y += height;
    }
    if placed.len() != doc.dashboard.panels.len() {
        bail!("every panel must be placed in a row");
    }
    let input = omochi_hub::dashboards::validate(json!({"name":name,"panels":panels}))
        .map_err(|e| anyhow::anyhow!(e.1))?;
    Ok(Composition {
        services,
        preset: json!({"id":id,"name":input.name,"panels":input.panels}),
    })
}

/// Each command owns a process group so grandchildren are also terminated.
pub struct Supervisor {
    children: Vec<Child>,
    groups: Vec<omochi_process::Group>,
}
impl Supervisor {
    pub fn start(services: &[ServiceCommand], runtime: &Path) -> Result<Self> {
        let mut supervisor = Self {
            children: vec![],
            groups: vec![],
        };
        for service in services {
            let mut command = Command::new(&service.argv[0]);
            command
                .args(&service.argv[1..])
                .current_dir(&service.cwd)
                .envs(&service.env)
                .env("OMOCHI_RUNTIME_DIR", runtime)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
            let child = command
                .spawn()
                .with_context(|| format!("could not start {}", service.argv[0]))?;
            supervisor.groups.push(child.group());
            supervisor.children.push(child);
        }
        Ok(supervisor)
    }
    fn signal(&self, signal: i32) {
        for group in &self.groups {
            group.signal(signal);
        }
    }

    pub async fn close(&mut self) {
        self.signal(omochi_process::SIGTERM);
        let _ = tokio::time::timeout(Duration::from_secs(1), async {
            for child in &mut self.children {
                let _ = child.wait().await;
            }
        })
        .await;
        self.signal(omochi_process::SIGKILL);
        for child in &mut self.children {
            let _ = child.wait().await;
        }
        self.groups.clear();
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        self.signal(omochi_process::SIGKILL);
    }
}
pub fn open_browser(url: &str) {
    if let Err(error) = omochi_process::open_browser(url) {
        eprintln!("[omochi-ui] could not open browser: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = r#"
version = 1
id = "work"
[services.test]
command = ["echo", "hello"]
[dashboard]
name = "Work"
rows = [{panels = ["one", "two"], height = "small"}]
[dashboard.panels.one]
title = "One"
service = "test"
[dashboard.panels.two]
title = "Two"
service = "test"
surface = "other"
"#;
    #[test]
    fn semantic_rows_preserve_references_and_reject_invalid_compositions() {
        let node = "a".repeat(64);
        let c = parse(SOURCE, Path::new("/tmp"), &node).unwrap();
        assert_eq!(c.preset["panels"][1]["x"], 6);
        assert_eq!(c.preset["panels"][1]["reference"]["nodeId"], node);
        assert_eq!(c.services[0].env["OMOCHI_SERVICE"], "test");
        assert!(
            parse(
                &SOURCE.replace("\"one\", \"two\"", "\"one\", \"one\""),
                Path::new("/tmp"),
                &node
            )
            .is_err()
        );
        assert!(
            parse(
                &SOURCE.replace("service = \"test\"", "service = \"missing\""),
                Path::new("/tmp"),
                &node
            )
            .is_err()
        );
        assert!(
            parse(
                &SOURCE.replace("version = 1", "version = 1\nunknown = true"),
                Path::new("/tmp"),
                &node
            )
            .is_err()
        );
    }
}

use crate::patent::{CadArtifact, CadAvailability, CadStatus, CadValidation};
use anyhow::{anyhow, bail, Context, Result};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct BridgeOrigin(String);

impl BridgeOrigin {
    pub fn parse(value: &str) -> Result<Self> {
        let parsed = Url::parse(value).context("invalid AionCAD bridge URL")?;
        let loopback = parsed.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if parsed.scheme() != "http"
            || !loopback
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.path() != "/"
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            bail!("AionCAD bridge must be loopback-only HTTP");
        }
        Ok(Self(value.trim_end_matches('/').to_string()))
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.0)
    }
}

#[derive(Clone)]
pub struct CadService {
    client: reqwest::Client,
    origin: BridgeOrigin,
    cad_root: PathBuf,
    workspace: Option<PathBuf>,
    startup_lock: Arc<Mutex<()>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedCadBundle {
    pub preview_rel_path: String,
    pub fcstd_rel_path: String,
    pub step_rel_path: Option<String>,
    pub validation: CadValidation,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
struct BridgeDrawRequest<'a> {
    text: &'a str,
    assumptions: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    model_path: Option<String>,
}

impl CadService {
    pub fn new(cad_root: PathBuf, workspace: Option<PathBuf>) -> Result<Self> {
        let origin = BridgeOrigin::parse(
            &std::env::var("INNOFORGE_AIONCAD_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8010".to_string()),
        )?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;
        std::fs::create_dir_all(&cad_root)?;
        let cad_root = cad_root
            .canonicalize()
            .context("failed to resolve the CAD artifact root")?;
        Ok(Self {
            client,
            origin,
            cad_root,
            workspace,
            startup_lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn cad_root(&self) -> &Path {
        &self.cad_root
    }

    pub async fn status(&self) -> CadStatus {
        match self.check_ready().await {
            Ok(()) => CadStatus {
                availability: CadAvailability::Ready,
                message: "FreeCAD ready".to_string(),
            },
            Err(_) => CadStatus {
                availability: CadAvailability::Unavailable,
                message: "FreeCAD is not ready".to_string(),
            },
        }
    }

    async fn check_ready(&self) -> Result<()> {
        let health: Value = self
            .client
            .get(self.origin.endpoint("/health"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if !is_compatible_health(&health) {
            bail!("AionCAD bridge does not expose the required API identity");
        }
        let expected_import_root = self.cad_root.canonicalize()?;
        let actual_import_root = health
            .get("artifact_import_root")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .context("AionCAD bridge has no artifact import root")?
            .canonicalize()?;
        if actual_import_root != expected_import_root {
            bail!("AionCAD bridge is bound to another artifact import root");
        }
        let artifact_route = self
            .client
            .get(self.origin.endpoint("/draw/artifact"))
            .send()
            .await?;
        if artifact_route.status() != reqwest::StatusCode::METHOD_NOT_ALLOWED {
            bail!("AionCAD bridge does not support artifact bundles");
        }
        let status: Value = self
            .client
            .get(self.origin.endpoint("/draw/status"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if status
            .pointer("/gui/worker_connected")
            .and_then(Value::as_bool)
            != Some(true)
        {
            bail!("FreeCAD worker is not connected");
        }
        let view: Value = self
            .client
            .get(self.origin.endpoint("/draw/view"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if view
            .get("view_path")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            bail!("FreeCAD preview is unavailable");
        }
        Ok(())
    }

    pub async fn ensure_ready(&self) -> CadStatus {
        #[cfg(not(target_os = "windows"))]
        {
            return CadStatus {
                availability: CadAvailability::Unsupported,
                message: "FreeCAD auto-start is supported on Windows desktop".to_string(),
            };
        }
        #[cfg(target_os = "windows")]
        {
            if self.check_ready().await.is_ok() {
                return CadStatus {
                    availability: CadAvailability::Ready,
                    message: "FreeCAD ready".to_string(),
                };
            }
            let _startup_guard = self.startup_lock.lock().await;
            if self.check_ready().await.is_ok() {
                return CadStatus {
                    availability: CadAvailability::Ready,
                    message: "FreeCAD ready".to_string(),
                };
            }
            let Some(workspace) = self.workspace.as_deref() else {
                return CadStatus {
                    availability: CadAvailability::Unavailable,
                    message: "AionCAD workspace is not configured".to_string(),
                };
            };
            let script = workspace.join("bootstrap_bridge.ps1");
            if !script.is_file() {
                return CadStatus {
                    availability: CadAvailability::Unavailable,
                    message: "AionCAD bootstrap script was not found".to_string(),
                };
            }
            let result = tokio::time::timeout(
                Duration::from_secs(105),
                tokio::process::Command::new("powershell")
                    .args([
                        "-WindowStyle",
                        "Hidden",
                        "-NoProfile",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-File",
                    ])
                    .arg(&script)
                    .args(["-Port", "8010", "-ReadyTimeoutSeconds", "90"])
                    .env("AIONCAD_ARTIFACT_IMPORT_ROOT", &self.cad_root)
                    .current_dir(workspace)
                    .output(),
            )
            .await;
            if !matches!(result, Ok(Ok(output)) if output.status.success()) {
                return CadStatus {
                    availability: CadAvailability::Unavailable,
                    message: "FreeCAD could not be started".to_string(),
                };
            }
            self.status().await
        }
    }

    pub async fn draw_artifact(
        &self,
        text: &str,
        assumptions: &[String],
        parent: Option<&CadArtifact>,
    ) -> Result<ImportedCadBundle> {
        let model_path = parent
            .map(|artifact| resolve_artifact_path(&self.cad_root, &artifact.fcstd_rel_path))
            .transpose()?
            .map(|path| path.display().to_string());
        let response: Value = self
            .client
            .post(self.origin.endpoint("/draw/artifact"))
            .json(&BridgeDrawRequest {
                text,
                assumptions,
                model_path,
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let artifacts = response
            .get("artifacts")
            .context("AionCAD response has no artifacts")?;
        let preview = required_source_path(artifacts, "preview_path")?;
        let fcstd = required_source_path(artifacts, "fcstd_path")?;
        let step = artifacts
            .get("step_path")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        let id = Uuid::new_v4().to_string();
        let target_dir = self.cad_root.join(&id);
        std::fs::create_dir_all(&target_dir)?;
        atomic_copy(&preview, &target_dir.join("preview.png"))?;
        atomic_copy(&fcstd, &target_dir.join("model.FCStd"))?;
        if let Some(source) = step.as_deref() {
            atomic_copy(source, &target_dir.join("model.step"))?;
        }
        let validation: CadValidation =
            serde_json::from_value(response.get("validation").cloned().unwrap_or_else(
                || serde_json::json!({"valid":false,"issues":["missing validation"]}),
            ))?;
        Ok(ImportedCadBundle {
            preview_rel_path: format!("{id}/preview.png"),
            fcstd_rel_path: format!("{id}/model.FCStd"),
            step_rel_path: step.as_ref().map(|_| format!("{id}/model.step")),
            validation,
            warnings: serde_json::from_value(
                response
                    .get("warnings")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!([])),
            )?,
        })
    }
}

fn is_compatible_health(health: &Value) -> bool {
    health.get("status").and_then(Value::as_str) == Some("ok")
        && health.get("backend").and_then(Value::as_str) == Some("rust-live")
        && health.get("api_schema").and_then(Value::as_str) == Some("aioncad.rust-bridge.v2")
}

fn required_source_path(value: &Value, key: &str) -> Result<PathBuf> {
    let path = value
        .get(key)
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| anyhow!("AionCAD required artifact is missing"))?;
    let path = PathBuf::from(path);
    let metadata = std::fs::metadata(&path)?;
    if !metadata.is_file() || metadata.len() == 0 {
        bail!("AionCAD artifact is empty");
    }
    Ok(path)
}

fn atomic_copy(source: &Path, target: &Path) -> Result<()> {
    let temp = target.with_extension(format!("tmp-{}", Uuid::new_v4()));
    std::fs::copy(source, &temp)?;
    if std::fs::metadata(&temp)?.len() == 0 {
        bail!("copied CAD artifact is empty");
    }
    std::fs::rename(&temp, target)?;
    Ok(())
}

pub fn resolve_artifact_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid CAD artifact path");
    }
    Ok(root.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_url_accepts_only_loopback_http() {
        assert!(BridgeOrigin::parse("http://127.0.0.1:8010").is_ok());
        assert!(BridgeOrigin::parse("http://localhost:8010").is_ok());
        assert!(BridgeOrigin::parse("https://127.0.0.1:8010").is_err());
        assert!(BridgeOrigin::parse("http://192.168.1.10:8010").is_err());
        assert!(BridgeOrigin::parse("http://example.com").is_err());
    }

    #[test]
    fn artifact_path_must_remain_beneath_cad_root() {
        let root = std::env::temp_dir().join(format!("innoforge-cad-{}", Uuid::new_v4()));
        assert!(resolve_artifact_path(&root, "abc/preview.png").is_ok());
        assert!(resolve_artifact_path(&root, "../secret.txt").is_err());
    }

    #[test]
    fn bridge_health_requires_the_artifact_api_identity() {
        assert!(is_compatible_health(&serde_json::json!({
            "status": "ok",
            "backend": "rust-live",
            "version": "0.4.0",
            "api_schema": "aioncad.rust-bridge.v2"
        })));
        assert!(!is_compatible_health(&serde_json::json!({
            "status": "ok",
            "backend": "rust-live",
            "version": "0.3.0"
        })));
    }

    #[test]
    fn cad_service_resolves_the_artifact_root_to_an_absolute_path() {
        let root = std::env::temp_dir().join(format!("innoforge-cad-root-{}", Uuid::new_v4()));
        let service = CadService::new(root.clone(), None).expect("CAD service");
        assert!(service.cad_root().is_absolute());
        assert_eq!(
            service.cad_root(),
            root.canonicalize().expect("canonical CAD root")
        );
    }
}

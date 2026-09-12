use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub name: String,
    pub slug: String,
    pub status: String,
    pub is_running: bool,
    pub image: String,
    pub cpu: String,
    pub mem: String,
}

pub struct PodmanManager;

impl PodmanManager {
    pub fn list_containers() -> Vec<ContainerInfo> {
        let mut list = Vec::new();

        let output = match Command::new("podman")
            .args(["ps", "-a", "--format", "{{.Names}}\t{{.Status}}\t{{.Image}}"])
            .output()
        {
            Ok(out) => out,
            Err(_) => return list,
        };

        let raw = String::from_utf8_lossy(&output.stdout);
        for line in raw.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let name = parts[0].trim().to_string();
                let status = parts[1].trim().to_string();
                let image = parts[2].trim().to_string();
                let is_running = status.starts_with("Up ");
                let slug = name.trim_start_matches("connector-").to_string();

                list.push(ContainerInfo {
                    name,
                    slug,
                    status,
                    is_running,
                    image,
                    cpu: if is_running { "0.1%".to_string() } else { "0% (Stopped)".to_string() },
                    mem: if is_running { "idle".to_string() } else { "0 MB (Stopped)".to_string() },
                });
            }
        }

        list
    }

    pub fn stop_container(slug: &str) -> Result<String, String> {
        let name = format!("connector-{}", slug);
        let res = Command::new("podman")
            .args(["stop", "-t", "2", &name])
            .output();
        match res {
            Ok(o) if o.status.success() => Ok(format!("Kontainer '{}' dihentikan.", slug)),
            Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn start_container(slug: &str) -> Result<String, String> {
        let name = format!("connector-{}", slug);
        let res = Command::new("podman")
            .args(["start", &name])
            .output();
        match res {
            Ok(o) if o.status.success() => Ok(format!("Kontainer '{}' dijalankan.", slug)),
            Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn restart_container(slug: &str) -> Result<String, String> {
        let name = format!("connector-{}", slug);
        let res = Command::new("podman")
            .args(["restart", "-t", "2", &name])
            .output();
        match res {
            Ok(o) if o.status.success() => Ok(format!("Kontainer '{}' di-restart.", slug)),
            Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn delete_container(slug: &str) -> Result<String, String> {
        let name = format!("connector-{}", slug);
        let res = Command::new("podman")
            .args(["rm", "-f", &name])
            .output();
        match res {
            Ok(o) if o.status.success() => Ok(format!("Kontainer '{}' berhasil dihapus.", slug)),
            Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
            Err(e) => Err(e.to_string()),
        }
    }
}

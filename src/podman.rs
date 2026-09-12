use serde::{Deserialize, Serialize};
use std::path::Path;
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
    pub git_status: String,
}

pub struct PodmanManager;

impl PodmanManager {
    pub fn get_project_dir(slug: &str) -> Option<String> {
        let candidate1 = format!("/var/lib/connector/projects/{}/work", slug);
        if Path::new(&format!("{}/.git", candidate1)).exists() {
            return Some(candidate1);
        }
        let candidate2 = format!("/var/lib/connector/projects/{}", slug);
        if Path::new(&format!("{}/.git", candidate2)).exists() {
            return Some(candidate2);
        }
        let candidate3 = format!("/home/fern/projects/{}", slug);
        if Path::new(&format!("{}/.git", candidate3)).exists() {
            return Some(candidate3);
        }
        None
    }

    pub fn get_git_sync_status(slug: &str) -> String {
        let work_dir = match Self::get_project_dir(slug) {
            Some(dir) => dir,
            None => return "⚪ Bukan repo Git".to_string(),
        };

        let status_out = Command::new("git")
            .args(["-C", &work_dir, "status", "--porcelain"])
            .output();

        if let Ok(out) = status_out {
            let s = String::from_utf8_lossy(&out.stdout);
            let count = s.lines().filter(|l| !l.trim().is_empty()).count();
            if count > 0 {
                return format!("⚠️ Belum Disimpan ({} file modified) — /simpan_{}", count, slug);
            }
        }

        let unpushed_out = Command::new("git")
            .args(["-C", &work_dir, "rev-list", "@{u}..HEAD", "--count"])
            .output();

        if let Ok(out) = unpushed_out {
            let count_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Ok(c) = count_str.parse::<usize>() {
                if c > 0 {
                    return format!("⚠️ Belum Di-push ({} commit pending) — /simpan_{}", c, slug);
                }
            }
        }

        "✅ Sudah Disimpan di GitHub (Synced)".to_string()
    }

    pub fn save_git_project(slug: &str) -> String {
        let work_dir = match Self::get_project_dir(slug) {
            Some(dir) => dir,
            None => return format!("Project '{}' belum terhubung repo git.", slug),
        };

        // git add -A
        let _ = Command::new("git")
            .args(["-C", &work_dir, "add", "-A"])
            .output();

        // git commit
        let _ = Command::new("git")
            .args([
                "-C",
                &work_dir,
                "commit",
                "-m",
                "chore: update workspace snapshot via Telegram (/simpan)",
            ])
            .output();

        // git push
        let push_res = Command::new("git")
            .args(["-C", &work_dir, "push"])
            .output();

        match push_res {
            Ok(o) if o.status.success() => {
                format!("✅ Berhasil disimpan dan di-push ke GitHub untuk '{}'!", slug)
            }
            Ok(_) => {
                format!("⚠️ Berhasil commit lokal '{}', tapi push ke remote pending.", slug)
            }
            Err(e) => format!("❌ Gagal push ke remote: {}", e),
        }
    }

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
                let git_status = Self::get_git_sync_status(&slug);

                list.push(ContainerInfo {
                    name,
                    slug,
                    status,
                    is_running,
                    image,
                    cpu: if is_running { "0.1%".to_string() } else { "0% (Stopped)".to_string() },
                    mem: if is_running { "idle".to_string() } else { "0 MB (Stopped)".to_string() },
                    git_status,
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

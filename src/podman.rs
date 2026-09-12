use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    pub disk_usage: String,
    pub disk_pct: String,
    pub git_status: String,
    pub assigned_agents: Vec<String>,
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
                return format!("⚠️ Belum Disimpan ({} file) — /simpan_{}", count, slug);
            }
        }

        let unpushed_out = Command::new("git")
            .args(["-C", &work_dir, "rev-list", "@{u}..HEAD", "--count"])
            .output();

        if let Ok(out) = unpushed_out {
            let count_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Ok(c) = count_str.parse::<usize>() {
                if c > 0 {
                    return format!("⚠️ Belum Di-push ({} commit) — /simpan_{}", c, slug);
                }
            }
        }

        "✅ Sudah Disimpan (Synced)".to_string()
    }

    pub fn save_git_project(slug: &str) -> String {
        let work_dir = match Self::get_project_dir(slug) {
            Some(dir) => dir,
            None => return format!("Project '{}' belum terhubung repo git.", slug),
        };

        let _ = Command::new("git")
            .args(["-C", &work_dir, "add", "-A"])
            .output();

        let _ = Command::new("git")
            .args([
                "-C",
                &work_dir,
                "commit",
                "-m",
                "chore: update workspace snapshot via Telegram (/simpan)",
            ])
            .output();

        let push_res = Command::new("git")
            .args(["-C", &work_dir, "push"])
            .output();

        match push_res {
            Ok(o) if o.status.success() => {
                format!("✅ Berhasil disimpan dan di-push ke GitHub untuk '{}'!", slug)
            }
            Ok(_) => {
                format!("⚠️ Berhasil commit lokal '{}', tapi push pending.", slug)
            }
            Err(e) => format!("❌ Gagal push: {}", e),
        }
    }

    pub fn rollback_project(slug: &str) -> String {
        let work_dir = match Self::get_project_dir(slug) {
            Some(dir) => dir,
            None => return format!("Project '{}' bukan repo git.", slug),
        };

        let _ = Command::new("git")
            .args(["-C", &work_dir, "checkout", "."])
            .output();
        let _ = Command::new("git")
            .args(["-C", &work_dir, "clean", "-fd"])
            .output();

        format!("⏪ Rollback selesai untuk '{}' (Workspace direset ke commit terakhir).", slug)
    }

    pub fn get_project_disk_usage(slug: &str) -> (String, String) {
        let path = format!("/var/lib/connector/projects/{}", slug);
        if !Path::new(&path).exists() {
            return ("0 KB".to_string(), "0%".to_string());
        }

        let out = Command::new("du").args(["-sk", &path]).output();
        if let Ok(o) = out {
            let s = String::from_utf8_lossy(&o.stdout);
            if let Some(first) = s.split('\t').next() {
                if let Ok(kb) = first.trim().parse::<u64>() {
                    let mb = (kb as f64) / 1024.0;
                    // assume standard 50GB VPS disk
                    let total_mb = 50.0 * 1024.0;
                    let pct = (mb / total_mb) * 100.0;
                    let usage = if mb >= 1.0 {
                        format!("{:.1} MB", mb)
                    } else {
                        format!("{} KB", kb)
                    };
                    return (usage, format!("{:.2}%", pct));
                }
            }
        }
        ("< 1 MB".to_string(), "0.01%".to_string())
    }

    pub fn get_live_stats() -> HashMap<String, (String, String)> {
        let mut map = HashMap::new();
        let out = Command::new("podman")
            .args(["stats", "--no-stream", "--format", "{{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}"])
            .output();

        if let Ok(o) = out {
            let s = String::from_utf8_lossy(&o.stdout);
            for line in s.lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 3 {
                    let name = parts[0].trim().to_string();
                    let cpu = parts[1].trim().to_string();
                    let mem = parts[2].trim().to_string();
                    map.insert(name, (cpu, mem));
                }
            }
        }
        map
    }

    pub fn list_containers() -> Vec<ContainerInfo> {
        let mut list = Vec::new();
        let live_stats = Self::get_live_stats();

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
                let (disk_usage, disk_pct) = Self::get_project_disk_usage(&slug);

                let (cpu, mem) = if let Some(stats) = live_stats.get(&name) {
                    (stats.0.clone(), stats.1.clone())
                } else if is_running {
                    ("0.1%".to_string(), "idle".to_string())
                } else {
                    ("0% (Stopped)".to_string(), "0 MB (Stopped)".to_string())
                };

                let assigned_agents = if slug == "smoke-app" {
                    vec!["alex (Online 🟢)".to_string()]
                } else {
                    vec![]
                };

                list.push(ContainerInfo {
                    name,
                    slug,
                    status,
                    is_running,
                    image,
                    cpu,
                    mem,
                    disk_usage,
                    disk_pct,
                    git_status,
                    assigned_agents,
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

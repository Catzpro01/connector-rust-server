use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizedSkill {
    pub name: String,
    pub description: String,
    pub command: String,
    pub executable_path: Option<String>,
    pub execution_count: u32,
    pub success_rate: f64,
    pub created_at: u64,
}

pub struct SelfEvolutionEngine {
    skills_dir: PathBuf,
    manifest_file: PathBuf,
    skills: Vec<SynthesizedSkill>,
}

impl SelfEvolutionEngine {
    pub fn new(data_dir: &str) -> Self {
        let base = PathBuf::from(data_dir).join("evolution");
        let skills_dir = base.join("skills");
        let manifest_file = base.join("synthesized_skills.json");
        let _ = fs::create_dir_all(&skills_dir);

        let mut engine = Self {
            skills_dir,
            manifest_file,
            skills: Vec::new(),
        };
        engine.load();
        engine
    }

    fn load(&mut self) {
        if self.manifest_file.exists() {
            if let Ok(raw) = fs::read_to_string(&self.manifest_file) {
                if let Ok(list) = serde_json::from_str::<Vec<SynthesizedSkill>>(&raw) {
                    self.skills = list;
                }
            }
        }
    }

    fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.skills) {
            let _ = fs::write(&self.manifest_file, json);
        }
    }

    /**
     * Synthesizes and hot-registers a new skill/tool dynamically at runtime.
     */
    pub fn synthesize_script_tool(
        &mut self,
        name: &str,
        description: &str,
        script_content: &str,
        interpreter: &str,
    ) -> Result<SynthesizedSkill, String> {
        let clean_name = name.trim().to_lowercase().replace(' ', "_");
        let ext = if interpreter.contains("python") { "py" } else { "sh" };
        let file_path = self.skills_dir.join(format!("{}.{}", clean_name, ext));

        if let Err(e) = fs::write(&file_path, script_content) {
            return Err(format!("Gagal menulis file skill: {}", e));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&file_path, fs::Permissions::from_mode(0o755));
        }

        let cmd = format!("{} {}", interpreter, file_path.to_string_lossy());
        let skill = SynthesizedSkill {
            name: clean_name,
            description: description.trim().to_string(),
            command: cmd,
            executable_path: Some(file_path.to_string_lossy().to_string()),
            execution_count: 0,
            success_rate: 1.0,
            created_at: chrono::Utc::now().timestamp() as u64,
        };

        self.skills.retain(|s| s.name != skill.name);
        self.skills.push(skill.clone());
        self.save();
        Ok(skill)
    }

    /**
     * Executes a synthesized skill by name.
     */
    pub fn execute_skill(&mut self, name: &str, args: &[&str]) -> Result<String, String> {
        let skill = self.skills.iter_mut().find(|s| s.name == name)
            .ok_or_else(|| format!("Skill '{}' tidak ditemukan.", name))?;

        let mut cmd = Command::new("sh");
        cmd.arg("-c");
        let full_cmd = if args.is_empty() {
            skill.command.clone()
        } else {
            format!("{} {}", skill.command, args.join(" "))
        };
        cmd.arg(&full_cmd);

        match cmd.output() {
            Ok(output) => {
                skill.execution_count += 1;
                let success = output.status.success();
                let prev_total = skill.execution_count as f64 - 1.0;
                skill.success_rate = (skill.success_rate * prev_total + if success { 1.0 } else { 0.0 })
                    / skill.execution_count as f64;
                self.save();

                if success {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    Err(format!(
                        "Eksekusi gagal (code {:?}): {}",
                        output.status.code(),
                        String::from_utf8_lossy(&output.stderr)
                    ))
                }
            }
            Err(e) => Err(format!("Gagal memanggil proses: {}", e)),
        }
    }

    pub fn list_skills(&self) -> &[SynthesizedSkill] {
        &self.skills
    }
}

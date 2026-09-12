use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinterViolation {
    pub line: usize,
    pub rule: String,
    pub snippet: String,
    pub suggestion: String,
}

pub struct MattPocockLinter;

impl MattPocockLinter {
    /**
     * Scans TypeScript code for Matt Pocock SOP violations.
     */
    pub fn scan_ts_code(code: &str) -> Vec<LinterViolation> {
        let mut violations = Vec::new();
        let lines: Vec<&str> = code.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();

            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
                continue;
            }

            // Rule 1: No 'any'
            if (trimmed.contains(": any") || trimmed.contains("<any>") || trimmed.contains("as any"))
                && !trimmed.contains("/* allow-any */")
            {
                violations.push(LinterViolation {
                    line: line_num,
                    rule: "MATTPOCOCK-001: Zero Any Policy".to_string(),
                    snippet: trimmed.to_string(),
                    suggestion: "Ganti 'any' dengan 'unknown' dan gunakan type guard / narrowing.".to_string(),
                });
            }

            // Rule 2: Unsafe double casting
            if trimmed.contains("as unknown as") {
                violations.push(LinterViolation {
                    line: line_num,
                    rule: "MATTPOCOCK-002: Avoid Double Casting".to_string(),
                    snippet: trimmed.to_string(),
                    suggestion: "Gunakan discriminated union atau refaktor struktur data agar tipe selaras tanpa casting paksa.".to_string(),
                });
            }

            // Rule 3: Function with no return type on exports
            if (trimmed.starts_with("export function ") || trimmed.starts_with("export const "))
                && trimmed.contains(" = (")
                && !trimmed.contains("): ")
            {
                violations.push(LinterViolation {
                    line: line_num,
                    rule: "MATTPOCOCK-003: Explicit Export Return Type".to_string(),
                    snippet: trimmed.to_string(),
                    suggestion: "Berikan tipe kembalian eksplisit pada fungsi yang di-export.".to_string(),
                });
            }
        }

        violations
    }
}

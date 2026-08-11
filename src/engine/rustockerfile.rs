use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    Download { url: String, alias: String },
    From(String),
    Copy { src: String, dst: String },
    Run(String),
    Cmd { args: Vec<String> },
    CpuLimit(f64),
    MemoryLimit(String),
}

#[derive(Debug, Default)]
pub struct Rustockerfile {
    pub instructions: Vec<Instruction>,
}

pub fn parse_memory_limit(s: &str) -> Result<f64, String> {
    let lower = s.trim().to_lowercase();

    let (num_part, mult): (&str, f64) = if let Some(n) = lower.strip_suffix('g') {
        (n, 1024.0 * 1024.0 * 1024.0)
    } else if let Some(n) = lower.strip_suffix('m') {
        (n, 1024.0 * 1024.0)
    } else if let Some(n) = lower.strip_suffix('k') {
        (n, 1024.0)
    } else if let Some(n) = lower.strip_suffix('b') {
        (n, 1.0)
    } else {
        (lower.as_str(), 1.0)
    };

    num_part
        .trim()
        .parse::<f64>()
        .map(|n| n * mult)
        .map_err(|_| {
            format!(
                "Invalid MEMORY_LIMIT value: '{}' (expected e.g. '512m', '2g', or a byte count)",
                s
            )
        })
}

impl Rustockerfile {
    pub fn parse_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Error reading file: {:?}", e))?;

        Self::parse(&content)
    }

    pub fn parse(content: &str) -> Result<Self, String> {
        let mut instructions = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let mut parts = trimmed.splitn(2, char::is_whitespace);
            let keyword = parts.next().unwrap_or("").to_uppercase();
            let args = parts.next().unwrap_or("").trim();

            match keyword.as_str() {
                "DOWNLOAD" => {
                    if args.is_empty() {
                        return Err(format!("Line {}: URL is required (DOWNLOAD)", line_num + 1));
                    }

                    let parts: Vec<&str> = args.split_whitespace().collect();
                    if parts.len() == 3 && parts[1].to_uppercase() == "AS" {
                        instructions.push(Instruction::Download {
                            url: parts[0].to_string(),
                            alias: parts[2].to_string(),
                        })
                    } else {
                        return Err(format!(
                            "Line {}: DOWNLOAD syntax requires format: DOWNLOAD <URL> AS <ALIAS>",
                            line_num + 1
                        ));
                    }
                }
                "FROM" => {
                    if args.is_empty() {
                        return Err(format!("Line {}: Image name required (FROM)", line_num + 1));
                    }
                    instructions.push(Instruction::From(args.to_string()));
                }
                "COPY" => {
                    let copy_parts: Vec<&str> = args.split_whitespace().collect();
                    if copy_parts.len() != 2 {
                        return Err(format!("Line {}: Copy requires 2 arguments", line_num + 1));
                    }

                    instructions.push(Instruction::Copy {
                        src: copy_parts[0].to_string(),
                        dst: copy_parts[1].to_string(),
                    })
                }
                "RUN" => {
                    if args.is_empty() {
                        return Err(format!("Line {}: Required argument (RUN)", line_num + 1));
                    }
                    instructions.push(Instruction::Run(args.to_string()));
                }
                "CMD" => {
                    if args.is_empty() {
                        return Err(format!("Line {}: Required argument (CMD)", line_num + 1));
                    }
                    let cmd_args: Vec<String> =
                        args.split_whitespace().map(|s| s.to_string()).collect();
                    instructions.push(Instruction::Cmd {
                        args: cmd_args
                    });
                }
                "CPU_LIMIT" => {
                    if args.is_empty() {
                        return Err(format!(
                            "Line {}: Required argument (CPU_LIMIT)",
                            line_num + 1
                        ));
                    }

                    let cores = args.parse::<f64>().map_err(|_| {
                        format!(
                            "Line {}: CPU_LIMIT must be a number (e.g. CPU_LIMIT 1.5)",
                            line_num + 1
                        )
                    })?;

                    if cores <= 0.0 {
                        return Err(format!(
                            "Line {}: CPU_LIMIT must be greater than 0",
                            line_num + 1
                        ));
                    }

                    instructions.push(Instruction::CpuLimit(cores));
                }
                "MEMORY_LIMIT" => {
                    if args.is_empty() {
                        return Err(format!(
                            "Line {}: Required argument (MEMORY_LIMIT)",
                            line_num + 1
                        ));
                    }

                    parse_memory_limit(args)
                        .map_err(|e| format!("Line {}: {}", line_num + 1, e))?;
                    instructions.push(Instruction::MemoryLimit(args.to_string()));
                }
                _ => {
                    return Err(format!("Line {}: Unknown keyword", line_num + 1));
                }
            }
        }

        Ok(Rustockerfile { instructions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_instruction_types() {
        let content = "\
DOWNLOAD https://example.com/alpine.tar.gz AS alpine-base
FROM alpine-base
COPY src /app
RUN echo hello
CMD /bin/sh -c
";
        let parsed = Rustockerfile::parse(content).unwrap();
        assert_eq!(
            parsed.instructions,
            vec![
                Instruction::Download {
                    url: "https://example.com/alpine.tar.gz".to_string(),
                    alias: "alpine-base".to_string(),
                },
                Instruction::From("alpine-base".to_string()),
                Instruction::Copy {
                    src: "src".to_string(),
                    dst: "/app".to_string()
                },
                Instruction::Run("echo hello".to_string()),
                Instruction::Cmd {
                    args: vec!["/bin/sh".to_string(), "-c".to_string()]
                },
            ]
        );
    }

    #[test]
    fn keywords_are_case_insensitive() {
        let parsed = Rustockerfile::parse("from ubuntu\ncopy a /b\nrun ls\n").unwrap();
        assert_eq!(
            parsed.instructions,
            vec![
                Instruction::From("ubuntu".to_string()),
                Instruction::Copy {
                    src: "a".to_string(),
                    dst: "/b".to_string()
                },
                Instruction::Run("ls".to_string()),
            ]
        );
    }

    #[test]
    fn ignores_blank_lines_and_comments() {
        let content = "\n# header comment\nFROM alpine\n\n# another comment\nRUN true\n";
        let parsed = Rustockerfile::parse(content).unwrap();
        assert_eq!(parsed.instructions.len(), 2);
    }

    #[test]
    fn empty_file_yields_no_instructions() {
        let parsed = Rustockerfile::parse("\n  \n# only a comment\n").unwrap();
        assert!(parsed.instructions.is_empty());
    }

    #[test]
    fn unknown_keyword_reports_line_number() {
        let err = Rustockerfile::parse("FROM alpine\nMAYBE ls\n").unwrap_err();
        assert!(err.contains("Line 2"), "unexpected error: {}", err);
        assert!(err.contains("Unknown keyword"));
    }

    #[test]
    fn from_requires_argument() {
        let err = Rustockerfile::parse("FROM\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("FROM"));
    }

    #[test]
    fn download_requires_as_syntax() {
        let err = Rustockerfile::parse("DOWNLOAD https://example.com/x.tar.gz\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("DOWNLOAD <URL> AS <ALIAS>"));

        let err = Rustockerfile::parse("DOWNLOAD https://example.com/x.tar.gz WRONG alias\n")
            .unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("DOWNLOAD <URL> AS <ALIAS>"));
    }

    #[test]
    fn copy_requires_exactly_two_arguments() {
        let err = Rustockerfile::parse("COPY only_one\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("2 arguments"));

        let err = Rustockerfile::parse("COPY a b c\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("2 arguments"));
    }

    #[test]
    fn run_requires_argument() {
        let err = Rustockerfile::parse("RUN\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("RUN"));
    }

    #[test]
    fn cmd_requires_argument() {
        let err = Rustockerfile::parse("CMD\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("CMD"));
    }

    #[test]
    fn preserves_run_command_including_spaces() {
        let parsed = Rustockerfile::parse("RUN apk add --no-cache curl && echo done\n").unwrap();
        assert_eq!(
            parsed.instructions,
            vec![Instruction::Run(
                "apk add --no-cache curl && echo done".to_string()
            ),]
        );
    }

    #[test]
    fn parse_from_file_reads_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Rustockerfile");
        std::fs::write(&path, "FROM ubuntu\n").unwrap();

        let parsed = Rustockerfile::parse_from_file(&path).unwrap();
        assert_eq!(
            parsed.instructions,
            vec![Instruction::From("ubuntu".to_string())]
        );
    }

    #[test]
    fn parse_from_file_missing_file_errors() {
        let err = Rustockerfile::parse_from_file("/does/not/exist/Rustockerfile").unwrap_err();
        assert!(err.contains("Error reading file"));
    }

    #[test]
    fn cmd_splits_command_and_args() {
        let parsed = Rustockerfile::parse("CMD /bin/sh -c echo hello\n").unwrap();
        assert_eq!(
            parsed.instructions,
            vec![Instruction::Cmd {
                args: vec!["/bin/sh".to_string(), "-c".to_string(), "echo".to_string(), "hello".to_string()],
            },]
        );
    }

    #[test]
    fn cmd_with_single_word_has_empty_args() {
        let parsed = Rustockerfile::parse("CMD ls\n").unwrap();
        assert_eq!(
            parsed.instructions,
            vec![Instruction::Cmd {
                args: vec!["ls".to_string()]
            },]
        );
    }

    #[test]
    fn cmd_splits_on_all_whitespace() {
        let parsed = Rustockerfile::parse("CMD   python3   -m   http.server\n").unwrap();
        assert_eq!(
            parsed.instructions,
            vec![Instruction::Cmd {
                args: vec!["python3".to_string(), "-m".to_string(), "http.server".to_string()],
            },]
        );
    }

    #[test]
    fn cpu_limit_parses_and_validates() {
        let parsed = Rustockerfile::parse("CPU_LIMIT 1.5\n").unwrap();
        assert_eq!(parsed.instructions, vec![Instruction::CpuLimit(1.5)]);

        let err = Rustockerfile::parse("CPU_LIMIT 0\n").unwrap_err();
        assert!(err.contains("greater than 0"), "unexpected error: {}", err);

        let err = Rustockerfile::parse("CPU_LIMIT abc\n").unwrap_err();
        assert!(
            err.contains("must be a number"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn memory_limit_parses_and_records_raw_value() {
        let parsed = Rustockerfile::parse("MEMORY_LIMIT 2g\n").unwrap();
        assert_eq!(
            parsed.instructions,
            vec![Instruction::MemoryLimit("2g".to_string())]
        );
    }

    #[test]
    fn memory_limit_validates_value() {
        let err = Rustockerfile::parse("MEMORY_LIMIT not-a-size\n").unwrap_err();
        assert!(err.contains("Line 1"), "unexpected error: {}", err);
        assert!(
            err.contains("Invalid MEMORY_LIMIT"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn parse_memory_limit_handles_units() {
        assert_eq!(parse_memory_limit("512m").unwrap(), 512.0 * 1024.0 * 1024.0);
        assert_eq!(
            parse_memory_limit("2g").unwrap(),
            2.0 * 1024.0 * 1024.0 * 1024.0
        );
        assert_eq!(parse_memory_limit("1024k").unwrap(), 1024.0 * 1024.0);
        assert_eq!(parse_memory_limit("8b").unwrap(), 8.0);
        assert_eq!(parse_memory_limit("100").unwrap(), 100.0);
    }

    #[test]
    fn parse_memory_limit_is_case_insensitive() {
        assert_eq!(
            parse_memory_limit("2G").unwrap(),
            parse_memory_limit("2g").unwrap()
        );
        assert_eq!(
            parse_memory_limit("512M").unwrap(),
            parse_memory_limit("512m").unwrap()
        );
    }

    #[test]
    fn parse_memory_limit_rejects_invalid_values() {
        assert!(parse_memory_limit("").is_err());
        assert!(parse_memory_limit("abc").is_err());
        assert!(parse_memory_limit("2x").is_err());
    }
}
